use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::mem::{align_of, size_of};
use std::ptr;

use zerocopy::FromBytes;

pub const HUGE_PAGE_SIZE: usize = 2 * 1024 * 1024;
const PAGE_SIZE: usize = 4096;

// Invariant: `virtual_address` points to `length` bytes of a live, mlock'd hugepage
// mapping, and `physical_address` is the bus address backing it. Established by
// alloc_huge(), torn down only by Drop. The CPU uses the virtual address; the controller
// (no IOMMU here) is handed the physical one. Accessors check bounds and alignment via
// check_bytes/check_typed.
pub struct DmaRegion {
    virtual_address: *mut u8,
    length: usize,
    physical_address: u64,
}

impl DmaRegion {
    pub fn alloc_huge() -> io::Result<Self> {
        let length = HUGE_PAGE_SIZE;
        // SAFETY: anonymous hugepage mmap, kernel-chosen address; MAP_FAILED checked below.
        let virtual_address = unsafe {
            libc::mmap(
                ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_HUGETLB,
                -1,
                0,
            )
        };
        if virtual_address == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        let virtual_address = virtual_address as *mut u8;
        chat!(
            "  mmap'd {} MiB hugepage DMA region at virtual {virtual_address:p}",
            length / (1024 * 1024)
        );
        // SAFETY: the freshly mapped `length` bytes. Zeroing also faults the pages in,
        // required before virtual_to_physical reads the pagemap.
        unsafe {
            ptr::write_bytes(virtual_address, 0, length);
            if libc::mlock(virtual_address as *const libc::c_void, length) != 0 {
                let error = io::Error::last_os_error();
                libc::munmap(virtual_address as *mut libc::c_void, length);
                return Err(error);
            }
        }
        chat!("  zeroed and mlock'd the region (faults every page in and pins it; no swapping, no migration)");
        let physical_address = virtual_to_physical(virtual_address as usize)?;
        chat!(
            "  walked /proc/self/pagemap: virtual {virtual_address:p} is backed by physical \
             {physical_address:#018x}. With iommu=off the controller DMAs to that bus address directly"
        );
        Ok(DmaRegion {
            virtual_address,
            length,
            physical_address,
        })
    }

    pub fn physical_address_at(&self, offset: usize) -> u64 {
        self.physical_address + offset as u64
    }

    pub fn write_volatile<T: Copy>(&self, offset: usize, value: T) {
        self.check_typed::<T>(offset);
        // SAFETY: check_typed proved offset is in bounds and aligned for T; the invariant
        // keeps the mapping live. Volatile so the controller sees the store.
        unsafe { (self.virtual_address.add(offset) as *mut T).write_volatile(value) };
    }

    pub fn read_volatile<T: FromBytes>(&self, offset: usize) -> T {
        self.check_typed::<T>(offset);
        // SAFETY: check_typed proved bounds and alignment, FromBytes makes any bit pattern
        // a valid T, volatile forces a fresh load. NOT discharged: the controller writes
        // this slot concurrently (a data race); sound only if aligned word reads are
        // tear-free and the caller drops entries with the wrong phase tag.
        unsafe { (self.virtual_address.add(offset) as *const T).read_volatile() }
    }

    pub fn read_bytes(&self, offset: usize, destination: &mut [u8]) {
        self.check_bytes(offset, destination.len());
        // SAFETY: check_bytes proved [offset, offset+len) is in bounds; source and
        // destination are distinct allocations.
        unsafe {
            ptr::copy_nonoverlapping(
                self.virtual_address.add(offset),
                destination.as_mut_ptr(),
                destination.len(),
            );
        }
    }

    pub fn write_bytes(&self, offset: usize, source: &[u8]) {
        self.check_bytes(offset, source.len());
        // SAFETY: check_bytes proved [offset, offset+len) is in bounds; source and
        // destination are distinct allocations.
        unsafe {
            ptr::copy_nonoverlapping(
                source.as_ptr(),
                self.virtual_address.add(offset),
                source.len(),
            );
        }
    }

    fn check_typed<T>(&self, offset: usize) {
        self.check_bytes(offset, size_of::<T>());
        assert!(
            offset % align_of::<T>() == 0,
            "DMA access at {offset:#x} is not aligned for a {}-byte type",
            align_of::<T>()
        );
    }

    fn check_bytes(&self, offset: usize, size: usize) {
        let end = offset
            .checked_add(size)
            .expect("DMA access offset overflow");
        assert!(
            end <= self.length,
            "DMA access at {offset:#x} (size {size}) exceeds length {:#x}",
            self.length
        );
    }
}

impl Drop for DmaRegion {
    fn drop(&mut self) {
        // SAFETY: virtual_address/length still describe the mapping; Drop runs once.
        // munmap also drops the mlock.
        unsafe {
            libc::munmap(self.virtual_address as *mut libc::c_void, self.length);
        }
    }
}

fn virtual_to_physical(virtual_address: usize) -> io::Result<u64> {
    let mut file = File::open("/proc/self/pagemap")?;
    file.seek(SeekFrom::Start(((virtual_address / PAGE_SIZE) * 8) as u64))?;
    let mut buffer = [0u8; 8];
    file.read_exact(&mut buffer)?;
    let entry = u64::from_le_bytes(buffer);
    if entry & (1 << 63) == 0 {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "page not present in pagemap (need root, and the page must be faulted in)",
        ));
    }
    let page_frame_number = entry & ((1u64 << 55) - 1);
    Ok(page_frame_number * PAGE_SIZE as u64 + (virtual_address % PAGE_SIZE) as u64)
}
