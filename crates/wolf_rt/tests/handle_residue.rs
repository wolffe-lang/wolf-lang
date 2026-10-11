//! s228 (wolf-lang#661, #662): what the runtime KEEPS per closed file,
//! per closed socket and per finished proc — counted, not sampled.
//!
//! ws57 measured a serving process's remaining growth as three residues
//! in the runtime: 4 bytes per `fs_open` (a slot of the file table), 56
//! per socket (a slot of the socket table) and about 277 per spawned,
//! monitored proc (its monitor channel, leaked into the program as a
//! raw pointer, plus a 16-byte registry tombstone). None of it is in
//! any region, so `live_region_bytes()` reads 0 throughout.
//!
//! This binary installs a global allocator that keeps the number of
//! host bytes outstanding, and reads it around a loop of whole cycles —
//! through the same shims compiled code calls. The count is exact, so
//! the bounds are tight where RSS sampling could only be loose:
//!
//! - files and sockets: nothing per cycle. The bound is a flat
//!   allowance for the tables' own warm storage, which does not scale
//!   with the cycle count; trunk grows by megabytes.
//! - procs: at most [`PROC_KEEP`] bytes each — one reason word per
//!   finished proc, which `[conc.proc.join]`'s late join requires
//!   (`task/proc.rs`'s `Registry`). Trunk keeps about 277.
//!
//! The legs run one at a time (one lock): the counter is the
//! process's.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, Ordering::Relaxed};

struct Outstanding;

static LIVE: AtomicIsize = AtomicIsize::new(0);

unsafe impl GlobalAlloc for Outstanding {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size() as isize, Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size() as isize, Relaxed);
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        LIVE.fetch_add(n as isize - l.size() as isize, Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size() as isize, Relaxed);
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static A: Outstanding = Outstanding;

/// One leg at a time: the counter is process-wide.
static SERIAL: Mutex<()> = Mutex::new(());

fn live() -> isize {
    LIVE.load(Relaxed)
}

fn pair(s: &str) -> (i64, i64) {
    (s.as_ptr() as i64, s.len() as i64)
}

/// The tables' own warm storage, and whatever a libtest thread keeps:
/// flat, whatever the cycle count.
const FLAT_ALLOWANCE: isize = 16 * 1024;

// -------------------------------------------------------------- files --

use wolf_rt::fs::{__wolf_rt_fs_close, __wolf_rt_fs_open, __wolf_rt_fs_read_chunk};

/// wolf-lang#661, the file table: 200,000 `fs_open` + `fs_close` keep
/// nothing. Trunk keeps a slot each (4 bytes; the vector doubles to
/// 1 MiB).
#[test]
fn a_closed_file_leaves_nothing() {
    let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let path = std::env::temp_dir().join(format!("s228-residue-{}.txt", std::process::id()));
    std::fs::write(&path, b"wolf").unwrap();
    let path_s = path.display().to_string();
    let (pp, pl) = pair(&path_s);
    let cycle = || {
        let fd = unsafe { __wolf_rt_fs_open(pp, pl, 0) };
        assert!(fd >= 3, "open: {fd}");
        assert_eq!(__wolf_rt_fs_close(fd), 0);
        fd
    };
    let mut last = 0;
    for _ in 0..1000 {
        last = cycle();
    }
    let before = live();
    const N: i64 = 200_000;
    for _ in 0..N {
        let fd = cycle();
        assert_eq!(fd, last + 1, "a handle number repeated or skipped");
        last = fd;
    }
    let grew = live() - before;
    let _ = std::fs::remove_file(&path);
    assert!(
        grew <= FLAT_ALLOWANCE,
        "{N} open+close cycles left {grew} host bytes outstanding ({:.2} a cycle): \
         the file table keeps something per closed handle (wolf-lang#661)",
        grew as f64 / N as f64
    );
}

/// The use-after-close half of #661 at the shims: a closed handle
/// answers `io` for ever — here across 10,000 opens that reuse its
/// storage — and never reaches the handle that did.
#[test]
fn a_closed_file_handle_stays_closed() {
    let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let dir = std::env::temp_dir();
    let a = dir.join(format!("s228-stale-a-{}.txt", std::process::id()));
    let b = dir.join(format!("s228-stale-b-{}.txt", std::process::id()));
    std::fs::write(&a, b"AAAA").unwrap();
    std::fs::write(&b, b"BBBB").unwrap();
    let (a_s, b_s) = (a.display().to_string(), b.display().to_string());
    let (ap, al) = pair(&a_s);
    let (bp, bl) = pair(&b_s);
    const IO: i64 = wolf_rt::fs::fs_code::IO;
    let stale = unsafe { __wolf_rt_fs_open(ap, al, 0) };
    assert!(stale >= 3);
    assert_eq!(__wolf_rt_fs_close(stale), 0);
    let mut out = 0i64;
    let o = &raw mut out as i64;
    for _ in 0..10_000 {
        let fd = unsafe { __wolf_rt_fs_open(bp, bl, 0) };
        assert!(fd > stale, "the closed handle's number came back: {fd}");
        assert_eq!(
            unsafe { __wolf_rt_fs_read_chunk(stale, 16, o) },
            IO,
            "a read on the closed handle {stale} reached open handle {fd}"
        );
        assert_eq!(
            __wolf_rt_fs_close(stale),
            IO,
            "a close of the closed handle {stale} closed something"
        );
        // The handle that reused the storage is untouched by both.
        assert_eq!(unsafe { __wolf_rt_fs_read_chunk(fd, 16, o) }, 0);
        let h = out as *const i64;
        let (data, len) = unsafe { (h.read() as *const u8, h.add(1).read()) };
        assert_eq!(
            unsafe { std::slice::from_raw_parts(data, len as usize) },
            b"BBBB"
        );
        assert_eq!(__wolf_rt_fs_close(fd), 0);
    }
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
}

// ------------------------------------------------------------ sockets --

use wolf_rt::net::{
    __wolf_rt_net_accept, __wolf_rt_net_close, __wolf_rt_net_read_bytes, __wolf_rt_net_write,
};

/// A listener and how to dial it: unix-domain where the host has it (no
/// ports to exhaust), loopback TCP elsewhere.
#[cfg(unix)]
fn listener() -> (i64, String, usize) {
    let path = std::env::temp_dir().join(format!("s228-residue-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let path_s = path.display().to_string();
    let (pp, pl) = pair(&path_s);
    let l = unsafe { wolf_rt::net::__wolf_rt_net_listen_unix(pp, pl) };
    assert!(l >= 0, "listen_unix: {l}");
    (l, path_s, 50_000)
}

#[cfg(not(unix))]
fn listener() -> (i64, String, usize) {
    let (ap, al) = pair("127.0.0.1:0");
    let l = unsafe { wolf_rt::net::__wolf_rt_net_listen(ap, al) };
    assert!(l >= 0, "listen: {l}");
    let port = wolf_rt::net::__wolf_rt_net_port(l);
    // Loopback TCP holds each closed pair's port for the host's wait:
    // few enough cycles to stay inside the ephemeral range.
    (l, format!("127.0.0.1:{port}"), 3_000)
}

fn dial(addr: &str) -> i64 {
    let (ap, al) = pair(addr);
    #[cfg(unix)]
    let c = unsafe { wolf_rt::net::__wolf_rt_net_connect_unix(ap, al) };
    #[cfg(not(unix))]
    let c = unsafe { wolf_rt::net::__wolf_rt_net_connect(ap, al) };
    assert!(c >= 0, "connect: {c}");
    c
}

/// wolf-lang#661, the socket table: a connection dialed, accepted and
/// closed at both ends keeps nothing. Trunk keeps two 56-byte slots a
/// cycle.
#[test]
fn a_closed_connection_leaves_nothing() {
    let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let (l, addr, n) = listener();
    let cycle = || {
        let c = dial(&addr);
        let s = __wolf_rt_net_accept(l);
        assert!(s >= 0, "accept: {s}");
        assert_eq!(__wolf_rt_net_close(s), 0);
        assert_eq!(__wolf_rt_net_close(c), 0);
        s
    };
    let mut last = 0;
    for _ in 0..200 {
        last = cycle();
    }
    let before = live();
    for _ in 0..n {
        let s = cycle();
        assert_eq!(s, last + 2, "a handle number repeated or skipped");
        last = s;
    }
    let grew = live() - before;
    assert_eq!(__wolf_rt_net_close(l), 0);
    assert!(
        grew <= FLAT_ALLOWANCE,
        "{n} connect+accept+close cycles left {grew} host bytes outstanding \
         ({:.1} a connection): the socket table keeps something per closed socket \
         (wolf-lang#661)",
        grew as f64 / n as f64
    );
}

/// Use after close at the net shims: a closed stream's handle answers
/// `io` across the connections that reuse its storage, and the bytes of
/// a later connection never arrive on it.
#[test]
fn a_closed_socket_handle_stays_closed() {
    let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    let (l, addr, _) = listener();
    const IO: i64 = wolf_rt::net::net_code::IO;
    let c0 = dial(&addr);
    let stale = __wolf_rt_net_accept(l);
    assert!(stale >= 0);
    assert_eq!(__wolf_rt_net_close(stale), 0);
    assert_eq!(__wolf_rt_net_close(c0), 0);
    let mut out = 0i64;
    let o = &raw mut out as i64;
    for i in 0..2_000 {
        let c = dial(&addr);
        let s = __wolf_rt_net_accept(l);
        assert!(s > stale, "the closed handle's number came back: {s}");
        let (mp, ml) = pair("ping");
        assert_eq!(unsafe { __wolf_rt_net_write(c, mp, ml) }, 0);
        assert_eq!(
            unsafe { __wolf_rt_net_read_bytes(stale, 16, o) },
            IO,
            "round {i}: a read on closed handle {stale} reached connection {s}"
        );
        assert_eq!(unsafe { __wolf_rt_net_write(stale, mp, ml) }, IO);
        assert_eq!(__wolf_rt_net_close(stale), IO);
        // The live connection still has its four bytes.
        assert_eq!(unsafe { __wolf_rt_net_read_bytes(s, 16, o) }, 0);
        let h = out as *const i64;
        assert_eq!(
            unsafe { h.add(1).read() },
            4,
            "round {i}: bytes went missing"
        );
        assert_eq!(__wolf_rt_net_close(s), 0);
        assert_eq!(__wolf_rt_net_close(c), 0);
    }
    assert_eq!(__wolf_rt_net_close(l), 0);
}

// -------------------------------------------------------------- procs --

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
mod procs {
    use super::{SERIAL, live};
    use core::ffi::c_void;
    use wolf_rt::task::{
        __wolf_rt_proc_join, __wolf_rt_proc_monitor, __wolf_rt_proc_spawn_outcome,
        __wolf_rt_select, ProcExit, WolfSelectArm,
    };

    /// What one finished proc may keep: its reason word, 8 bytes, in a
    /// vector that doubles — 50,000 more procs grow it by about
    /// 510 KiB, 10.2 bytes each. Trunk's 16-byte tombstones grow theirs
    /// by 1 MiB over the same loop, 20.3 each.
    const PROC_KEEP: isize = 12;
    /// Pool workers come and go while the loop runs, each with a
    /// thread record and thread-locals: flat, not per proc.
    const POOL_ALLOWANCE: isize = 128 * 1024;

    unsafe extern "C" fn body(_env: *mut c_void) -> i64 {
        0
    }

    fn spawn() -> u64 {
        // SAFETY: a null env of length 0 and a null name are the
        // shim's documented empty forms.
        unsafe { __wolf_rt_proc_spawn_outcome(body, std::ptr::null(), 0, std::ptr::null(), 0) }
    }

    /// `select { exit(r) from m => … , timeout(30.s) => … }`, as
    /// lowering spells it: the committed arm and the word.
    fn await_exit(m: *mut c_void) -> (i64, u64) {
        let arm = WolfSelectArm {
            chan: m,
            dir: 0,
            _pad: 0,
            value: 0,
        };
        let mut word = 0u64;
        let mut status = 0i32;
        // SAFETY: one live descriptor, two writable out slots.
        let got =
            unsafe { __wolf_rt_select(&arm, 1, 30_000_000_000, 0, &raw mut word, &raw mut status) };
        (got, word)
    }

    /// wolf-lang#662: spawn, monitor, receive the exit — ws57's p12 at
    /// the shims. Trunk keeps about 277 bytes a proc (the monitor's
    /// channel is never freed).
    #[test]
    fn a_monitored_proc_leaves_one_word() {
        let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        let cycle = || {
            let id = spawn();
            let m = __wolf_rt_proc_monitor(id);
            let (arm, word) = await_exit(m);
            assert_eq!(arm, 0, "the exit never arrived (verdict {arm})");
            assert_eq!(ProcExit::decode(word), ProcExit::Normal { value: 0 });
        };
        for _ in 0..2_000 {
            cycle();
        }
        let before = live();
        const N: isize = 50_000;
        for _ in 0..N {
            cycle();
        }
        let grew = live() - before;
        assert!(
            grew <= PROC_KEEP * N + POOL_ALLOWANCE,
            "{N} spawned and monitored procs left {grew} host bytes outstanding \
             ({:.1} a proc, {PROC_KEEP} allowed): a finished proc keeps more than its \
             reason word (wolf-lang#662)",
            grew as f64 / N as f64
        );
    }

    /// The same for a proc that is only joined (16 bytes a proc on
    /// trunk: the tombstone), and the late readers still answer: a
    /// join and a monitor long after the exit read the same reason.
    #[test]
    fn a_joined_proc_leaves_one_word_and_still_answers_late() {
        let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        let first = spawn();
        assert_eq!(
            ProcExit::decode(__wolf_rt_proc_join(first)),
            ProcExit::Normal { value: 0 }
        );
        for _ in 0..2_000 {
            __wolf_rt_proc_join(spawn());
        }
        let before = live();
        const N: isize = 50_000;
        for _ in 0..N {
            let word = __wolf_rt_proc_join(spawn());
            assert_eq!(ProcExit::decode(word), ProcExit::Normal { value: 0 });
        }
        let grew = live() - before;
        // 52,000 procs later the first one's id still answers, twice
        // over and by both readers ([conc.proc.join]).
        for _ in 0..2 {
            assert_eq!(
                ProcExit::decode(__wolf_rt_proc_join(first)),
                ProcExit::Normal { value: 0 }
            );
            let (arm, word) = await_exit(__wolf_rt_proc_monitor(first));
            assert_eq!(arm, 0);
            assert_eq!(ProcExit::decode(word), ProcExit::Normal { value: 0 });
        }
        assert!(
            grew <= PROC_KEEP * N + POOL_ALLOWANCE,
            "{N} spawned and joined procs left {grew} host bytes outstanding \
             ({:.1} a proc, {PROC_KEEP} allowed): the registry keeps more than a reason \
             word per finished proc (wolf-lang#662)",
            grew as f64 / N as f64
        );
    }

    /// A monitor is still a channel after its delivery has been
    /// received: a second wait on it times out (nothing else is coming)
    /// and does not fault, and a copy of the handle is the same
    /// monitor.
    #[test]
    fn a_spent_monitor_is_an_empty_channel() {
        let _g = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        let id = spawn();
        let m = __wolf_rt_proc_monitor(id);
        let (arm, _) = await_exit(m);
        assert_eq!(arm, 0);
        let again = WolfSelectArm {
            chan: m,
            dir: 0,
            _pad: 0,
            value: 0,
        };
        let mut word = 0u64;
        let mut status = 0i32;
        // SAFETY: as in `await_exit`, with a 20 ms timeout arm.
        let got =
            unsafe { __wolf_rt_select(&again, 1, 20_000_000, 0, &raw mut word, &raw mut status) };
        assert_eq!(
            got,
            wolf_rt::task::select_verdict::TIMEOUT,
            "a monitor delivered twice, or a spent one answered something"
        );
        // Two monitors of one proc are two channels: each gets the
        // reason once.
        let (m1, m2) = (__wolf_rt_proc_monitor(id), __wolf_rt_proc_monitor(id));
        assert_ne!(m1, m2);
        assert_eq!(await_exit(m2).0, 0);
        assert_eq!(await_exit(m1).0, 0);
    }
}
