//! linux-gnu: the real body lives in linux_only/arena_limit.rs, in a
//! subdirectory so cargo does not treat it as its own target. glibc's
//! arenas are glibc's, so elsewhere this target is an empty main.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[path = "linux_only/arena_limit.rs"]
mod imp;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn main() {
    imp::main()
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn main() {}
