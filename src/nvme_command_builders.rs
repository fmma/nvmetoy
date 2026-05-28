use crate::nvme_spec::{AdminOpcode, IdentifyStructure, IoOpcode, SubmissionQueueEntry};

pub fn nvme_identify(structure: IdentifyStructure, nsid: u32, prp1: u64) -> SubmissionQueueEntry {
    let mut entry = SubmissionQueueEntry::new(AdminOpcode::Identify.into());
    entry.nsid = nsid;
    entry.prp1 = prp1;
    entry.cdw10 = structure as u32;
    entry
}

pub fn nvme_create_io_completion_queue(
    queue_id: u16,
    size: u16,
    prp1: u64,
) -> SubmissionQueueEntry {
    let mut entry = SubmissionQueueEntry::new(AdminOpcode::CreateIoCompletionQueue.into());
    entry.prp1 = prp1;
    entry.cdw10 = (queue_id as u32) | (((size as u32) - 1) << 16);
    entry.cdw11 = 1;
    entry
}

pub fn nvme_create_io_submission_queue(
    queue_id: u16,
    size: u16,
    completion_queue_id: u16,
    prp1: u64,
) -> SubmissionQueueEntry {
    let mut entry = SubmissionQueueEntry::new(AdminOpcode::CreateIoSubmissionQueue.into());
    entry.prp1 = prp1;
    entry.cdw10 = (queue_id as u32) | (((size as u32) - 1) << 16);
    entry.cdw11 = 1 | ((completion_queue_id as u32) << 16);
    entry
}

pub fn nvme_read(nsid: u32, slba: u64, nlb: u16, prp1: u64, prp2: u64) -> SubmissionQueueEntry {
    let mut entry = SubmissionQueueEntry::new(IoOpcode::Read.into());
    entry.nsid = nsid;
    entry.prp1 = prp1;
    entry.prp2 = prp2;
    entry.cdw10 = slba as u32;
    entry.cdw11 = (slba >> 32) as u32;
    entry.cdw12 = nlb as u32;
    entry
}

pub fn nvme_write(nsid: u32, slba: u64, nlb: u16, prp1: u64, prp2: u64) -> SubmissionQueueEntry {
    let mut entry = SubmissionQueueEntry::new(IoOpcode::Write.into());
    entry.nsid = nsid;
    entry.prp1 = prp1;
    entry.prp2 = prp2;
    entry.cdw10 = slba as u32;
    entry.cdw11 = (slba >> 32) as u32;
    entry.cdw12 = nlb as u32;
    entry
}
