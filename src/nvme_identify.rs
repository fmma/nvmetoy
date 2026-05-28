use std::io;

use zerocopy::FromBytes;

use crate::nvme_spec::IdentifyController;

fn ascii_field(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_string()
}

fn flags(value: u32, bits: &[(u32, &str)]) -> String {
    let set: Vec<&str> = bits
        .iter()
        .filter(|(bit, _)| value & (1 << bit) != 0)
        .map(|(_, name)| *name)
        .collect();
    if set.is_empty() {
        "(none)".to_string()
    } else {
        set.join(", ")
    }
}

pub fn print_controller_identification(data: &[u8]) -> io::Result<()> {
    let (id, _) = IdentifyController::read_from_prefix(data)
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "Identify Controller data too short"))?;

    let version = id.ver;
    let version_major = version >> 16;
    let version_minor = (version >> 8) & 0xFF;
    let version_tertiary = version & 0xFF;

    let maximum_data_transfer = id.mdts; // in 2^MPS units, 0 = no limit
    let controller_id = id.cntlid;
    let submission_entry_size = id.sqes; // 2^(low nibble) bytes
    let completion_entry_size = id.cqes;
    let max_outstanding_commands = id.maxcmd;
    let namespaces = id.nn;
    let abort_command_limit = id.acl;
    let async_event_request_limit = id.aerl;
    let power_states = id.npss as u32 + 1; // zero-based

    println!();
    println!("=== Identification ===");
    println!("PCI Vendor (VID)        : {:#06x}", id.vid);
    println!("PCI Subsys Vendor(SSVID): {:#06x}", id.ssvid);
    println!("Model (MN)              : {}", ascii_field(&id.mn));
    println!("Serial (SN)             : {}", ascii_field(&id.sn));
    println!("Firmware (FR)           : {}", ascii_field(&id.fr));
    println!(
        "IEEE OUI                : {:02x}-{:02x}-{:02x}",
        id.ieee[2], id.ieee[1], id.ieee[0]
    );
    println!("Controller ID (CNTLID)  : {controller_id:#06x}");
    println!("NVMe Version (VER)      : {version_major}.{version_minor}.{version_tertiary}");

    println!();
    println!("=== Limits ===");
    if maximum_data_transfer == 0 {
        println!("Max Data Transfer(MDTS) : no limit");
    } else {
        println!("Max Data Transfer(MDTS) : 2^{maximum_data_transfer} pages");
    }
    println!("Number of Namespaces(NN): {namespaces}");
    println!(
        "SQ Entry Size (SQES)    : min 2^{}, max 2^{} bytes",
        submission_entry_size & 0xF,
        submission_entry_size >> 4
    );
    println!(
        "CQ Entry Size (CQES)    : min 2^{}, max 2^{} bytes",
        completion_entry_size & 0xF,
        completion_entry_size >> 4
    );
    println!("Max Outstanding(MAXCMD) : {max_outstanding_commands}");
    println!(
        "Abort Command Limit(ACL): {}",
        abort_command_limit as u32 + 1
    );
    println!(
        "Async Event Limit(AERL) : {}",
        async_event_request_limit as u32 + 1
    );
    println!("Power States (NPSS)     : {power_states}");

    println!();
    println!("=== Capabilities ===");

    let optional_admin = id.oacs as u32; // OACS: Optional Admin Command Support
    println!(
        "Admin commands (OACS)   : {}",
        flags(
            optional_admin,
            &[
                (0, "Security Send/Receive"),
                (1, "Format NVM"),
                (2, "Firmware Download/Commit"),
                (3, "Namespace Management"),
                (4, "Device Self-test"),
                (5, "Directives"),
                (6, "NVMe-MI Send/Receive"),
                (7, "Virtualization Management"),
                (8, "Doorbell Buffer Config"),
                (9, "Get LBA Status"),
            ],
        )
    );

    let optional_nvm = id.oncs as u32; // ONCS: Optional NVM Command Support
    println!(
        "NVM commands (ONCS)     : {}",
        flags(
            optional_nvm,
            &[
                (0, "Compare"),
                (1, "Write Uncorrectable"),
                (2, "Dataset Management"),
                (3, "Write Zeroes"),
                (4, "Save/Select in Features"),
                (5, "Reservations"),
                (6, "Timestamp"),
                (7, "Verify"),
                (8, "Copy"),
            ],
        )
    );

    let async_events = id.oaes; // OAES: Optional Asynchronous Events Supported
    println!(
        "Async events (OAES)     : {}",
        flags(
            async_events,
            &[
                (8, "Namespace Attribute Notices"),
                (9, "Firmware Activation Notices"),
                (11, "ANA Change Notices"),
                (12, "Predictable Latency Aggregate Log Change"),
                (13, "LBA Status Information Notices"),
                (14, "Endurance Group Aggregate Log Change"),
                (31, "Zone Descriptor Changed"),
            ],
        )
    );

    let attributes = id.ctratt; // CTRATT: Controller Attributes
    println!(
        "Attributes (CTRATT)     : {}",
        flags(
            attributes,
            &[
                (0, "128-bit Host Identifier"),
                (1, "Non-Operational Power State Permissive"),
                (2, "NVM Sets"),
                (3, "Read Recovery Levels"),
                (4, "Endurance Groups"),
                (5, "Predictable Latency Mode"),
                (6, "Traffic Based Keep Alive"),
                (7, "Namespace Granularity"),
                (8, "SQ Associations"),
                (9, "UUID List"),
            ],
        )
    );

    let firmware = id.frmw as u32; // FRMW: Firmware Updates
    let firmware_slots = (firmware >> 1) & 0x7;
    println!(
        "Firmware (FRMW)         : {firmware_slots} slots, {}{}",
        if firmware & 0x1 != 0 {
            "slot 1 read-only"
        } else {
            "slot 1 writable"
        },
        if firmware & (1 << 4) != 0 {
            ", activate without reset"
        } else {
            ""
        }
    );

    let log_page = id.lpa as u32; // LPA: Log Page Attributes
    println!(
        "Log pages (LPA)         : {}",
        flags(
            log_page,
            &[
                (0, "SMART/Health per-namespace"),
                (1, "Commands Supported and Effects"),
                (2, "Extended Get Log Page"),
                (3, "Telemetry"),
                (4, "Persistent Event"),
            ],
        )
    );

    let format_attributes = id.fna as u32; // FNA: Format NVM Attributes
    println!(
        "Format (FNA)            : {}",
        flags(
            format_attributes,
            &[
                (0, "Format applies to all namespaces"),
                (1, "Secure erase applies to all namespaces"),
                (2, "Cryptographic erase"),
            ],
        )
    );

    let write_cache = id.vwc; // VWC: Volatile Write Cache
    println!(
        "Volatile Write Cache    : {}",
        if write_cache & 0x1 != 0 {
            "present"
        } else {
            "not present"
        }
    );

    let sgl = id.sgls; // SGLS: SGL Support
    let sgl_support = match sgl & 0x3 {
        0 => "not supported",
        1 => "supported, no alignment requirement",
        2 => "supported, dword aligned",
        _ => "reserved",
    };
    println!("SGL Support (SGLS)      : {sgl_support}");
    if sgl & 0x3 != 0 {
        println!(
            "SGL Features (SGLS)     : {}",
            flags(
                sgl,
                &[
                    (2, "Keyed SGL Data Block"),
                    (16, "Bit Bucket descriptor"),
                    (17, "Byte-aligned metadata buffer"),
                    (18, "Oversized SGL length"),
                    (19, "MPTR SGL descriptor"),
                    (20, "Address as offset"),
                    (21, "Transport SGL Data Block"),
                ],
            )
        );
    }
    Ok(())
}
