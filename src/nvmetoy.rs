use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::mem::size_of;
use std::thread::sleep;
use std::time::Duration;

use crate::dma_region::{DmaRegion, HUGE_PAGE_SIZE};
use crate::nvme_bar::NvmeBar;
use crate::nvme_command_builders::{
    nvme_create_io_completion_queue, nvme_create_io_submission_queue, nvme_identify, nvme_read,
    nvme_write,
};
use crate::nvme_spec::{CompletionQueueEntry, IdentifyStructure};
use crate::queue_pair::{QueuePair, QUEUE_DEPTH};

const PAGE_SIZE: usize = 4096;
const IO_QUEUE_ID: u16 = 1;

const IO_SUBMISSION_ENTRY_SIZE_EXPONENT: u32 = 6; // 2^6 = 64-byte SubmissionQueueEntry
const IO_COMPLETION_ENTRY_SIZE_EXPONENT: u32 = 4; // 2^4 = 16-byte CompletionQueueEntry

const ADMIN_SUBMISSION_QUEUE_OFFSET: usize = 0x0000;
const ADMIN_COMPLETION_QUEUE_OFFSET: usize = 0x1000;
const IO_SUBMISSION_QUEUE_OFFSET: usize = 0x2000;
const IO_COMPLETION_QUEUE_OFFSET: usize = 0x3000;
const PRP_LIST_OFFSET: usize = 0x4000;
const DATA_OFFSET: usize = 0x5000;

pub struct NvmeUserspaceDriver {
    nvme_bar: NvmeBar,
    dma_region: DmaRegion,
    doorbell_stride: usize,
    admin_queue: QueuePair,
    io_queue: QueuePair,
    block_sizes: HashMap<u32, usize>,
    nsid: u32,
}

impl NvmeUserspaceDriver {
    pub fn open(bdf: &str, nsid: u32) -> io::Result<Self> {
        chat!("opening NVMe controller at PCI {bdf} from userspace");
        refuse_if_kernel_driver_bound(bdf)?;
        enable_bus_master(bdf)?;
        crate::pause();

        chat!("mapping the controller's register BAR (BAR0)...");
        let bar = NvmeBar::map(bdf, 0)?;
        chat!("allocating a pinned hugepage for queues, PRP lists, and data...");
        let dma = DmaRegion::alloc_huge()?;

        chat!("reading controller capabilities to learn the doorbell stride...");
        let capabilities = bar.read_capabilities();
        let doorbell_stride = 4usize << ((capabilities >> 32) & 0xF);
        chat!("doorbell stride is {doorbell_stride} bytes; the per-queue doorbell registers are spaced this far apart");
        crate::pause();

        let mut driver = NvmeUserspaceDriver {
            nvme_bar: bar,
            dma_region: dma,
            doorbell_stride,
            admin_queue: QueuePair::new(
                0,
                ADMIN_SUBMISSION_QUEUE_OFFSET,
                ADMIN_COMPLETION_QUEUE_OFFSET,
            ),
            io_queue: QueuePair::new(
                IO_QUEUE_ID,
                IO_SUBMISSION_QUEUE_OFFSET,
                IO_COMPLETION_QUEUE_OFFSET,
            ),
            block_sizes: HashMap::new(),
            nsid,
        };
        driver.reset_and_enable()?;
        crate::pause();
        driver.check_queue_entry_sizes()?;
        crate::pause();
        driver.create_io_queues()?;
        crate::pause();
        chat!("--- prelude: learning namespace {nsid}'s block size now, once, so later reads and writes reuse it instead of paying for an Identify each time ---");
        driver.namespace_block_size(nsid)?;
        chat!("controller is up: admin and I/O queues live, ready for commands");
        Ok(driver)
    }

    fn reset_and_enable(&mut self) -> io::Result<()> {
        chat!("--- resetting and re-enabling the controller ---");
        chat!("clearing CC.EN to ask the controller to reset...");
        let configuration = self.nvme_bar.read_configuration();
        self.nvme_bar.write_configuration(configuration & !1);
        chat!("waiting for CSTS.RDY to drop to 0 (reset acknowledged)...");
        self.wait_ready(false)?;

        chat!("programming the admin queues before re-enabling...");
        let admin_queue_attributes =
            (((QUEUE_DEPTH as u32) - 1) << 16) | ((QUEUE_DEPTH as u32) - 1);
        self.nvme_bar
            .write_admin_queue_attributes(admin_queue_attributes);
        self.nvme_bar
            .write_admin_submission_queue(self.admin_submission_queue_physical_address());
        self.nvme_bar
            .write_admin_completion_queue(self.admin_completion_queue_physical_address());

        chat!("setting CC.EN with the I/O queue entry sizes, then waiting for CSTS.RDY to come back up...");
        let configuration = (IO_COMPLETION_ENTRY_SIZE_EXPONENT << 20)
            | (IO_SUBMISSION_ENTRY_SIZE_EXPONENT << 16)
            | 1;
        self.nvme_bar.write_configuration(configuration);
        self.wait_ready(true)
    }

    fn wait_ready(&self, want_ready: bool) -> io::Result<()> {
        for spins in 0..1000 {
            let controller_status = self.nvme_bar.read_status();
            if controller_status & 0x2 != 0 {
                return Err(other("controller reported fatal status (CSTS.CFS)"));
            }
            if (controller_status & 0x1 != 0) == want_ready {
                chat!(
                    "  CSTS.RDY is now {} after {spins} polls (CSTS = {controller_status:#010x})",
                    want_ready as u8
                );
                return Ok(());
            }
            sleep(Duration::from_millis(5));
        }
        Err(other("timed out waiting for CSTS.RDY"))
    }

    fn check_queue_entry_sizes(&mut self) -> io::Result<()> {
        chat!("--- checking the controller accepts our 64-byte SQE / 16-byte CQE sizes ---");
        let data = self.identify(IdentifyStructure::Controller, 0)?;
        let submission = data[512]; // SQES: low nibble required min, high nibble max (2^n)
        let completion = data[513]; // CQES: low nibble required min, high nibble max (2^n)
        let required_submission = IO_SUBMISSION_ENTRY_SIZE_EXPONENT as u8;
        let required_completion = IO_COMPLETION_ENTRY_SIZE_EXPONENT as u8;

        if !((submission & 0xF)..=(submission >> 4)).contains(&required_submission) {
            return Err(other(&format!(
                "controller SQ entry size range 2^{}..=2^{} excludes the required 2^{required_submission}",
                submission & 0xF,
                submission >> 4
            )));
        }
        if !((completion & 0xF)..=(completion >> 4)).contains(&required_completion) {
            return Err(other(&format!(
                "controller CQ entry size range 2^{}..=2^{} excludes the required 2^{required_completion}",
                completion & 0xF,
                completion >> 4
            )));
        }
        chat!("  entry sizes accepted");
        Ok(())
    }

    fn create_io_queues(&mut self) -> io::Result<()> {
        chat!("--- creating I/O queue pair {IO_QUEUE_ID} via admin commands ---");
        chat!("the completion queue must exist before the submission queue that targets it");
        let cq = self.io_completion_queue_physical_address();
        let command = nvme_create_io_completion_queue(IO_QUEUE_ID, QUEUE_DEPTH as u16, cq);
        let completion = self.admin_queue.submit_and_wait(
            &self.nvme_bar,
            &self.dma_region,
            self.doorbell_stride,
            command,
        )?;
        check(&completion, "Create I/O Completion Queue")?;

        let sq = self.io_submission_queue_physical_address();
        let command =
            nvme_create_io_submission_queue(IO_QUEUE_ID, QUEUE_DEPTH as u16, IO_QUEUE_ID, sq);
        let completion = self.admin_queue.submit_and_wait(
            &self.nvme_bar,
            &self.dma_region,
            self.doorbell_stride,
            command,
        )?;
        check(&completion, "Create I/O Submission Queue")
    }

    fn identify(&mut self, structure: IdentifyStructure, nsid: u32) -> io::Result<[u8; 4096]> {
        chat!("issuing Identify (structure {structure:?}, nsid {nsid}) on the admin queue");
        let data = self.data_page_physical_address(0);
        let command = nvme_identify(structure, nsid, data);

        let completion = self.admin_queue.submit_and_wait(
            &self.nvme_bar,
            &self.dma_region,
            self.doorbell_stride,
            command,
        )?;
        check(&completion, "Identify")?;

        let mut output = [0u8; 4096];
        chat!("  copying the 4096-byte Identify result out of the DMA buffer");
        self.dma_region.read_bytes(DATA_OFFSET, &mut output);
        Ok(output)
    }

    pub fn identify_controller(&mut self) -> io::Result<[u8; 4096]> {
        self.identify(IdentifyStructure::Controller, 0)
    }

    pub fn namespace_block_size(&mut self, nsid: u32) -> io::Result<usize> {
        if let Some(&block_size) = self.block_sizes.get(&nsid) {
            return Ok(block_size);
        }
        let data = self.identify(IdentifyStructure::Namespace, nsid)?;
        let formatted_lba_size = (data[26] & 0xF) as usize;
        let lba_format = 128 + formatted_lba_size * 4;
        let lba_data_size_exponent = data[lba_format + 2];
        let block_size = 1usize << lba_data_size_exponent;
        chat!("namespace {nsid}: formatted LBA size index {formatted_lba_size} -> {block_size}-byte blocks");
        self.block_sizes.insert(nsid, block_size);
        Ok(block_size)
    }

    pub fn read(&mut self, slba: u64, block_count: u16) -> io::Result<Vec<u8>> {
        let nsid = self.nsid;
        chat!("=== READ nsid={nsid}, starting LBA {slba}, {block_count} block(s) ===");
        let block_size = self.namespace_block_size(nsid)?;
        let total = block_size * block_count as usize;
        if total > HUGE_PAGE_SIZE - DATA_OFFSET {
            return Err(other("read larger than the data buffer"));
        }

        let (prp1, prp2) = self.data_pointers(total);
        let command = nvme_read(nsid, slba, block_count - 1, prp1, prp2);

        let completion = self.io_queue.submit_and_wait(
            &self.nvme_bar,
            &self.dma_region,
            self.doorbell_stride,
            command,
        )?;
        check(&completion, "Read")?;

        let mut output = vec![0u8; total];
        chat!("  command done; copying {total} bytes out of the DMA buffer to the caller");
        self.dma_region.read_bytes(DATA_OFFSET, &mut output);
        Ok(output)
    }

    pub fn write(&mut self, slba: u64, data: &[u8]) -> io::Result<()> {
        let nsid = self.nsid;
        chat!(
            "=== WRITE nsid={nsid}, starting LBA {slba}, {} byte(s) ===",
            data.len()
        );
        let block_size = self.namespace_block_size(nsid)?;
        if data.is_empty() || data.len() % block_size != 0 {
            return Err(other(
                "write length must be a non-zero multiple of the block size",
            ));
        }
        if data.len() > HUGE_PAGE_SIZE - DATA_OFFSET {
            return Err(other("write larger than the data buffer"));
        }
        let block_count = data.len() / block_size;

        chat!("  copying {} bytes ({block_count} block(s)) into the DMA buffer for the controller to fetch", data.len());
        self.dma_region.write_bytes(DATA_OFFSET, data);
        let (prp1, prp2) = self.data_pointers(data.len());
        let command = nvme_write(nsid, slba, block_count as u16 - 1, prp1, prp2);

        let completion = self.io_queue.submit_and_wait(
            &self.nvme_bar,
            &self.dma_region,
            self.doorbell_stride,
            command,
        )?;
        check(&completion, "Write")
    }

    fn data_pointers(&self, length: usize) -> (u64, u64) {
        let prp1 = self.data_page_physical_address(0);
        let page_count = length.div_ceil(PAGE_SIZE);

        let prp2 = match page_count {
            0 | 1 => {
                chat!("  PRP: {length} bytes fit in one page; PRP1={prp1:#018x}, PRP2 unused");
                0
            }
            2 => {
                let second = self.data_page_physical_address(1);
                chat!("  PRP: {length} bytes span two pages; PRP1={prp1:#018x}, PRP2={second:#018x} (second page directly)");
                second
            }
            _ => {
                let list = self.prp_list_physical_address();
                chat!(
                    "  PRP: {length} bytes span {page_count} pages; PRP1={prp1:#018x}, PRP2={list:#018x} points at a {}-entry PRP list:",
                    page_count - 1
                );
                for index in 0..(page_count - 1) {
                    let page = self.data_page_physical_address(index + 1);
                    self.dma_region
                        .write_volatile(PRP_LIST_OFFSET + index * size_of::<u64>(), page);
                    chat!("    PRP list[{index}] = {page:#018x}  (data page {})", index + 1);
                }
                list
            }
        };
        (prp1, prp2)
    }

    fn admin_submission_queue_physical_address(&self) -> u64 {
        self.dma_region
            .physical_address_at(ADMIN_SUBMISSION_QUEUE_OFFSET)
    }

    fn admin_completion_queue_physical_address(&self) -> u64 {
        self.dma_region
            .physical_address_at(ADMIN_COMPLETION_QUEUE_OFFSET)
    }

    fn io_submission_queue_physical_address(&self) -> u64 {
        self.dma_region
            .physical_address_at(IO_SUBMISSION_QUEUE_OFFSET)
    }

    fn io_completion_queue_physical_address(&self) -> u64 {
        self.dma_region
            .physical_address_at(IO_COMPLETION_QUEUE_OFFSET)
    }

    fn data_page_physical_address(&self, page: usize) -> u64 {
        self.dma_region
            .physical_address_at(DATA_OFFSET + page * PAGE_SIZE)
    }

    fn prp_list_physical_address(&self) -> u64 {
        self.dma_region.physical_address_at(PRP_LIST_OFFSET)
    }
}

fn check(completion: &CompletionQueueEntry, what: &str) -> io::Result<()> {
    if completion.is_success() {
        return Ok(());
    }
    Err(other(&format!(
        "{what} failed: SCT={:#x} SC={:#x}",
        completion.status_code_type(),
        completion.status_code()
    )))
}

fn refuse_if_kernel_driver_bound(bdf: &str) -> io::Result<()> {
    chat!("checking no kernel driver is bound to {bdf} (we need exclusive control)...");
    let driver_link = format!("/sys/bus/pci/devices/{bdf}/driver");
    let target = match std::fs::read_link(&driver_link) {
        Ok(target) => target,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            chat!("  no driver bound, good");
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    let driver_name = target.file_name().and_then(|name| name.to_str());
    if driver_name == Some("nvme") {
        return Err(other(&format!(
            "{bdf} is bound to the kernel nvme driver; unbind it first \
             (resetting the controller under the live driver will hang it)"
        )));
    }
    chat!("  bound to {driver_name:?}, not the kernel nvme driver; proceeding");
    Ok(())
}

pub fn enable_bus_master(bdf: &str) -> io::Result<()> {
    chat!("setting PCI Bus Master + Memory Space in the command register of {bdf} (so the device can DMA)...");
    let path = format!("/sys/bus/pci/devices/{bdf}/config");
    let mut file = OpenOptions::new().read(true).write(true).open(&path)?;
    file.seek(SeekFrom::Start(0x04))?;
    let mut buffer = [0u8; 2];
    file.read_exact(&mut buffer)?;
    let before = u16::from_le_bytes(buffer);
    let command = before | (1 << 1) | (1 << 2);
    file.seek(SeekFrom::Start(0x04))?;
    file.write_all(&command.to_le_bytes())?;
    chat!("  PCI command register {before:#06x} -> {command:#06x}");
    Ok(())
}

fn other(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, message.to_string())
}
