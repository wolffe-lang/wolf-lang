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
//!
//! # The ambient region (s76, wolf-lang#81)
//!
//! `[mem.region.create.3]`: an allocation lands in the AMBIENT region
//! at its site, and D12 says a callee allocates into its CALLER's
//! region by default — so "ambient" is a DYNAMIC property of the call
//! stack, not a lexical one, exactly as `wolf_mem`'s checker models it
//! (its own `ambient` stack). The native tier realizes it as one
//! thread-local slot: lowering brackets every construct that opens a
//! region ([`__wolf_rt_region_ambient_enter`] after `region.new`,
//! [`__wolf_rt_region_ambient_leave`] on the X4 cleanup chain, so every
//! exit edge restores it), and the container runtime asks
//! [`ambient_region`] where to allocate. A null slot means the process
//! root (`crate::str`'s c08 arena) — `main`'s enclosing region is the
//! process, so a program with no `region` block is unchanged.
//!
//! Enter/leave SAVE AND RESTORE rather than push/pop a stack: an
//! unbalanced leave can then only restore an older handle, never
//! corrupt a depth. The slot is per-thread, so a spawned task starts at
//! the process root (a task's allocations are not the spawner's
//! region's — the region-transfer seam is how a region crosses a spawn),
//! and `crate::task::pool` clears it around every task body so a
//! REUSED pool worker can never inherit a dead handle.

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

/// Render the trap report (s125). The FIRST line is the machine
/// contract and stays byte-identical to what it has been since s28 —
/// `wolf-trap: <kind>` with the whole remainder as the kind (both
/// harness parsers take it that way), so the site goes on its OWN
/// line:
///
/// ```text
/// wolf-trap: bounds
///   at exmpl.lu:3:13
/// ```
///
/// The site line is additive and optional (a site-less trap prints
/// exactly the one line it always did); parsers that only want the
/// kind ignore it by construction — it does not start with the
/// `wolf-trap:` prefix.
fn write_trap_report(w: &mut impl std::io::Write, kind: i32, site: Option<(&str, u64, u64)>) {
    let _ = writeln!(w, "wolf-trap: {}", trap_kind_name(kind));
    if let Some((file, line, col)) = site {
        let _ = writeln!(w, "  at {file}:{line}:{col}");
    }
}

/// The proc-containment seam (s132, D68): when procs exist, a trap on
/// a task INSIDE a proc is contained at the proc boundary — the proc
/// dies with reason `fault(kind)`, the process lives ([conc.proc.1],
/// [conc.proc.exit]). Same D15 posture as [`RegionLedgerHooks`]: the
/// proc registry's lazy init installs the hook, a no-proc binary
/// links none of the proc layer and pays one null-check on the trap
/// path (which is already cold and terminal). The hook DIVERGES when
/// it contains (the trapping thread is parked — compiled frames are
/// never unwound, `[abi.native.nounwind]`) and returns when the trap
/// is not containable (root domain, or no proc on this task): then
/// the process-exit path below proceeds exactly as it always did.
static TRAP_CONTAIN: std::sync::atomic::AtomicPtr<()> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

/// Install the trap-containment hook (once, from the proc registry's
/// lazy init).
pub(crate) fn install_trap_containment(hook: fn(i32)) {
    TRAP_CONTAIN.store(hook as *mut (), std::sync::atomic::Ordering::SeqCst);
}

/// Give the proc layer its chance to contain `kind` — diverges if it
/// does; returns if the trap belongs to the process.
fn try_contain_trap(kind: i32) {
    let p = TRAP_CONTAIN.load(std::sync::atomic::Ordering::SeqCst);
    if !p.is_null() {
        // SAFETY: only ever null or the fn(i32) stored above.
        let hook: fn(i32) = unsafe { core::mem::transmute::<*mut (), fn(i32)>(p) };
        hook(kind);
    }
}

/// A deterministic fault fired (X3). Reports the kind on stderr in the
/// machine-readable form `wolf-trap: <kind>` and exits with
/// [`TRAP_EXIT_CODE`]. Never returns. Codegen emits this form for
/// sites with no source coordinates (synthetic functions, spanless
/// instructions); user-code sites carry them via [`__wolf_rt_trap_at`].
/// A trap on a task inside a proc never reaches the report: it is
/// contained at the proc boundary as reason `fault(kind)` (D68,
/// [`try_contain_trap`]) — the `wolf-trap:` stderr line is the
/// PROCESS outcome's, and a contained trap is not a process outcome.
///
/// # Safety
///
/// Callable from any thread at any time; takes no pointers.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_trap(kind: i32) -> ! {
    try_contain_trap(kind);
    let mut err = std::io::stderr().lock();
    write_trap_report(&mut err, kind, None);
    let _ = err.flush();
    // The fault path dumps too (s45): the counters up to the trap are
    // exactly the execution that happened, and a program that traps is
    // the one you most want a profile of. No-op in a normal build.
    crate::prof::dump_on_exit();
    std::process::exit(TRAP_EXIT_CODE)
}

/// [`__wolf_rt_trap`] with the trap SITE (s125): the first stderr line
/// is byte-identical to the site-less form (the harness ABI), and a
/// second line names where — `  at <file>:<line>:<col>`, 1-based,
/// pointing at the statement whose check fired (the WIR srcspan the
/// trap check carries). `file`/`file_len` name rodata bytes the
/// backend interned per source file; a null/empty file degrades to the
/// one-line report rather than inventing a site.
///
/// # Safety
///
/// `file` must be null or point at `file_len` readable bytes that live
/// for the call (compiled wolf code passes rodata).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_trap_at(
    kind: i32,
    file: *const u8,
    file_len: i64,
    line: i64,
    col: i64,
) -> ! {
    // Containment first (D68): inside a proc the site rides the
    // reason's fault kind, not stderr — the proc dies, not the
    // process, and the harness line would misreport an outcome.
    try_contain_trap(kind);
    let mut err = std::io::stderr().lock();
    if file.is_null() || file_len <= 0 {
        write_trap_report(&mut err, kind, None);
    } else {
        // SAFETY: caller contract — `file_len` readable bytes.
        let bytes = unsafe { core::slice::from_raw_parts(file, file_len as usize) };
        let name = String::from_utf8_lossy(bytes);
        write_trap_report(&mut err, kind, Some((&name, line as u64, col as u64)));
    }
    let _ = err.flush();
    crate::prof::dump_on_exit();
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
    crate::prof::dump_on_exit();
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
    crate::io::write_stream(crate::io::STREAM_STDOUT, bytes);
}

/// Write a signed 64-bit integer in decimal to stdout, flushed.
///
/// # Safety
///
/// Callable from any thread at any time; takes no pointers.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_print_i64(v: i64) {
    crate::io::write_stream(crate::io::STREAM_STDOUT, v.to_string().as_bytes());
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
    let text = if v != 0 { "true" } else { "false" };
    crate::io::write_stream(crate::io::STREAM_STDOUT, text.as_bytes());
}

// ---- the v0 bump-region allocator ----------------------------------------

/// First-chunk size, and the base of the geometric chunk ladder below.
///
/// s76 shrank this from 16 KiB: a region's first chunk is a fixed cost
/// every region pays on its first allocation, and at 16 KiB that cost
/// dominated the whole point of the thing — once s76 made a scratch
/// region actually hold its container, `b3_churn` — one region per
/// request, ~430 bytes of it used — paid a 16 KiB calloc per request
/// and ran 3x SLOWER than the leaking version it replaced. The ladder
/// is the standard arena answer: start at a page-ish chunk, double per
/// chunk, cap out, so a small scratch region is cheap and a large one
/// still allocates O(log n) times.
///
/// #113 removed the zeroing and the per-cycle heap traffic: chunks are
/// `MaybeUninit` capacity (a bump allocator never needs zeroed memory)
/// and retire to a per-thread pool instead of the host allocator — see
/// [`take_chunk`]/[`retire_chunk`] for where the s76 determinism aid
/// (debug builds read a stable 0) now lives.
const CHUNK_MIN: usize = 1024;
/// Ceiling of the geometric ladder — past this, chunks stay this size
/// (an allocation larger than the ceiling still gets its own exact
/// chunk).
const CHUNK_MAX: usize = 1024 * 1024;
const ALIGN: usize = 16;

/// The size of a region's `n`-th chunk when it needs at least `size`
/// bytes: `CHUNK_MIN << n`, capped at [`CHUNK_MAX`], never below `size`.
fn chunk_size(nth: usize, size: usize) -> usize {
    // Both bounds are powers of two (asserted in the ladder test), so
    // the ladder has exactly this many doublings — clamping `nth` to it
    // keeps the shift in range without an overflow dance.
    let steps = (CHUNK_MAX.trailing_zeros() - CHUNK_MIN.trailing_zeros()) as usize;
    size.max(CHUNK_MIN << nth.min(steps))
}

/// One 16-byte, 16-aligned block of chunk capacity. `Chunk` stores
/// BLOCKS rather than bytes so the box allocation itself carries a
/// formal `Layout` alignment of [`ALIGN`] — the base guarantee every
/// 16-grained grant stands on (s101). Before this type the base was
/// `Box<[MaybeUninit<u8>]>`, formal alignment ONE: the `align 16` the
/// LLVM tier has claimed on `__wolf_rt_region_alloc` results since
/// report 10 delta 2 was resting on the host allocator's habit, and
/// a habit is not a guarantee (the D44-addendum hole class, wearing
/// an alignment hat).
#[repr(C, align(16))]
pub(crate) struct ChunkBlock([core::mem::MaybeUninit<u8>; ALIGN]);

/// A raw arena chunk: uninitialized, 16-ALIGNED capacity behind a bump
/// cursor. Byte-flavored accessors keep every call site written in
/// bytes; the block representation is this type's private business.
pub(crate) struct Chunk(Box<[ChunkBlock]>);

impl Chunk {
    /// Capacity in bytes.
    pub(crate) fn len(&self) -> usize {
        self.0.len() * ALIGN
    }

    /// Base pointer — 16-aligned by the element type's layout.
    pub(crate) fn as_mut_ptr(&mut self) -> *mut core::mem::MaybeUninit<u8> {
        self.0.as_mut_ptr().cast()
    }
}

/// A fresh chunk of `cap` bytes (`cap` 16-grained — every caller
/// rounds; the ladder's rungs and the arenas' minima all are), WITHOUT
/// the host allocator zeroing it (#113: the memset was 6.1% of
/// `b3_churn`'s per-request instructions, and a bump allocator never
/// needs zeroed memory). The read-before-write that zeroing papered
/// over cannot happen through the language: use-of-uninit is E1001
/// (`wolf_mem::moves`, forward maybe-uninit dataflow), container
/// access is len-bounded, and ubcheck's L1 tracks per-byte
/// initialization in its OWN shadow store, never through this memory.
/// DEBUG builds keep the s76 determinism aid — a latent
/// runtime/lowering bug reads a stable 0, not garbage — exactly where
/// bugs are hunted; the release runtime pays nothing (the D21 posture:
/// "release builds get the plain arena/pool paths").
pub(crate) fn new_chunk(cap: usize) -> Chunk {
    debug_assert_eq!(cap % ALIGN, 0, "chunk capacities are 16-grained");
    // SAFETY: `ChunkBlock` is `MaybeUninit` bytes through and through —
    // every bit pattern is a valid value, so the init assertion asserts
    // nothing.
    let mut c = Chunk(unsafe { Box::new_uninit_slice(cap / ALIGN).assume_init() });
    #[cfg(debug_assertions)]
    // SAFETY: `c` owns `cap` writable bytes.
    unsafe {
        core::ptr::write_bytes(c.as_mut_ptr().cast::<u8>(), 0, cap);
    }
    debug_assert_eq!(c.as_mut_ptr().addr() % ALIGN, 0);
    c
}

/// Retired ladder chunks awaiting reuse, per thread (#113). A bump
/// allocator's chunks are interchangeable, so a freed region's chunks
/// come here instead of going back to the host allocator — `b3_churn`
/// paid two mallocs, two frees and a memset per request for a design
/// whose report calls it "a pointer bump and a pointer reset"
/// (reports/01 fact 5); with the pool warm, a region cycle is exactly
/// that. Thread-local like [`AMBIENT_REGION`] (zero contention on the
/// hot path; workers are reused, so pools stay warm); capped so a
/// burst cannot squat on memory; ladder-sized chunks only, so the
/// exact-size match below stays a short scan.
struct ChunkPool {
    chunks: Vec<Chunk>,
    bytes: usize,
}

/// Pool ceiling per thread — two max-ladder chunks. Past it, retired
/// chunks go back to the host allocator like they always did.
const POOL_MAX_BYTES: usize = 2 * CHUNK_MAX;
/// Retired `Region` headers awaiting reuse (their chunk `Vec`s keep
/// their capacity, so a warm cycle re-mallocs nothing at all).
const REGION_POOL_MAX: usize = 8;

/// Both per-thread pools behind ONE thread-local and ONE `RefCell`:
/// a region free retires chunks and parks its header in a single
/// visit (the split design ran the TLS+borrow sequence twice).
struct RtPools {
    chunks: ChunkPool,
    // The Box IS the pooled resource: `region_new` hands out the box's
    // pointer as the region handle (`Box::into_raw`), so pooling
    // `Region` by value would re-malloc the very allocation the pool
    // exists to keep.
    #[allow(clippy::vec_box)]
    regions: Vec<Box<Region>>,
}

thread_local! {
    static POOLS: core::cell::RefCell<RtPools> = const {
        core::cell::RefCell::new(RtPools {
            chunks: ChunkPool { chunks: Vec::new(), bytes: 0 },
            regions: Vec::new(),
        })
    };
}

/// A chunk of exactly `cap` bytes: pooled when one is waiting, fresh
/// otherwise. Debug builds zero the pooled path too — reuse must not
/// be less deterministic than a fresh chunk.
fn take_chunk(cap: usize) -> Chunk {
    let pooled = POOLS.with(|p| {
        let p = &mut p.borrow_mut().chunks;
        match p.chunks.iter().rposition(|c| c.len() == cap) {
            Some(i) => {
                p.bytes -= cap;
                Some(p.chunks.swap_remove(i))
            }
            None => None,
        }
    });
    match pooled {
        #[allow(unused_mut)]
        Some(mut c) => {
            #[cfg(debug_assertions)]
            // SAFETY: `c` owns `cap` writable bytes.
            unsafe {
                core::ptr::write_bytes(c.as_mut_ptr().cast::<u8>(), 0, cap);
            }
            // The base guarantee holds for POOLED chunks by the same
            // type-level fact as fresh ones (s101); the assertion pins
            // reuse to it all the same.
            debug_assert_eq!(c.as_mut_ptr().addr() % ALIGN, 0);
            c
        }
        None => new_chunk(cap),
    }
}

/// Retire one chunk: ladder-sized chunks pool (up to the cap), odd
/// sizes — an oversize allocation's exact chunk — and overflow drop to
/// the host allocator exactly as before #113.
fn retire_chunk_into(p: &mut ChunkPool, c: Chunk) {
    let n = c.len();
    if !(n.is_power_of_two() && (CHUNK_MIN..=CHUNK_MAX).contains(&n)) {
        return;
    }
    if p.bytes + n <= POOL_MAX_BYTES {
        p.bytes += n;
        p.chunks.push(c);
    }
}

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
/// at ownership-transfer seams — `region_transfer`/`region_adopt`; the
/// s76 region litmus reads it to attribute a container's storage to the
/// region that was ambient at its allocation site).
///
/// # Safety
///
/// `handle` must be a live pointer from [`__wolf_rt_region_new`].
pub unsafe fn region_bytes(handle: *mut core::ffi::c_void) -> usize {
    // SAFETY: caller contract — live region handle.
    let r: &Region = unsafe { &*handle.cast() };
    r.bytes
}

/// The `region_bytes(r)` builtin's shim (s131, #187): the region's
/// ledger weight, surfaced to wolf code. The number is the exact,
/// alignment-rounded total ever charged to the region — monotone
/// within the region's lifetime (a container's growth charges the new
/// buffer without discharging the abandoned one), zero at creation.
///
/// # Safety
///
/// `handle` must be a live pointer from [`__wolf_rt_region_new`] —
/// which the lowering guarantees: the builtin takes a named region
/// binding, and the static tiers refuse a use after free/freeze.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_region_bytes(handle: *mut core::ffi::c_void) -> i64 {
    // SAFETY: caller contract — live region handle.
    (unsafe { region_bytes(handle) }) as i64
}

/// Chunk capacity currently owned by LIVE regions, process-wide — the
/// runtime's own reclamation accounting (s76 target 3). Maintained at
/// CHUNK granularity, so the per-allocation bump path pays nothing: a
/// counter bump happens only when a region takes a new chunk from the
/// system allocator and when a region hands its chunks back.
///
/// This is the number the region litmus asserts on. RSS would answer
/// the same question less deterministically (the system allocator may
/// keep freed pages, and the reading is noisy); this counter moves for
/// exactly one reason.
static LIVE_REGION_BYTES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Read [`LIVE_REGION_BYTES`].
pub fn live_region_bytes() -> usize {
    LIVE_REGION_BYTES.load(std::sync::atomic::Ordering::Relaxed)
}

/// The `live_region_bytes()` builtin's shim (s131, #187): the
/// process-wide chunk capacity owned by live named regions, surfaced
/// to wolf code. Chunk-granular by design (the reclamation counter,
/// not an RSS proxy): it rises when a region takes a chunk, falls
/// wholesale when a region frees, and counts neither the process-root
/// arena nor the retired-chunk pool.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_live_region_bytes() -> i64 {
    live_region_bytes() as i64
}

// ---- the ambient region slot (s76) ---------------------------------------

thread_local! {
    /// The region an allocation with no explicit target lands in
    /// (`[mem.region.create.3]`). Null = the process root.
    static AMBIENT_REGION: core::cell::Cell<*mut core::ffi::c_void> =
        const { core::cell::Cell::new(core::ptr::null_mut()) };
}

/// The ambient region on this thread, or null for the process root.
/// Container allocation (`crate::list`) reads this to decide placement.
pub(crate) fn ambient_region() -> *mut core::ffi::c_void {
    AMBIENT_REGION.with(core::cell::Cell::get)
}

/// Run `f` with the ambient region reset to the process root, restoring
/// it afterwards. The pool wraps every task body in this: workers are
/// REUSED, and a body that left the slot set would hand the next task a
/// handle that is no longer live.
pub(crate) fn with_root_ambient<R>(f: impl FnOnce() -> R) -> R {
    struct Restore(*mut core::ffi::c_void);
    impl Drop for Restore {
        fn drop(&mut self) {
            AMBIENT_REGION.with(|c| c.set(self.0));
        }
    }
    let _g = Restore(AMBIENT_REGION.with(|c| c.replace(core::ptr::null_mut())));
    f()
}

/// Open `handle` as the ambient region for the enclosing construct
/// (`region name { }`, `in r { }`, `freeze region { }`); returns the
/// PREVIOUS ambient handle, which the caller hands back to
/// [`__wolf_rt_region_ambient_leave`] on every exit edge.
///
/// # Safety
///
/// `handle` must be a live pointer from [`__wolf_rt_region_new`], live
/// for as long as it stays ambient.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_region_ambient_enter(
    handle: *mut core::ffi::c_void,
) -> *mut core::ffi::c_void {
    AMBIENT_REGION.with(|c| c.replace(handle))
}

/// Restore the ambient region saved by
/// [`__wolf_rt_region_ambient_enter`]. Emitted on the X4 cleanup chain,
/// so it runs on every exit edge (fall-through, `return`, `?`-err,
/// `break`/`continue` crossing the boundary), ahead of `region.free`.
///
/// # Safety
///
/// `prev` must be the value a matching enter returned, and must still
/// name a live region (or be null).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_region_ambient_leave(prev: *mut core::ffi::c_void) {
    AMBIENT_REGION.with(|c| c.set(prev));
}

struct Region {
    /// Bump cursor into the current (last) chunk. Null when no chunk
    /// is open; `cur == end` (or null/null) sends the next allocation
    /// to the slow path, so the fast path needs no emptiness branch.
    /// The pointers aim into a `Chunk`'s heap storage, which never
    /// moves when the `Box<Region>` header itself is pooled/moved.
    cur: *mut u8,
    /// One past the current chunk's usable end.
    end: *mut u8,
    /// Owned chunks — raw `MaybeUninit` capacity the bump cursor hands
    /// out. The Rust side never reads these bytes (pointers go to
    /// compiled code), so nothing here may materialize a `&[u8]` over
    /// unwritten capacity; `MaybeUninit` is that rule in the type.
    chunks: Vec<Chunk>,
    /// Sum of owned chunk capacities — maintained at take time so
    /// `region_free` subtracts one number instead of walking the
    /// chunk list (the walk was 21 Ir/request on `b3_churn`).
    owned: usize,
    /// Total bytes ever bump-allocated (aligned) — the ledger weight.
    /// Tracked unconditionally (one add on the alloc path); read only
    /// through the proc-ledger seam.
    bytes: usize,
    /// Creation-time byte budget on the LEDGER (`[mem.region.cap.1]`,
    /// D68/#187): an allocation that would take `bytes` past this
    /// traps `alloc-contract` at the site. `usize::MAX` = today's
    /// unbounded region (the absent-cap default), so the hot path pays
    /// exactly one always-predictable compare. The cap is charged in
    /// the ledger's own units ([mem.region.account.1]: aligned charge,
    /// monotone, high-water) and travels with the region across
    /// transfer/adopt — it is the region's for life.
    cap: usize,
}

/// `region.new` — a fresh region arena. Returns the opaque handle.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_region_new() -> *mut core::ffi::c_void {
    // A pooled header was reset at retirement and keeps its chunk
    // Vec's capacity — a warm region cycle allocates nothing (#113).
    let r = POOLS
        .with(|p| p.borrow_mut().regions.pop())
        .unwrap_or_else(|| {
            Box::new(Region {
                cur: core::ptr::null_mut(),
                end: core::ptr::null_mut(),
                chunks: Vec::new(),
                owned: 0,
                bytes: 0,
                cap: usize::MAX,
            })
        });
    let handle: *mut core::ffi::c_void = Box::into_raw(r).cast();
    if let Some(h) = ledger_hooks() {
        (h.on_new)(handle as usize);
    }
    handle
}

/// `region_set_cap` — install the creation-time byte budget
/// (`[mem.region.cap.1]`, D68/#187). The lowering calls this
/// immediately after [`__wolf_rt_region_new`], before any allocation
/// can land in the region, so the budget is creation-time in every
/// observable sense. A negative budget is an allocation-contract
/// violation at the creating site (`[mem.region.cap.2]` — the same
/// contract class as a negative allocation size); zero is a legal
/// budget every charge breaches.
///
/// # Safety
///
/// `handle` must be a live pointer from [`__wolf_rt_region_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_region_set_cap(handle: *mut core::ffi::c_void, cap: i64) {
    if cap < 0 {
        __wolf_rt_trap(trap_code::ALLOC_CONTRACT);
    }
    // SAFETY: caller contract — live region handle.
    let r: &mut Region = unsafe { &mut *handle.cast() };
    r.cap = cap as usize;
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
    // The contract check `try_from` used to make: a negative size
    // traps. Spelled as one sign test so the fast path carries a
    // single predictable branch instead of `Result` machinery.
    if size < 0 {
        __wolf_rt_trap(trap_code::ALLOC_CONTRACT);
    }
    // Round up to the 16-byte grain. `ALIGN` is a power of two, so
    // this is `next_multiple_of` as an add+mask; `.max(ALIGN)` keeps
    // zero-size allocations distinct, as before. No overflow: `size`
    // fits i64 and the add is at most +15.
    let size = ((size as usize) + (ALIGN - 1)) & !(ALIGN - 1);
    let size = size.max(ALIGN);
    // The cap compare (`[mem.region.cap.1]`, D68 — s131's "one field
    // + one compare"): a charge that would take the LEDGER past the
    // budget is trap(alloc-contract) AT this allocating site. At-cap-
    // exactly is not a breach — the next byte is (`>` against the
    // post-charge ledger). Uncapped regions hold `usize::MAX`, so the
    // branch never fires for them; `bytes + size` cannot overflow
    // before real memory does.
    if r.bytes + size > r.cap {
        __wolf_rt_trap(trap_code::ALLOC_CONTRACT);
    }
    // The bump: one compare, one store, one add. A closed region
    // (null cur, null end) fails the compare for any size >= ALIGN,
    // so the empty case needs no branch of its own. `cur` stays
    // 16-aligned: every grant is a multiple of `ALIGN` off a chunk
    // base, and the slow path re-establishes both pointers per chunk.
    let cur = r.cur as usize;
    let next = cur + size;
    if next <= r.end as usize {
        r.cur = next as *mut u8;
        r.bytes += size;
        return cur as *mut u8;
    }
    region_alloc_slow(r, size)
}

/// The chunk-open half of `region.alloc`, kept out of the hot path.
/// Behavior is byte-for-byte the pre-cursor design: the previous
/// chunk's tail is abandoned (the bump never back-fills), the new
/// chunk's capacity follows the ladder, and `LIVE_REGION_BYTES` moves
/// at exactly this boundary.
#[cold]
#[inline(never)]
fn region_alloc_slow(r: &mut Region, size: usize) -> *mut u8 {
    let cap = chunk_size(r.chunks.len(), size);
    let mut c = take_chunk(cap);
    LIVE_REGION_BYTES.fetch_add(cap, std::sync::atomic::Ordering::Relaxed);
    r.owned += cap;
    let base = c.as_mut_ptr().cast::<u8>();
    r.chunks.push(c);
    // SAFETY: `size <= cap` (chunk_size never returns below `size`),
    // so both pointers stay inside or one-past the chunk's storage.
    unsafe {
        r.cur = base.add(size);
        r.end = base.add(cap);
    }
    r.bytes += size;
    base
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
    let mut r = unsafe { Box::<Region>::from_raw(handle.cast()) };
    if let Some(h) = ledger_hooks() {
        (h.on_free)(handle as usize, r.bytes);
    }
    LIVE_REGION_BYTES.fetch_sub(r.owned, std::sync::atomic::Ordering::Relaxed);
    // #113: chunks retire to the per-thread pool (the accounting above
    // moves at exactly the boundary it always did — LIVE bytes count
    // regions, never the pool — with `owned` maintained at take time
    // instead of re-walked here), and the reset header keeps its Vec
    // capacity for the next region on this thread. One TLS visit and
    // one RefCell borrow cover both pools; the split-pool design paid
    // that sequence twice per free (~35 Ir/request on `b3_churn`).
    r.cur = core::ptr::null_mut();
    r.end = core::ptr::null_mut();
    r.owned = 0;
    r.bytes = 0;
    r.cap = usize::MAX;
    POOLS.with(|p| {
        let mut p = p.borrow_mut();
        for c in r.chunks.drain(..) {
            retire_chunk_into(&mut p.chunks, c);
        }
        if p.regions.len() < REGION_POOL_MAX {
            p.regions.push(r);
        }
    });
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

    /// The cap's non-breach half ([mem.region.cap.1]), rt-level: an
    /// uncapped region never compares against a real budget, a capped
    /// region charges to EXACTLY its cap without complaint (at-cap is
    /// not a breach — the next byte is; the breach half fires the
    /// process trap path, so it is pinned where it can be observed:
    /// the proc containment tests in task/proc.rs and the corpus
    /// fault witness).
    #[test]
    fn region_cap_at_cap_exactly_is_not_a_breach() {
        let h = __wolf_rt_region_new();
        // SAFETY: fresh live handle; freed at the end.
        unsafe {
            __wolf_rt_region_set_cap(h, 96);
            let a = __wolf_rt_region_alloc(h, 80); // charges 80
            let b = __wolf_rt_region_alloc(h, 1); // rounds to 16: at cap
            assert!(!a.is_null() && !b.is_null());
            assert_eq!(region_bytes(h), 96);
            __wolf_rt_region_free(h);
        }
        // A pooled header must not leak the cap into the next region.
        let h2 = __wolf_rt_region_new();
        // SAFETY: fresh (possibly pooled) live handle.
        unsafe {
            let big = __wolf_rt_region_alloc(h2, 4096);
            assert!(!big.is_null(), "the reset header is uncapped again");
            __wolf_rt_region_free(h2);
        }
    }

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

    /// s76: the geometric chunk ladder. A small scratch region pays one
    /// small zeroed chunk (the whole reason `b3_churn` is 1.7x faster
    /// after s76 than the leaking version before it); a big region still
    /// reaches its size in O(log n) chunks; an oversized ask gets an
    /// exact chunk of its own.
    #[test]
    fn chunk_ladder_starts_small_and_caps_out() {
        // `chunk_size`'s shift-clamp is derived from `trailing_zeros`,
        // which only means "the ladder's depth" for powers of two.
        const { assert!(CHUNK_MIN.is_power_of_two() && CHUNK_MAX.is_power_of_two()) };
        const { assert!(CHUNK_MIN <= CHUNK_MAX) };
        assert_eq!(chunk_size(0, 1), CHUNK_MIN);
        assert_eq!(chunk_size(1, 1), CHUNK_MIN * 2);
        assert_eq!(chunk_size(4, 1), CHUNK_MIN * 16);
        // Capped, and never below the ask.
        assert_eq!(chunk_size(64, 1), CHUNK_MAX);
        assert_eq!(chunk_size(1000, 1), CHUNK_MAX);
        assert_eq!(chunk_size(0, CHUNK_MAX * 3), CHUNK_MAX * 3);
        assert_eq!(chunk_size(90, CHUNK_MAX * 3), CHUNK_MAX * 3);
    }

    // The `live_region_bytes` ledger is process-wide, and this binary
    // runs its tests on parallel threads — so no assertion on it belongs
    // here. Its litmus is `tests/region_containers.rs`, one test alone
    // in its own process, where `before == after` is exact.

    /// s101: the base guarantee is the TYPE's, not the allocator's.
    /// Under the old `Box<[MaybeUninit<u8>]>` base (formal Layout
    /// alignment 1) the const half of this test did not exist to
    /// fail, and the runtime half held only by the host allocator's
    /// habit; `ChunkBlock`'s `repr(align(16))` is what turns every
    /// 16-grained grant into a 16-ALIGNED pointer by construction.
    #[test]
    fn chunk_bases_are_formally_aligned() {
        const { assert!(core::mem::align_of::<ChunkBlock>() == ALIGN) };
        const { assert!(core::mem::size_of::<ChunkBlock>() == ALIGN) };
        // Live, across every allocation path: fresh ladder rungs, the
        // oversize exact chunk, and pooled reuse.
        for cap in [CHUNK_MIN, CHUNK_MIN * 2, CHUNK_MAX, CHUNK_MAX * 3] {
            let mut c = new_chunk(cap);
            assert_eq!(c.as_mut_ptr().addr() % ALIGN, 0);
            assert_eq!(c.len(), cap);
        }
        POOLS.with(|p| retire_chunk_into(&mut p.borrow_mut().chunks, new_chunk(CHUNK_MIN)));
        let mut pooled = take_chunk(CHUNK_MIN);
        assert_eq!(pooled.as_mut_ptr().addr() % ALIGN, 0);
    }

    /// s125: the trap report's two-line contract. The FIRST line is a
    /// parsed ABI (both harness drivers `strip_prefix("wolf-trap:")`
    /// and take the whole remainder as the kind) — it must stay
    /// byte-identical whether or not a site rides along; the site is
    /// its own additive line.
    #[test]
    fn trap_report_first_line_is_byte_identical_with_and_without_site() {
        let mut bare = Vec::new();
        write_trap_report(&mut bare, trap_code::BOUNDS, None);
        assert_eq!(bare, b"wolf-trap: bounds\n");

        let mut sited = Vec::new();
        write_trap_report(&mut sited, trap_code::BOUNDS, Some(("exmpl.lu", 3, 13)));
        assert_eq!(sited, b"wolf-trap: bounds\n  at exmpl.lu:3:13\n");
        // The machine line is the same bytes in both renderings.
        assert_eq!(
            sited.split(|&b| b == b'\n').next(),
            bare.split(|&b| b == b'\n').next()
        );
        // And the site line never matches the kind parser's prefix.
        let site_line = sited.split(|&b| b == b'\n').nth(1).unwrap();
        assert!(!site_line.starts_with(b"wolf-trap:"));
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
