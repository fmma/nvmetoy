use zerocopy::{FromBytes, FromZeros, Immutable, IntoBytes, KnownLayout};

pub(crate) const CAP: usize = 0x00; // Controller Capabilities
pub(crate) const CC: usize = 0x14; // Controller Configuration
pub(crate) const CSTS: usize = 0x1C; // Controller Status
pub(crate) const AQA: usize = 0x24; // Admin Queue Attributes
pub(crate) const ASQ: usize = 0x28; // Admin Submission Queue Base Address
pub(crate) const ACQ: usize = 0x30; // Admin Completion Queue Base Address
pub(crate) const DOORBELL_BASE: usize = 0x1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AdminOpcode {
    DeleteIoSubmissionQueue = 0x00,
    CreateIoSubmissionQueue = 0x01,
    GetLogPage = 0x02,
    DeleteIoCompletionQueue = 0x04,
    CreateIoCompletionQueue = 0x05,
    Identify = 0x06,
    Abort = 0x08,
    SetFeatures = 0x09,
    GetFeatures = 0x0A,
    AsyncEventRequest = 0x0C,
    NamespaceManagement = 0x0D,
    FirmwareCommit = 0x10,
    FirmwareImageDownload = 0x11,
    DeviceSelfTest = 0x14,
    NamespaceAttachment = 0x15,
    KeepAlive = 0x18,
    DirectiveSend = 0x19,
    DirectiveReceive = 0x1A,
    VirtualizationManagement = 0x1C,
    NvmeMiSend = 0x1D,
    NvmeMiReceive = 0x1E,
    CapacityManagement = 0x20,
    Lockdown = 0x24,
    DoorbellBufferConfig = 0x7C,
    FormatNvm = 0x80,
    SecuritySend = 0x81,
    SecurityReceive = 0x82,
    Sanitize = 0x84,
    GetLbaStatus = 0x86,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IoOpcode {
    Flush = 0x00,
    Write = 0x01,
    Read = 0x02,
    WriteUncorrectable = 0x04,
    Compare = 0x05,
    WriteZeroes = 0x08,
    DatasetManagement = 0x09,
    Verify = 0x0C,
    ReservationRegister = 0x0D,
    ReservationReport = 0x0E,
    ReservationAcquire = 0x11,
    ReservationRelease = 0x15,
    Copy = 0x19,
}

impl From<AdminOpcode> for u8 {
    fn from(op: AdminOpcode) -> u8 {
        op as u8
    }
}

impl From<IoOpcode> for u8 {
    fn from(op: IoOpcode) -> u8 {
        op as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IdentifyStructure {
    Namespace = 0x00,
    Controller = 0x01,
    ActiveNamespaceList = 0x02,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct SubmissionQueueEntry {
    pub opc: u8, // Opcode
    pub flags: u8,
    pub cid: u16,   // Command Identifier
    pub nsid: u32,  // Namespace Identifier
    pub cdw2: u32,  // Command Dword 2
    pub cdw3: u32,  // Command Dword 3
    pub mptr: u64,  // Metadata Pointer
    pub prp1: u64,  // Physical Region Page entry 1
    pub prp2: u64,  // Physical Region Page entry 2
    pub cdw10: u32, // Command Dword 10
    pub cdw11: u32, // Command Dword 11
    pub cdw12: u32, // Command Dword 12
    pub cdw13: u32, // Command Dword 13
    pub cdw14: u32, // Command Dword 14
    pub cdw15: u32, // Command Dword 15
}

impl SubmissionQueueEntry {
    pub fn new(opc: u8) -> Self {
        let mut entry = SubmissionQueueEntry::new_zeroed();
        entry.opc = opc;
        entry
    }

    pub fn as_bytes(&self) -> &[u8] {
        IntoBytes::as_bytes(self)
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct CompletionQueueEntry {
    pub result: u32,
    pub reserved: u32,
    pub sq_head: u16, // Submission Queue Head Pointer
    pub sq_id: u16,   // Submission Queue Identifier
    pub cid: u16,     // Command Identifier
    pub status: u16,
}

impl CompletionQueueEntry {
    pub fn phase(&self) -> bool {
        self.status & 0x1 != 0
    }

    pub fn status_code(&self) -> u8 {
        ((self.status >> 1) & 0xFF) as u8
    }

    pub fn status_code_type(&self) -> u8 {
        ((self.status >> 9) & 0x7) as u8
    }

    pub fn is_success(&self) -> bool {
        self.status_code() == 0 && self.status_code_type() == 0
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, KnownLayout, Immutable)]
pub struct IdentifyController {
    pub vid: u16,      // PCI Vendor ID
    pub ssvid: u16,    // PCI Subsystem Vendor ID
    pub sn: [u8; 20],  // Serial Number
    pub mn: [u8; 40],  // Model Number
    pub fr: [u8; 8],   // Firmware Revision
    pub rab: u8,       // Recommended Arbitration Burst
    pub ieee: [u8; 3], // IEEE OUI Identifier
    pub cmic: u8,      // Controller Multi-Path I/O and Namespace Sharing Capabilities
    pub mdts: u8,      // Maximum Data Transfer Size
    pub cntlid: u16,   // Controller ID
    pub ver: u32,      // Version
    pub rtd3r: u32,    // RTD3 Resume Latency
    pub rtd3e: u32,    // RTD3 Entry Latency
    pub oaes: u32,     // Optional Asynchronous Events Supported
    pub ctratt: u32,   // Controller Attributes
    _reserved0: [u8; 156],
    pub oacs: u16, // Optional Admin Command Support
    pub acl: u8,   // Abort Command Limit
    pub aerl: u8,  // Asynchronous Event Request Limit
    pub frmw: u8,  // Firmware Updates
    pub lpa: u8,   // Log Page Attributes
    pub elpe: u8,  // Error Log Page Entries
    pub npss: u8,  // Number of Power States Support
    _reserved1: [u8; 248],
    pub sqes: u8,    // Submission Queue Entry Size
    pub cqes: u8,    // Completion Queue Entry Size
    pub maxcmd: u16, // Maximum Outstanding Commands
    pub nn: u32,     // Number of Namespaces
    pub oncs: u16,   // Optional NVM Command Support
    pub fuses: u16,  // Fused Operation Support
    pub fna: u8,     // Format NVM Attributes
    pub vwc: u8,     // Volatile Write Cache
    _reserved2: [u8; 10],
    pub sgls: u32, // SGL Support
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};

    #[test]
    fn identify_controller_byte_layout() {
        assert_eq!(size_of::<IdentifyController>(), 540);

        assert_eq!(offset_of!(IdentifyController, vid), 0);
        assert_eq!(offset_of!(IdentifyController, ssvid), 2);
        assert_eq!(offset_of!(IdentifyController, sn), 4);
        assert_eq!(offset_of!(IdentifyController, mn), 24);
        assert_eq!(offset_of!(IdentifyController, fr), 64);
        assert_eq!(offset_of!(IdentifyController, ieee), 73);
        assert_eq!(offset_of!(IdentifyController, mdts), 77);
        assert_eq!(offset_of!(IdentifyController, cntlid), 78);
        assert_eq!(offset_of!(IdentifyController, ver), 80);
        assert_eq!(offset_of!(IdentifyController, oaes), 92);
        assert_eq!(offset_of!(IdentifyController, ctratt), 96);
        assert_eq!(offset_of!(IdentifyController, oacs), 256);
        assert_eq!(offset_of!(IdentifyController, acl), 258);
        assert_eq!(offset_of!(IdentifyController, aerl), 259);
        assert_eq!(offset_of!(IdentifyController, frmw), 260);
        assert_eq!(offset_of!(IdentifyController, lpa), 261);
        assert_eq!(offset_of!(IdentifyController, npss), 263);
        assert_eq!(offset_of!(IdentifyController, sqes), 512);
        assert_eq!(offset_of!(IdentifyController, cqes), 513);
        assert_eq!(offset_of!(IdentifyController, maxcmd), 514);
        assert_eq!(offset_of!(IdentifyController, nn), 516);
        assert_eq!(offset_of!(IdentifyController, oncs), 520);
        assert_eq!(offset_of!(IdentifyController, fna), 524);
        assert_eq!(offset_of!(IdentifyController, vwc), 525);
        assert_eq!(offset_of!(IdentifyController, sgls), 536);
    }

    #[test]
    fn submission_queue_entry_byte_layout() {
        assert_eq!(size_of::<SubmissionQueueEntry>(), 64);
        assert_eq!(align_of::<SubmissionQueueEntry>(), 8);

        assert_eq!(offset_of!(SubmissionQueueEntry, opc), 0);
        assert_eq!(offset_of!(SubmissionQueueEntry, flags), 1);
        assert_eq!(offset_of!(SubmissionQueueEntry, cid), 2);
        assert_eq!(offset_of!(SubmissionQueueEntry, nsid), 4);
        assert_eq!(offset_of!(SubmissionQueueEntry, cdw2), 8);
        assert_eq!(offset_of!(SubmissionQueueEntry, cdw3), 12);
        assert_eq!(offset_of!(SubmissionQueueEntry, mptr), 16);
        assert_eq!(offset_of!(SubmissionQueueEntry, prp1), 24);
        assert_eq!(offset_of!(SubmissionQueueEntry, prp2), 32);
        assert_eq!(offset_of!(SubmissionQueueEntry, cdw10), 40);
        assert_eq!(offset_of!(SubmissionQueueEntry, cdw11), 44);
        assert_eq!(offset_of!(SubmissionQueueEntry, cdw12), 48);
        assert_eq!(offset_of!(SubmissionQueueEntry, cdw13), 52);
        assert_eq!(offset_of!(SubmissionQueueEntry, cdw14), 56);
        assert_eq!(offset_of!(SubmissionQueueEntry, cdw15), 60);
    }

    #[test]
    fn completion_queue_entry_size_is_16() {
        assert_eq!(size_of::<CompletionQueueEntry>(), 16);
    }
}
