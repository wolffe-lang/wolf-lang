//! The freestanding runtime (kw12; spec/04 `[abi.target.none.alloc]`,
//! STATUS #31 K8(b)) — the `no_std` half of the region runtime, linked
//! into a program built for `x86_64-unknown-none` as
//! `libwolf_rt_none.a`.
//!
//! It defines the runtime symbols of the allocating constructs a kernel
//! may use — `List`, `Map`, string interpolation, a capturing closure
//! and `region` — under the SAME names, signatures and layouts the
//! hosted `wolf_rt` defines, so both compiling tiers lower those
//! constructs exactly as they do on a hosted target. Every byte it hands
//! out comes from the program's allocator hook; it imports nothing but
//! the hook list (`[abi.target.none.hooks]`):
//!
//! - `wolf_alloc(size: i64, align: i64) -> *u8` and
//!   `wolf_free(p: *u8, size: i64, align: i64)`, the allocator pair the
//!   program supplies ([`native`] states the contract);
//! - `wolf_trap(kind, file, file_len, line, col)`, which a runtime fault
//!   reaches site-less (`(null, 0, 0, 0)`), as the hosted runtime's
//!   `__wolf_rt_trap` is site-less;
//! - the C memory functions, which `compiler_builtins` defines here as
//!   WEAK symbols, so a program's own `memcpy` wins at the link.
//!
//! What is shared and what is not. [`list`] and [`map`] are the hosted
//! runtime's own source files, compiled a second time without std: one
//! text, two builds, so the container layouts cannot drift. The region,
//! the root arena and the strbuf are this crate's — the hosted versions
//! stand on `Box`, thread-locals, a `Mutex` and `String` — and keep the
//! hosted contracts: 16-aligned grants, the ledger and its cap, the
//! ambient slot, the root region that never frees.
//!
//! One CPU at a time. The target has no threads (`spawn` is refused on
//! it), so the ambient slot and the live-bytes counter are single
//! words, not per-thread state; a kernel that runs wolf code on several
//! CPUs at once must not allocate from two of them concurrently — the
//! runtime takes no lock.

#![no_std]

// The hosted runtime's container families, compiled no_std. They read
// `crate::native::{ambient_region, __wolf_rt_region_alloc}` and
// `crate::str::ambient_alloc`, which this crate defines below with the
// hosted signatures. Helpers the hosted crate's other modules use (byte
// minting for fs/net, `par`'s raw parts) have no caller here.
//
// Every module is `cfg(not(test))`: the shared files carry unit tests
// written against the HOSTED crate, and `clippy --all-targets` compiles
// the lib-test target even with `test = false`. This crate's tests are
// `tests/*.rs`, which link the ordinary build.
#[cfg(not(test))]
#[allow(dead_code)]
#[path = "../../wolf_rt/src/list.rs"]
pub mod list;
#[cfg(not(test))]
#[allow(dead_code)]
#[path = "../../wolf_rt/src/map.rs"]
pub mod map;

#[cfg(not(test))]
mod fmt;
#[cfg(not(test))]
pub mod native;
#[cfg(not(test))]
pub mod str;

/// A runtime-internal panic is a runtime bug: the kernel gets a trap
/// (`ub`), never an unwind. Only on the freestanding target — a host
/// build links std's handler (the integration tests).
#[cfg(all(target_os = "none", not(test)))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    native::trap(native::trap_code::UB)
}
