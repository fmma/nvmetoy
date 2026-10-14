// Registers, opcodes, and data structures transcribed from the NVMe specifications.
//
// From the NVM Express Base Specification, Revision 2.3:
//   - Controller register offsets (CAP, CC, CSTS, AQA, ASQ, ACQ, doorbells).
//   - SubmissionQueueEntry: Common Command Format (Figures 91 and 92).
//   - CompletionQueueEntry: Common Completion Queue Entry Layout (Figures 96 and 100).
//   - AdminOpcode: Opcodes for Admin Commands (Figure 142).
//   - IdentifyController, PowerStateDescriptor (Figures 328 and 329).
//
// From the NVM Express NVM Command Set Specification, Revision 1.1:
//   - IoOpcode: Opcodes for NVM Commands (Figure 21).
//   - IdentifyNamespace, LbaFormat (Figures 114 and 116).
use zerocopy::{FromBytes, FromZeros, Immutable, KnownLayout};

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
    DiscoveryInformationManagement = 0x21,
    FabricZoningReceive = 0x22,
    Lockdown = 0x24,
    FabricZoningLookup = 0x25,
    ClearExportedNvmResourceConfiguration = 0x28,
    FabricZoningSend = 0x29,
    CreateExportedNvmSubsystem = 0x2A,
    ManageExportedNvmSubsystem = 0x2D,
    ManageExportedNamespace = 0x31,
    ManageExportedPort = 0x35,
    CrossControllerReset = 0x38,
    SendDiscoveryLogPage = 0x39,
    TrackSend = 0x3D,
    TrackReceive = 0x3E,
    MigrationSend = 0x41,
    MigrationReceive = 0x42,
    ControllerDataQueue = 0x45,
    DoorbellBufferConfig = 0x7C,
    FabricsCommands = 0x7F,
    FormatNvm = 0x80,
    SecuritySend = 0x81,
    SecurityReceive = 0x82,
    Sanitize = 0x84,
    LoadProgram = 0x85,
    GetLbaStatus = 0x86,
    ProgramActivationManagement = 0x88,
    MemoryRangeSetManagement = 0x89,
    SanitizeNamespace = 0x8C,
    // C0h..FFh are vendor specific.
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
    IoManagementReceive = 0x12,
    ReservationRelease = 0x15,
    Cancel = 0x18,
    Copy = 0x19,
    IoManagementSend = 0x1D,
    // 80h..FFh are vendor specific.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromBytes, KnownLayout, Immutable)]
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
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromBytes, KnownLayout, Immutable)]
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
    // Controller Capabilities and Features (bytes 0..255)
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
    pub rrls: u16,     // Read Recovery Levels Supported
    pub bpcap: u8,     // Boot Partition Capabilities
    _reserved103: u8,
    pub nssl: u32, // NVM Subsystem Shutdown Latency
    _reserved108: [u8; 2],
    pub plsi: u8,        // Power Loss Signaling Information
    pub cntrltype: u8,   // Controller Type
    pub fguid: [u8; 16], // FRU Globally Unique Identifier
    pub crdt1: u16,      // Command Retry Delay Time 1
    pub crdt2: u16,      // Command Retry Delay Time 2
    pub crdt3: u16,      // Command Retry Delay Time 3
    pub crcap: u8,       // Controller Reachability Capabilities
    pub ciu: u8,         // Controller Instance Uniquifier
    pub cirn: [u8; 8],   // Controller Instance Random Number
    _reserved144: [u8; 96],
    _reserved240: [u8; 13],
    pub nvmsr: u8, // NVM Subsystem Report
    pub vwci: u8,  // VPD Write Cycle Information
    pub mec: u8,   // Management Endpoint Capabilities

    // Admin Command Set Attributes (bytes 256..511)
    pub oacs: u16,         // Optional Admin Command Support
    pub acl: u8,           // Abort Command Limit
    pub aerl: u8,          // Asynchronous Event Request Limit
    pub frmw: u8,          // Firmware Updates
    pub lpa: u8,           // Log Page Attributes
    pub elpe: u8,          // Error Log Page Entries
    pub npss: u8,          // Number of Power States Support
    pub avscc: u8,         // Admin Vendor Specific Command Configuration
    pub apsta: u8,         // Autonomous Power State Transition Attributes
    pub wctemp: u16,       // Warning Composite Temperature Threshold
    pub cctemp: u16,       // Critical Composite Temperature Threshold
    pub mtfa: u16,         // Maximum Time for Firmware Activation
    pub hmpre: u32,        // Host Memory Buffer Preferred Size
    pub hmmin: u32,        // Host Memory Buffer Minimum Size
    pub tnvmcap: [u8; 16], // Total NVM Capacity
    pub unvmcap: [u8; 16], // Unallocated NVM Capacity
    pub rpmbs: u32,        // Replay Protected Memory Block Support
    pub edstt: u16,        // Extended Device Self-test Time
    pub dsto: u8,          // Device Self-test Options
    pub fwug: u8,          // Firmware Update Granularity
    pub kas: u16,          // Keep Alive Support
    pub hctma: u16,        // Host Controlled Thermal Management Attributes
    pub mntmt: u16,        // Minimum Thermal Management Temperature
    pub mxtmt: u16,        // Maximum Thermal Management Temperature
    pub sanicap: u32,      // Sanitize Capabilities
    pub hmminds: u32,      // Host Memory Buffer Minimum Descriptor Entry Size
    pub hmmaxd: u16,       // Host Memory Maximum Descriptors Entries
    pub nsetidmax: u16,    // NVM Set Identifier Maximum
    pub endgidmax: u16,    // Endurance Group Identifier Maximum
    pub anatt: u8,         // ANA Transition Time
    pub anacap: u8,        // Asymmetric Namespace Access Capabilities
    pub anagrpmax: u32,    // ANA Group Identifier Maximum
    pub nanagrpid: u32,    // Number of ANA Group Identifiers
    pub pels: u32,         // Persistent Event Log Size
    pub did: u16,          // Domain Identifier
    pub kpioc: u8,         // Key Per I/O Capabilities
    _reserved359: u8,
    pub mptfawr: u16, // Maximum Processing Time for Firmware Activation Without Reset
    _reserved362: [u8; 6],
    pub megcap: [u8; 16], // Max Endurance Group Capacity
    pub tmptthha: u8,     // Temperature Threshold Hysteresis Attributes
    pub mupa: u8,         // Maximum Unlimited Power Attributes
    pub cqt: u16,         // Command Quiesce Time
    pub cdpa: u16,        // Configurable Device Personality Attributes
    pub mup: u16,         // Maximum Unlimited Power
    pub ipmsr: u16,       // Interval Power Measurement Sample Rate
    pub srv: u16,         // Sample Rate Value
    _reserved396: [u8; 116],

    // NVM Command Set Attributes (bytes 512..703)
    pub sqes: u8,         // Submission Queue Entry Size
    pub cqes: u8,         // Completion Queue Entry Size
    pub maxcmd: u16,      // Maximum Outstanding Commands
    pub nn: u32,          // Number of Namespaces
    pub oncs: u16,        // Optional NVM Command Support
    pub fuses: u16,       // Fused Operation Support
    pub fna: u8,          // Format NVM Attributes
    pub vwc: u8,          // Volatile Write Cache
    pub awun: u16,        // Atomic Write Unit Normal
    pub awupf: u16,       // Atomic Write Unit Power Fail
    pub icsvscc: u8,      // I/O Command Set Vendor Specific Command Configuration
    pub nwpc: u8,         // Namespace Write Protection Capabilities
    pub acwu: u16,        // Atomic Compare & Write Unit
    pub cdfs: u16,        // Copy Descriptor Formats Supported
    pub sgls: u32,        // SGL Support
    pub mnan: u32,        // Maximum Number of Allowed Namespaces
    pub maxdna: [u8; 16], // Maximum Domain Namespace Attachments
    pub maxcna: u32,      // Maximum I/O Controller Namespace Attachments
    pub oaqd: u32,        // Optimal Aggregated Queue Depth
    pub rhiri: u8,        // Recommended Host-Initiated Refresh Interval
    pub hirt: u8,         // Host-Initiated Refresh Time
    pub cmmrtd: u16,      // Controller Maximum Memory Range Tracking Descriptors
    pub nmmrtd: u16,      // NVM Subsystem Maximum Memory Range Tracking Descriptors
    pub minmrtg: u8,      // Minimum Memory Range Tracking Granularity
    pub maxmrtg: u8,      // Maximum Memory Range Tracking Granularity
    pub trattr: u8,       // Tracking Attributes
    _reserved577: u8,
    pub mcudmq: u16,  // Maximum Controller User Data Migration Queues
    pub mnsudmq: u16, // Maximum NVM Subsystem User Data Migration Queues
    pub mcmr: u16,    // Maximum CDQ Memory Ranges
    pub nmcmr: u16,   // NVM Subsystem Maximum CDQ Memory Ranges
    pub mcdqpc: u16,  // Maximum Controller Data Queue PRP Count
    _reserved588: [u8; 180],
    pub subnqn: [u8; 256], // NVM Subsystem NVMe Qualified Name
    _reserved1024: [u8; 768],

    // NVMe over Fabrics (bytes 1792..2047)
    pub ioccsz: u32, // I/O Queue Command Capsule Supported Size
    pub iorcsz: u32, // I/O Queue Response Capsule Supported Size
    pub icdoff: u16, // In Capsule Data Offset
    pub fcatt: u8,   // Fabrics Controller Attributes
    pub msdbd: u8,   // Maximum SGL Data Block Descriptors
    pub ofcs: u16,   // Optional Fabrics Commands Support
    pub dctype: u8,  // Discovery Controller Type
    _reserved1807: [u8; 241],

    // Power State Descriptors (bytes 2048..3071) and Vendor Specific (bytes 3072..4095)
    pub psds: [PowerStateDescriptor; 32],
    pub vendor_specific: [u8; 1024],
}

// Figure 329. A 32-byte descriptor; the upper 9 bytes pack the newer power/bandwidth fields
// (MBW, MBWS, EPFVT, EPFVTS, FQVT, FQVTS, EPFRT, EPFRTS) at bit granularity, kept as a blob.
#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, KnownLayout, Immutable)]
pub struct PowerStateDescriptor {
    pub mp: u16, // Maximum Power
    _reserved2: u8,
    pub mxps_nops: u8, // bit 0 = MXPS (Max Power Scale), bit 1 = NOPS (Non-Operational State)
    pub enlat: u32,    // Entry Latency, microseconds
    pub exlat: u32,    // Exit Latency, microseconds
    pub rrt: u8,       // Relative Read Throughput (bits 4:0)
    pub rrl: u8,       // Relative Read Latency (bits 4:0)
    pub rwt: u8,       // Relative Write Throughput (bits 4:0)
    pub rwl: u8,       // Relative Write Latency (bits 4:0)
    pub idlp: u16,     // Idle Power
    pub ips: u8,       // Idle Power Scale (bits 7:6)
    _reserved19: u8,
    pub actp: u16,   // Active Power
    pub apw_aps: u8, // Active Power Workload (bits 2:0), Active Power Scale (bits 7:6)
    _upper: [u8; 9], // MBW/MBWS/EPFVT/EPFVTS/FQVT/FQVTS/EPFRT/EPFRTS, bit-packed
}

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, KnownLayout, Immutable)]
pub struct LbaFormat {
    pub ms: u16,   // Metadata Size, in bytes
    pub lbads: u8, // LBA Data Size, as a power of two (2^lbads bytes)
    pub rp: u8,    // Relative Performance
}

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, KnownLayout, Immutable)]
pub struct IdentifyNamespace {
    pub nsze: u64,        // Namespace Size, in logical blocks
    pub ncap: u64,        // Namespace Capacity, in logical blocks
    pub nuse: u64,        // Namespace Utilization, in logical blocks
    pub nsfeat: u8,       // Namespace Features
    pub nlbaf: u8,        // Number of LBA Formats (zero-based)
    pub flbas: u8,        // Formatted LBA Size (FIDXL = bits 3:0, FIDXU = bits 6:5)
    pub mc: u8,           // Metadata Capabilities
    pub dpc: u8,          // End-to-end Data Protection Capabilities
    pub dps: u8,          // End-to-end Data Protection Type Settings
    pub nmic: u8,         // Namespace Multi-path I/O and Sharing Capabilities
    pub rescap: u8,       // Reservation Capabilities
    pub fpi: u8,          // Format Progress Indicator
    pub dlfeat: u8,       // Deallocate Logical Block Features
    pub nawun: u16,       // Namespace Atomic Write Unit Normal
    pub nawupf: u16,      // Namespace Atomic Write Unit Power Fail
    pub nacwu: u16,       // Namespace Atomic Compare & Write Unit
    pub nabsn: u16,       // Namespace Atomic Boundary Size Normal
    pub nabo: u16,        // Namespace Atomic Boundary Offset
    pub nabspf: u16,      // Namespace Atomic Boundary Size Power Fail
    pub noiob: u16,       // Namespace Optimal I/O Boundary
    pub nvmcap: [u8; 16], // NVM Capacity, in bytes (128-bit)
    pub npwg: u16,        // Namespace Preferred Write Granularity
    pub npwa: u16,        // Namespace Preferred Write Alignment
    pub npdg: u16,        // Namespace Preferred Deallocate Granularity
    pub npda: u16,        // Namespace Preferred Deallocate Alignment
    pub nows: u16,        // Namespace Optimal Write Size
    pub mssrl: u16,       // Maximum Single Source Range Length
    pub mcl: u32,         // Maximum Copy Length
    pub msrc: u8,         // Maximum Source Range Count
    pub kpios: u8,        // Key Per I/O Status
    pub nulbaf: u8,       // Number of Unique Attribute LBA Formats
    _reserved83: u8,
    pub kpiodaag: u32, // Key Per I/O Data Access Alignment and Granularity
    _reserved88: [u8; 4],
    pub anagrpid: u32, // ANA Group Identifier
    _reserved96: [u8; 3],
    pub nsattr: u8,                   // Namespace Attributes
    pub nvmsetid: u16,                // NVM Set Identifier
    pub endgid: u16,                  // Endurance Group Identifier
    pub nguid: [u8; 16],              // Namespace Globally Unique Identifier
    pub eui64: [u8; 8],               // IEEE Extended Unique Identifier
    pub lba_formats: [LbaFormat; 64], // LBAF0..LBAF63, at offset 128
    pub vendor_specific: [u8; 3712],  // VS, bytes 384..4095
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, offset_of, size_of};

    #[test]
    fn identify_controller_byte_layout() {
        assert_eq!(size_of::<IdentifyController>(), 4096);
        assert_eq!(size_of::<PowerStateDescriptor>(), 32);

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
        assert_eq!(offset_of!(IdentifyController, rrls), 100);
        assert_eq!(offset_of!(IdentifyController, nssl), 104);
        assert_eq!(offset_of!(IdentifyController, fguid), 112);
        assert_eq!(offset_of!(IdentifyController, crdt1), 128);
        assert_eq!(offset_of!(IdentifyController, crcap), 134);
        assert_eq!(offset_of!(IdentifyController, cirn), 136);
        assert_eq!(offset_of!(IdentifyController, nvmsr), 253);
        assert_eq!(offset_of!(IdentifyController, oacs), 256);
        assert_eq!(offset_of!(IdentifyController, acl), 258);
        assert_eq!(offset_of!(IdentifyController, aerl), 259);
        assert_eq!(offset_of!(IdentifyController, frmw), 260);
        assert_eq!(offset_of!(IdentifyController, lpa), 261);
        assert_eq!(offset_of!(IdentifyController, npss), 263);
        assert_eq!(offset_of!(IdentifyController, wctemp), 266);
        assert_eq!(offset_of!(IdentifyController, hmpre), 272);
        assert_eq!(offset_of!(IdentifyController, tnvmcap), 280);
        assert_eq!(offset_of!(IdentifyController, sanicap), 328);
        assert_eq!(offset_of!(IdentifyController, anagrpmax), 344);
        assert_eq!(offset_of!(IdentifyController, did), 356);
        assert_eq!(offset_of!(IdentifyController, megcap), 368);
        assert_eq!(offset_of!(IdentifyController, sqes), 512);
        assert_eq!(offset_of!(IdentifyController, cqes), 513);
        assert_eq!(offset_of!(IdentifyController, maxcmd), 514);
        assert_eq!(offset_of!(IdentifyController, nn), 516);
        assert_eq!(offset_of!(IdentifyController, oncs), 520);
        assert_eq!(offset_of!(IdentifyController, fna), 524);
        assert_eq!(offset_of!(IdentifyController, vwc), 525);
        assert_eq!(offset_of!(IdentifyController, sgls), 536);
        assert_eq!(offset_of!(IdentifyController, mnan), 540);
        assert_eq!(offset_of!(IdentifyController, mcdqpc), 586);
        assert_eq!(offset_of!(IdentifyController, subnqn), 768);
        assert_eq!(offset_of!(IdentifyController, ioccsz), 1792);
        assert_eq!(offset_of!(IdentifyController, dctype), 1806);
        assert_eq!(offset_of!(IdentifyController, psds), 2048);
        assert_eq!(offset_of!(IdentifyController, vendor_specific), 3072);

        assert_eq!(offset_of!(PowerStateDescriptor, enlat), 4);
        assert_eq!(offset_of!(PowerStateDescriptor, exlat), 8);
        assert_eq!(offset_of!(PowerStateDescriptor, idlp), 16);
        assert_eq!(offset_of!(PowerStateDescriptor, actp), 20);
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

    #[test]
    fn identify_namespace_byte_layout() {
        assert_eq!(size_of::<IdentifyNamespace>(), 4096);
        assert_eq!(size_of::<LbaFormat>(), 4);
        assert_eq!(offset_of!(LbaFormat, lbads), 2);

        assert_eq!(offset_of!(IdentifyNamespace, nsze), 0);
        assert_eq!(offset_of!(IdentifyNamespace, ncap), 8);
        assert_eq!(offset_of!(IdentifyNamespace, nuse), 16);
        assert_eq!(offset_of!(IdentifyNamespace, nsfeat), 24);
        assert_eq!(offset_of!(IdentifyNamespace, flbas), 26);
        assert_eq!(offset_of!(IdentifyNamespace, dlfeat), 33);
        assert_eq!(offset_of!(IdentifyNamespace, nawun), 34);
        assert_eq!(offset_of!(IdentifyNamespace, nvmcap), 48);
        assert_eq!(offset_of!(IdentifyNamespace, npwg), 64);
        assert_eq!(offset_of!(IdentifyNamespace, mcl), 76);
        assert_eq!(offset_of!(IdentifyNamespace, msrc), 80);
        assert_eq!(offset_of!(IdentifyNamespace, kpiodaag), 84);
        assert_eq!(offset_of!(IdentifyNamespace, anagrpid), 92);
        assert_eq!(offset_of!(IdentifyNamespace, nsattr), 99);
        assert_eq!(offset_of!(IdentifyNamespace, nvmsetid), 100);
        assert_eq!(offset_of!(IdentifyNamespace, endgid), 102);
        assert_eq!(offset_of!(IdentifyNamespace, nguid), 104);
        assert_eq!(offset_of!(IdentifyNamespace, eui64), 120);
        assert_eq!(offset_of!(IdentifyNamespace, lba_formats), 128);
        assert_eq!(offset_of!(IdentifyNamespace, vendor_specific), 384);
    }
}
