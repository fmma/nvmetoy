use std::io;
use std::mem::size_of;
use std::sync::atomic::{fence, Ordering};

use crate::dma_region::DmaRegion;
use crate::nvme_bar::NvmeBar;
use crate::nvme_spec::{CompletionQueueEntry, SubmissionQueueEntry};

pub(crate) const QUEUE_DEPTH: usize = 64;

pub(crate) struct QueuePair {
    id: u16,
    submission_offset: usize,
    completion_offset: usize,
    submission_tail: u32,
    completion_head: u32,
    completion_phase: bool,
    next_cid: u16,
}

impl QueuePair {
    pub(crate) fn new(id: u16, submission_offset: usize, completion_offset: usize) -> Self {
        QueuePair {
            id,
            submission_offset,
            completion_offset,
            submission_tail: 0,
            completion_head: 0,
            completion_phase: true,
            next_cid: 0,
        }
    }

    pub(crate) fn submit_and_wait(
        &mut self,
        bar: &NvmeBar,
        dma: &DmaRegion,
        doorbell_stride: usize,
        mut command: SubmissionQueueEntry,
    ) -> io::Result<CompletionQueueEntry> {
        let cid = self.next_cid;
        self.next_cid = self.next_cid.wrapping_add(1);
        command.cid = cid;

        let sqe_offset = self.submission_offset
            + self.submission_tail as usize * size_of::<SubmissionQueueEntry>();
        chat!(
            "queue {}: submitting command CID {cid} into submission queue slot {} (DMA offset {sqe_offset:#x}, phys {:#018x})",
            self.id,
            self.submission_tail,
            dma.physical_address_at(sqe_offset)
        );
        dma.write_volatile(sqe_offset, command);
        let written_back: SubmissionQueueEntry = dma.read_volatile(sqe_offset);
        chat!("  the 64-byte SQE, read back from DMA to confirm what the controller will fetch:\n{written_back:#x?}");
        chat!("  memory fence, so the SQE is visible before the doorbell");
        fence(Ordering::SeqCst);

        self.submission_tail = (self.submission_tail + 1) % QUEUE_DEPTH as u32;
        bar.ring_submission_doorbell(self.id, doorbell_stride, self.submission_tail);

        chat!(
            "  polling completion queue slot {} for an entry tagged phase {}...",
            self.completion_head,
            self.completion_phase as u8
        );
        let mut spins = 0u32;
        loop {
            crate::pause();
            let cqe_offset = self.completion_offset
                + self.completion_head as usize * size_of::<CompletionQueueEntry>();
            let completion: CompletionQueueEntry = dma.read_volatile(cqe_offset);
            if completion.phase() == self.completion_phase {
                fence(Ordering::SeqCst);
                chat!(
                    "  CQE the controller posted at DMA offset {cqe_offset:#x} (phys {:#018x}):\n{completion:#x?}",
                    dma.physical_address_at(cqe_offset)
                );
                chat!(
                    "  completion after {spins} idle polls: phase={} (matches expected, so this slot is freshly posted), CID {}, SCT={:#x} SC={:#x} ({})",
                    completion.phase() as u8,
                    completion.cid,
                    completion.status_code_type(),
                    completion.status_code(),
                    if completion.is_success() {
                        "success"
                    } else {
                        "FAILED"
                    }
                );
                self.completion_head = (self.completion_head + 1) % QUEUE_DEPTH as u32;
                if self.completion_head == 0 {
                    self.completion_phase = !self.completion_phase;
                    chat!(
                        "  completion queue wrapped; expected phase tag flips to {}",
                        self.completion_phase as u8
                    );
                }
                bar.ring_completion_doorbell(self.id, doorbell_stride, self.completion_head);

                if completion.cid != cid {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!(
                            "completion for command {} but expected {cid}",
                            completion.cid
                        ),
                    ));
                }
                return Ok(completion);
            }
            spins += 1;
            if spins > 200_000 {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "timed out polling completion queue",
                ));
            }
        }
    }
}
