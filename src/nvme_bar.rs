use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::io::AsRawFd;
use std::ptr;

use crate::nvme_spec::{ACQ, AQA, ASQ, CAP, CC, CSTS, DOORBELL_BASE};

// Invariant: `address` points to `length` bytes of a live mmap of the device BAR,
// established by map(), torn down only by Drop. Accessors rely on it plus check() for
// bounds and alignment. They cannot defend against the device mutating a register
// concurrently (a data race), which works only because aligned word accesses to MMIO are
// atomic on this target.
pub struct NvmeBar {
    address: *mut u8,
    length: usize,
    _file: File,
}

impl NvmeBar {
    pub fn map(bdf: &str, index: u8) -> io::Result<Self> {
        let path = format!("/sys/bus/pci/devices/{bdf}/resource{index}");
        let file = OpenOptions::new().read(true).write(true).open(&path)?;
        let length = file.metadata()?.len() as usize;
        // SAFETY: valid fd for the BAR resource, kernel-chosen address; MAP_FAILED checked
        // below. The fd lives in `_file` as long as the mapping.
        let address = unsafe {
            libc::mmap(
                ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if address == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        chat!(
            "  BAR{index}: mmap'd {path} ({length} bytes of MMIO) at {address:p}; \
             register reads/writes now go straight to the controller over PCIe"
        );
        Ok(NvmeBar {
            address: address as *mut u8,
            length,
            _file: file,
        })
    }

    fn check(&self, offset: usize, size: usize) {
        let end = offset
            .checked_add(size)
            .expect("BAR access offset overflow");
        assert!(
            end <= self.length,
            "BAR access at {offset:#x} (size {size}) exceeds length {:#x}",
            self.length
        );
        assert!(
            offset % size == 0,
            "BAR access at {offset:#x} is not {size}-byte aligned"
        );
    }

    pub fn read_capabilities(&self) -> u64 {
        let value = self.read64(CAP);
        let max_queue_entries = (value & 0xFFFF) + 1; // MQES is zero-based
        let stride = 4usize << ((value >> 32) & 0xF); // DSTRD, in bytes
        let timeout_ms = ((value >> 24) & 0xFF) * 500; // TO, in 500 ms units
        chat!(
            "  r64 CAP   @{CAP:#06x} = {value:#018x} \
             (MQES {max_queue_entries} entries, doorbell stride {stride} B, CAP.TO {timeout_ms} ms)"
        );
        value
    }

    pub fn read_configuration(&self) -> u32 {
        let value = self.read32(CC);
        chat!("  r32 CC    @{CC:#06x} = {value:#010x} (controller configuration)");
        value
    }

    pub fn write_configuration(&self, value: u32) {
        let enabled = value & 1;
        let io_submission_entry_size = (value >> 16) & 0xF; // IOSQES, 2^n bytes
        let io_completion_entry_size = (value >> 20) & 0xF; // IOCQES, 2^n bytes
        chat!(
            "  w32 CC    @{CC:#06x} = {value:#010x} \
             (CC.EN={enabled}, IOSQES=2^{io_submission_entry_size}, IOCQES=2^{io_completion_entry_size})"
        );
        self.write32(CC, value);
    }

    pub fn read_status(&self) -> u32 {
        // No chat! here: the caller polls this register in a tight loop while waiting on
        // CSTS.RDY, so it narrates the transition once rather than every read.
        self.read32(CSTS)
    }

    pub fn write_admin_queue_attributes(&self, value: u32) {
        let submission_size = (value & 0xFFF) + 1; // ASQS, zero-based
        let completion_size = ((value >> 16) & 0xFFF) + 1; // ACQS, zero-based
        chat!(
            "  w32 AQA   @{AQA:#06x} = {value:#010x} \
             (admin SQ {submission_size} entries, admin CQ {completion_size} entries)"
        );
        self.write32(AQA, value);
    }

    pub fn write_admin_submission_queue(&self, physical: u64) {
        chat!("  w64 ASQ   @{ASQ:#06x} = {physical:#018x} (admin submission queue base, physical)");
        self.write64(ASQ, physical);
    }

    pub fn write_admin_completion_queue(&self, physical: u64) {
        chat!("  w64 ACQ   @{ACQ:#06x} = {physical:#018x} (admin completion queue base, physical)");
        self.write64(ACQ, physical);
    }

    pub fn ring_submission_doorbell(&self, queue_id: u16, stride: usize, value: u32) {
        let offset = DOORBELL_BASE + (2 * queue_id as usize) * stride;
        chat!(
            "  w32 SQ{queue_id}DB @{offset:#06x} = {value} \
             (ring submission doorbell: new tail, controller may now fetch the command)"
        );
        self.write32(offset, value);
    }

    pub fn ring_completion_doorbell(&self, queue_id: u16, stride: usize, value: u32) {
        let offset = DOORBELL_BASE + (2 * queue_id as usize + 1) * stride;
        chat!(
            "  w32 CQ{queue_id}DB @{offset:#06x} = {value} \
             (ring completion doorbell: head advanced, entry slot freed)"
        );
        self.write32(offset, value);
    }

    fn read32(&self, offset: usize) -> u32 {
        self.check(offset, 4);
        // SAFETY: check() proved offset+4 is in bounds and aligned. Volatile because it
        // targets a device register.
        unsafe { (self.address.add(offset) as *const u32).read_volatile() }
    }

    fn write32(&self, offset: usize, value: u32) {
        self.check(offset, 4);
        // SAFETY: as read32. Volatile because it targets a device register.
        unsafe { (self.address.add(offset) as *mut u32).write_volatile(value) }
    }

    fn read64(&self, offset: usize) -> u64 {
        self.check(offset, 8);
        // SAFETY: same as read32, with check() proving 8-byte bounds and alignment.
        unsafe { (self.address.add(offset) as *const u64).read_volatile() }
    }

    fn write64(&self, offset: usize, value: u64) {
        self.check(offset, 8);
        // SAFETY: same as write32, with check() proving 8-byte bounds and alignment.
        unsafe { (self.address.add(offset) as *mut u64).write_volatile(value) }
    }
}

impl Drop for NvmeBar {
    fn drop(&mut self) {
        // SAFETY: address/length still describe the mapping; Drop runs once.
        unsafe {
            libc::munmap(self.address as *mut libc::c_void, self.length);
        }
    }
}
