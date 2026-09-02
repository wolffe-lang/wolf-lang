//! Linux, macOS, windows (the task layer's platform posture): the real body lives
//! in linux_only/proc_root.rs — a subdirectory so cargo does not treat it
//! as its own target; elsewhere this target is an empty main.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[path = "linux_only/proc_root.rs"]
mod imp;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn main() {
    imp::main()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn main() {}
