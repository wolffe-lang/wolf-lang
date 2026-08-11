//! The s28 native runtime surface — the symbols compiled wolf code
//! links against. Minimal by law (D15): a trap reporter and the v0
//! bump-region allocator, nothing else. `wolf_codegen_clif` emits
//! calls to these names; the driver's link step pulls in this crate's
//! staticlib. s31 grows this into the real runtime model; the SYMBOL
//! CONTRACT here is what s28 freezes.
//!
//! # The trap contract
//!
//! Checked-arithmetic failures, assertion failures, and every other
//! deterministic fault (X3) reach [`__wolf_rt_trap`] with a kind code
//! from the closed conformance vocabulary. The reporter writes one
//! machine-readable line to stderr — `wolf-trap: <kind>` — and exits
//! with [`TRAP_EXIT_CODE`], so harnesses (conform-run's native rung,
//! the differ) recover the trap IDENTITY, not just "it died". The
//! codes are this module's; codegen and the runtime share them through
//! this single authority.
//!
//! # The region contract (v0)
//!
//! `region.new`/`region.alloc`/`region.free` lower to the shims below:
//! a bump allocator over malloc'd chunks, wholesale-freed. Tokens are
//! erased at codegen — ordering is the code's; the shims only move
//! memory. `sync.freeze` is a no-op at runtime in v0 (freezing is a
//! verifier-enforced capability change, not yet a page-protection
//! one); `rc.dup`/`rc.drop` will land with the shared tier (s42).

use std::io::Write as _;

/// Exit code of a trapped wolf program (128 + SIGABRT by convention;
/// deterministic, never a real signal — the trap is a reported outcome,
/// not a crash).
pub const TRAP_EXIT_CODE: i32 = 134;

/// Trap kind codes — the closed s06 conformance vocabulary, numbered.
/// Codegen passes these as immediates; [`trap_kind_name`] spells them.
pub mod trap_code {
    pub const OVERFLOW: i32 = 1;
    pub const DIV_ZERO: i32 = 2;
    pub const BOUNDS: i32 = 3;
    pub const ASSERT: i32 = 4;
    pub const USE_AFTER_MOVE: i32 = 5;
    pub const EXCLUSIVITY: i32 = 6;
    pub const REGION_FAULT: i32 = 7;
    pub const STALE_HANDLE: i32 = 8;
    pub const ALLOC_CONTRACT: i32 = 9;
    pub const RACE: i32 = 10;
    pub const UB: i32 = 11;
    /// `[conc.deadlock.trap]` / `[conc.deadlock.self]` — added to the
    /// closed `[conf.trap.set]` by the spec/03 amendment (s33: the
    /// `when` runtime's self-acquisition detection fires it).
    pub const DEADLOCK: i32 = 12;
}

/// The vocabulary name of a trap code (`trap(<name>)` in verdicts).
pub fn trap_kind_name(code: i32) -> &'static str {
    match code {
        trap_code::OVERFLOW => "overflow",
        trap_code::DIV_ZERO => "div-zero",
        trap_code::BOUNDS => "bounds",
        trap_code::ASSERT => "assert",
        trap_code::USE_AFTER_MOVE => "use-after-move",
        trap_code::EXCLUSIVITY => "exclusivity",
        trap_code::REGION_FAULT => "region-fault",
        trap_code::STALE_HANDLE => "stale-handle",
        trap_code::ALLOC_CONTRACT => "alloc-contract",
        trap_code::RACE => "race",
        trap_code::UB => "ub",
        trap_code::DEADLOCK => "deadlock",
        _ => "unknown",
    }
}

/// A deterministic fault fired (X3). Reports the kind on stderr in the
/// machine-readable form `wolf-trap: <kind>` and exits with
/// [`TRAP_EXIT_CODE`]. Never returns.
///
/// # Safety
///
/// Callable from any thread at any time; takes no pointers.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_trap(kind: i32) -> ! {
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "wolf-trap: {}", trap_kind_name(kind));
    let _ = err.flush();
    std::process::exit(TRAP_EXIT_CODE)
}

/// `main` returned an error value (D30, s29 `[abi.err]`): the
/// documented process behavior is `error: <tag name>` on STDOUT
/// (matching the reference interpreter) and exit 1 — an error return
/// is a reported outcome, never a trap and never an unwind. The tag's
/// name arrives as immediates from the entry shim's compile-time tag
/// dispatch: `len` bytes (0 for an unknown tag — then the numeric
/// `tag` id is reported), packed little-endian into `w0..w3` (32-byte
/// cap; longer names truncate — tags that long do not exist in the
/// corpus and the cap is the shim's, not the language's).
///
/// # Safety
///
/// Callable from any thread at any time; takes no pointers.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_main_err(tag: i64, len: i64, w0: i64, w1: i64, w2: i64, w3: i64) -> ! {
    let mut out = std::io::stdout().lock();
    let len = len.clamp(0, 32) as usize;
    if len == 0 {
        let _ = writeln!(out, "error: {tag}");
    } else {
        let mut bytes = [0u8; 32];
        for (i, w) in [w0, w1, w2, w3].into_iter().enumerate() {
            bytes[i * 8..(i + 1) * 8].copy_from_slice(&w.to_le_bytes());
        }
        let name = String::from_utf8_lossy(&bytes[..len]);
        let _ = writeln!(out, "error: {name}");
    }
    let _ = out.flush();
    std::process::exit(1)
}

// ---- the v0 print path (s31) ---------------------------------------------
//
// `print`/`print_raw` lower to per-segment calls: literal chunks and
// str values through [`__wolf_rt_print_str`], integer holes through
// [`__wolf_rt_print_i64`], bool holes through [`__wolf_rt_print_bool`].
// Formatting matches the reference interpreter's `format_value`
// (Rust `to_string` on both sides — the parity is by construction).
// Every call flushes: the process exits through the C entry shim, so
// no Rust exit hook ever runs to drain a buffer.

/// Write `len` bytes at `ptr` to stdout, flushed.
///
/// # Safety
///
/// `ptr` must point at `len` readable bytes (compiled wolf code passes
/// rodata or checked str values only).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_print_str(ptr: *const u8, len: i64) {
    if ptr.is_null() || len <= 0 {
        return;
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len as usize) };
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(bytes);
    let _ = out.flush();
}

/// Write a signed 64-bit integer in decimal to stdout, flushed.
///
/// # Safety
///
/// Callable from any thread at any time; takes no pointers.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_print_i64(v: i64) {
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "{v}");
    let _ = out.flush();
}

/// Write `true`/`false` to stdout, flushed. The parameter is `i8` —
/// WIR bools cross the call boundary as one byte (only the low byte of
/// the register is defined under SysV).
///
/// # Safety
///
/// Callable from any thread at any time; takes no pointers.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_print_bool(v: i8) {
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "{}", v != 0);
    let _ = out.flush();
}

// ---- the v0 bump-region allocator ----------------------------------------

const CHUNK_MIN: usize = 16 * 1024;
const ALIGN: usize = 16;

/// The proc-ledger seam (s34, sprint Target 2): when procs exist, the
/// proc layer accounts every region create/free against its owning
/// proc (count + bytes — the per-proc resource accounting Erlang
/// never had) and bulk-frees the ledger on proc exit. The seam is a
/// function-pointer table installed by the proc registry's lazy init:
/// a binary that never spawns a proc never installs it, keeps zero
/// static reference from region code into the proc layer (D15 — the
/// `--gc-sections` link drops the registry outright), and pays one
/// null-check per region create/free.
pub(crate) struct RegionLedgerHooks {
    /// A region was created; `handle` is its opaque address.
    pub on_new: fn(handle: usize),
    /// A region is about to free; `bytes` is its ledger weight.
    pub on_free: fn(handle: usize, bytes: usize),
}

static LEDGER_HOOKS: std::sync::atomic::AtomicPtr<RegionLedgerHooks> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

/// Install the proc-ledger hooks (once, from the proc registry's lazy
/// init). The table leaks by design: it lives for the process.
pub(crate) fn install_region_ledger_hooks(hooks: &'static RegionLedgerHooks) {
    LEDGER_HOOKS.store(
        std::ptr::from_ref(hooks).cast_mut(),
        std::sync::atomic::Ordering::SeqCst,
    );
}

fn ledger_hooks() -> Option<&'static RegionLedgerHooks> {
    let p = LEDGER_HOOKS.load(std::sync::atomic::Ordering::SeqCst);
    // SAFETY: only ever null or a leaked 'static table.
    unsafe { p.cast_const().as_ref() }
}

/// A region's current ledger weight in bytes (the proc layer reads it
/// at ownership-transfer seams — `region_transfer`/`region_adopt`).
///
/// # Safety
///
/// `handle` must be a live pointer from [`__wolf_rt_region_new`].
pub(crate) unsafe fn region_bytes(handle: *mut core::ffi::c_void) -> usize {
    // SAFETY: caller contract — live region handle.
    let r: &Region = unsafe { &*handle.cast() };
    r.bytes
}

struct Region {
    /// Owned chunks: (base pointer as a boxed allocation, capacity).
    chunks: Vec<Box<[u8]>>,
    /// Bump cursor within the last chunk.
    used: usize,
    /// Total bytes ever bump-allocated (aligned) — the ledger weight.
    /// Tracked unconditionally (one add on the alloc path); read only
    /// through the proc-ledger seam.
    bytes: usize,
}

/// `region.new` — a fresh region arena. Returns the opaque handle.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_region_new() -> *mut core::ffi::c_void {
    let r = Box::new(Region {
        chunks: Vec::new(),
        used: 0,
        bytes: 0,
    });
    let handle: *mut core::ffi::c_void = Box::into_raw(r).cast();
    if let Some(h) = ledger_hooks() {
        (h.on_new)(handle as usize);
    }
    handle
}

/// `region.alloc` — bump-allocate `size` bytes (16-aligned) in the
/// region behind `handle`.
///
/// # Safety
///
/// `handle` must be a live pointer from [`__wolf_rt_region_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_region_alloc(
    handle: *mut core::ffi::c_void,
    size: i64,
) -> *mut u8 {
    let r: &mut Region = unsafe { &mut *handle.cast() };
    let size = usize::try_from(size).unwrap_or_else(|_| {
        __wolf_rt_trap(trap_code::ALLOC_CONTRACT);
    });
    let size = size.next_multiple_of(ALIGN).max(ALIGN);
    let need_new_chunk = match r.chunks.last() {
        Some(c) => r.used + size > c.len(),
        None => true,
    };
    if need_new_chunk {
        let cap = size.max(CHUNK_MIN);
        r.chunks.push(vec![0u8; cap].into_boxed_slice());
        r.used = 0;
    }
    let chunk = r.chunks.last_mut().expect("chunk exists");
    let p = unsafe { chunk.as_mut_ptr().add(r.used) };
    r.used += size;
    r.bytes += size;
    p
}

/// `region.free` — wholesale-free the region. The handle is dead after
/// this call (the WIR verifier's token linearity is the static proof
/// no allocation outlives it).
///
/// # Safety
///
/// `handle` must be a live pointer from [`__wolf_rt_region_new`]; no
/// pointer into the region may be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_region_free(handle: *mut core::ffi::c_void) {
    // SAFETY: caller contract — live handle, dead after this call.
    let r = unsafe { Box::<Region>::from_raw(handle.cast()) };
    if let Some(h) = ledger_hooks() {
        (h.on_free)(handle as usize, r.bytes);
    }
    drop(r);
}

/// `sync.freeze` — v0 no-op (freezing is a capability change enforced
/// statically; page protection is a later, checked-profile upgrade).
///
/// # Safety
///
/// `handle` must be a live pointer from [`__wolf_rt_region_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_region_freeze(_handle: *mut core::ffi::c_void) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_regions_allocate_and_free() {
        let h = __wolf_rt_region_new();
        unsafe {
            let a = __wolf_rt_region_alloc(h, 8);
            let b = __wolf_rt_region_alloc(h, 24);
            assert!(!a.is_null() && !b.is_null());
            assert_ne!(a, b);
            assert_eq!(a.addr() % ALIGN, 0);
            assert_eq!(b.addr() % ALIGN, 0);
            // Big allocation forces a new chunk.
            let c = __wolf_rt_region_alloc(h, (CHUNK_MIN as i64) + 1);
            assert!(!c.is_null());
            __wolf_rt_region_free(h);
        }
    }

    #[test]
    fn trap_names_cover_the_vocabulary() {
        for (code, name) in [
            (trap_code::OVERFLOW, "overflow"),
            (trap_code::DIV_ZERO, "div-zero"),
            (trap_code::ASSERT, "assert"),
            (trap_code::BOUNDS, "bounds"),
            (trap_code::UB, "ub"),
            (trap_code::DEADLOCK, "deadlock"),
        ] {
            assert_eq!(trap_kind_name(code), name);
        }
        assert_eq!(trap_kind_name(999), "unknown");
    }
}
