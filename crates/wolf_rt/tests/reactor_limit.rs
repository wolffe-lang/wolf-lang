//! Linux and macOS (the unix reactor hosts): the real body lives in
//! linux_only/reactor_limit.rs — a subdirectory so cargo does not treat
//! it as its own target; elsewhere this target is an empty main.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "linux_only/reactor_limit.rs"]
mod imp;

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn main() {
    imp::main()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn main() {}
