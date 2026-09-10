//! linux: the real body lives in linux_only/syscall_shape.rs — a
//! subdirectory so cargo does not treat it as its own target;
//! elsewhere this target is an empty main. The instrument is
//! `strace`, which is what the count in wolf-lang#289/#290 was read
//! with, so this witness and lobo's measurement are the same
//! measurement.
#[cfg(target_os = "linux")]
#[path = "linux_only/syscall_shape.rs"]
mod imp;

#[cfg(target_os = "linux")]
fn main() {
    imp::main()
}

#[cfg(not(target_os = "linux"))]
fn main() {}
