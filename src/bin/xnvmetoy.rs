use std::process::ExitCode;

fn main() -> ExitCode {
    // SAFETY: the version getters take no arguments and touch no device state.
    let (major, minor, patch) = unsafe {
        (
            xnvme_sys::xnvme_ver_major(),
            xnvme_sys::xnvme_ver_minor(),
            xnvme_sys::xnvme_ver_patch(),
        )
    };
    println!("xnvmetoy against libxnvme {major}.{minor}.{patch}");
    ExitCode::SUCCESS
}
