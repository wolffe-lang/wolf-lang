//! Linux-only (the reactor's platform posture, same as the task
//! layer): the real body lives in linux_only/no_io.rs — a
//! subdirectory so cargo does not treat it as its own target;
//! elsewhere this target is an empty main.
#[cfg(target_os = "linux")]
#[path = "linux_only/no_io.rs"]
mod imp;

#[cfg(target_os = "linux")]
fn main() {
    imp::main()
}

#[cfg(not(target_os = "linux"))]
fn main() {}
