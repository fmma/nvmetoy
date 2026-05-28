#![warn(clippy::undocumented_unsafe_blocks)]

#[macro_export]
macro_rules! chat {
    ($($arg:tt)*) => {{
        eprintln!($($arg)*);
    }};
}

pub fn pause() {
    eprint!("\n    --- press Enter to continue ---");
    let _ = std::io::stdin().read_line(&mut String::new());
}

pub mod dma_region;
pub mod nvme_bar;
pub mod nvme_command_builders;
pub mod nvme_identify;
pub mod nvme_spec;
pub mod nvmetoy;
pub mod queue_pair;

pub use nvme_spec::{CompletionQueueEntry, SubmissionQueueEntry};
pub use nvmetoy::NvmeUserspaceDriver;
