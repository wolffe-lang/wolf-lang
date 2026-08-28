//! Linux+macOS (the task layer's platform posture): the real body lives
//! in linux_only/no_spawn.rs — a subdirectory so cargo does not treat it
//! as its own target; elsewhere this target is an empty main.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "linux_only/no_spawn.rs"]
mod imp;

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn main() {
    imp::main()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn main() {}
