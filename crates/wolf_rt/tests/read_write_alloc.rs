//! s222 (wolf-lang#635): what one read, one gathered write and one
//! interpolation ask of the HOST allocator once warm — counted, not
//! profiled.
//!
//! ws54's profile of a lobo hand named three per-request costs beside
//! the root arena's lock: a read buffer allocated ZEROED on every read
//! (`vec![0u8; max]` in the net and fs read families), two
//! malloc/free pairs in every `net_writev_head` (the part list and the
//! `IoSlice` run), and an interpolation buffer that is `Box`ed,
//! grown by `realloc` and freed on every `strbuf_new`/`finish`. This
//! binary installs a counting global allocator and arms it on the
//! calling thread around exactly the call under test, so the counts
//! are that call's and no other test's (libtest runs tests on threads
//! of their own; the counters are thread-local).
//!
//! The bound separates a per-call buffer from the root arena's own
//! refill (one 64 KiB chunk per 64 KiB of materialized results, and
//! the chunk list's amortized growth — the root region doing its job,
//! never freed): no zeroed allocation at all, at most one free per
//! `REFILL_SLACK` calls, and at most one allocation per ten. Trunk
//! pays an allocation and a free (or more) on every call (red); the
//! fix frees nothing once warm. (The first bound, one host call of any
//! kind per 50 calls, was too tight for a 1 KiB result: `fs_read_at`
//! read 21 — 17 chunk refills and 4 chunk-list reallocs — in 2 of 30
//! release runs under `taskset -c 0-3` on kasumi.)

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Counting;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
    static ZEROED: Cell<usize> = const { Cell::new(0) };
    static REALLOCS: Cell<usize> = const { Cell::new(0) };
    static FREES: Cell<usize> = const { Cell::new(0) };
}

fn bump(c: &'static std::thread::LocalKey<Cell<usize>>) {
    if ARMED.try_with(Cell::get).unwrap_or(false) {
        let _ = c.try_with(|c| c.set(c.get() + 1));
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        bump(&ALLOCS);
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        bump(&ZEROED);
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        bump(&REALLOCS);
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        bump(&FREES);
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static A: Counting = Counting;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Counts {
    allocs: usize,
    zeroed: usize,
    reallocs: usize,
    frees: usize,
}

fn reset() {
    for c in [&ALLOCS, &ZEROED, &REALLOCS, &FREES] {
        c.with(|c| c.set(0));
    }
}

fn read() -> Counts {
    Counts {
        allocs: ALLOCS.with(Cell::get),
        zeroed: ZEROED.with(Cell::get),
        reallocs: REALLOCS.with(Cell::get),
        frees: FREES.with(Cell::get),
    }
}

/// Run `f` armed and return what it asked of the host allocator.
fn armed<R>(f: impl FnOnce() -> R) -> (R, Counts) {
    reset();
    ARMED.with(|a| a.set(true));
    let r = f();
    ARMED.with(|a| a.set(false));
    (r, read())
}

const CALLS: usize = 1000;
const WARM: usize = 16;
/// At most one host allocation per this many calls: the root arena's
/// chunk refills, never a per-call buffer.
const REFILL_SLACK: usize = 50;

fn assert_steady(what: &str, total: Counts) {
    assert_eq!(
        total.zeroed, 0,
        "{what}: {} zeroed allocations over {CALLS} calls ({total:?}) — a buffer zeroed for a read to overwrite",
        total.zeroed
    );
    // A per-call buffer is allocated AND freed every call; the root
    // arena's refills are allocations (a 64 KiB chunk per 64 KiB of
    // results, plus the chunk list's own amortized growth) and are
    // never freed. So frees are the per-call signal, and allocations
    // get a bound that only a per-call buffer can break.
    assert!(
        total.frees * REFILL_SLACK <= CALLS,
        "{what}: {} host frees over {CALLS} calls ({total:?}) — a per-call buffer",
        total.frees
    );
    assert!(
        (total.allocs + total.reallocs) * 10 <= CALLS,
        "{what}: {} host allocations over {CALLS} calls ({total:?}) — a per-call buffer",
        total.allocs + total.reallocs
    );
}

fn add(a: Counts, b: Counts) -> Counts {
    Counts {
        allocs: a.allocs + b.allocs,
        zeroed: a.zeroed + b.zeroed,
        reallocs: a.reallocs + b.reallocs,
        frees: a.frees + b.frees,
    }
}

// ------------------------------------------------------------ helpers --

use wolf_rt::fs::{
    __wolf_rt_fs_close, __wolf_rt_fs_open, __wolf_rt_fs_read_at, __wolf_rt_fs_read_chunk,
    __wolf_rt_fs_seek,
};
use wolf_rt::list::{__wolf_rt_list_new, __wolf_rt_list_push};
use wolf_rt::net::{
    __wolf_rt_net_accept, __wolf_rt_net_close, __wolf_rt_net_connect, __wolf_rt_net_listen,
    __wolf_rt_net_port, __wolf_rt_net_read, __wolf_rt_net_read_bytes, __wolf_rt_net_write,
    __wolf_rt_net_writev_head,
};
use wolf_rt::str::{__wolf_rt_strbuf_finish, __wolf_rt_strbuf_new, __wolf_rt_strbuf_str};

fn pair(s: &str) -> (i64, i64) {
    (s.as_ptr() as i64, s.len() as i64)
}

/// A connected loopback pair of runtime stream handles.
fn loopback() -> (i64, i64, i64) {
    let (ap, al) = pair("127.0.0.1:0");
    let srv = unsafe { __wolf_rt_net_listen(ap, al) };
    assert!(srv >= 0, "listen: {srv}");
    let port = __wolf_rt_net_port(srv);
    let addr = format!("127.0.0.1:{port}");
    let (cp, cl) = pair(&addr);
    let cli = unsafe { __wolf_rt_net_connect(cp, cl) };
    assert!(cli >= 0, "connect: {cli}");
    let conn = __wolf_rt_net_accept(srv);
    assert!(conn >= 0, "accept: {conn}");
    (srv, cli, conn)
}

fn list_header(hdr: i64) -> [i64; 2] {
    // The list's {data, len} — enough for the bytes a read returned.
    let h = hdr as *const i64;
    unsafe { [h.read(), h.add(1).read()] }
}

fn byte_list(bytes: &[u8]) -> i64 {
    let hdr = __wolf_rt_list_new(1);
    for b in bytes {
        unsafe { __wolf_rt_list_push(hdr, b as *const u8 as i64) };
    }
    hdr
}

// -------------------------------------------------------------- reads --

/// `net_read_bytes` and `net_read` at lobo's shape (a large `max`, a
/// small arrival): the buffer the kernel fills must not be a fresh
/// zeroed allocation per call.
#[test]
fn a_net_read_allocates_no_buffer() {
    let (srv, cli, conn) = loopback();
    let (xp, xl) = pair("x");
    let mut total_bytes = Counts::default();
    let mut total_str = Counts::default();
    for i in 0..WARM + CALLS {
        assert_eq!(unsafe { __wolf_rt_net_write(cli, xp, xl) }, 0);
        let mut out = [0i64; 2];
        let (rc, c) =
            armed(|| unsafe { __wolf_rt_net_read_bytes(conn, 65536, out.as_mut_ptr() as i64) });
        assert_eq!(rc, 0, "read_bytes");
        assert_eq!(list_header(out[0])[1], 1, "one byte back");
        if i >= WARM {
            total_bytes = add(total_bytes, c);
        }
        assert_eq!(unsafe { __wolf_rt_net_write(cli, xp, xl) }, 0);
        let (rc, c) = armed(|| unsafe { __wolf_rt_net_read(conn, 65536, out.as_mut_ptr() as i64) });
        assert_eq!(rc, 0, "read");
        assert_eq!(out[1], 1, "one byte back");
        if i >= WARM {
            total_str = add(total_str, c);
        }
    }
    for fd in [cli, conn, srv] {
        __wolf_rt_net_close(fd);
    }
    assert_steady("net_read_bytes(fd, 65536)", total_bytes);
    assert_steady("net_read(fd, 65536)", total_str);
}

/// `fs_read_chunk` (lobo's body read) and `fs_read_at`, the same rule.
#[test]
fn a_file_read_allocates_no_buffer() {
    let dir = std::env::temp_dir().join(format!("s222-read-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("body.html");
    std::fs::write(&path, [b'w'; 1024]).unwrap();
    let p = path.to_str().unwrap().to_string();
    let (pp, pl) = pair(&p);
    let fd = unsafe { __wolf_rt_fs_open(pp, pl, 0) };
    assert!(fd >= 0, "open: {fd}");
    let mut total_chunk = Counts::default();
    let mut total_at = Counts::default();
    for i in 0..WARM + CALLS {
        let mut pos = 0i64;
        assert_eq!(
            unsafe { __wolf_rt_fs_seek(fd, 0, 0, &mut pos as *mut i64 as i64) },
            0
        );
        let mut out = 0i64;
        let (rc, c) =
            armed(|| unsafe { __wolf_rt_fs_read_chunk(fd, 65536, &mut out as *mut i64 as i64) });
        assert_eq!(rc, 0, "read_chunk");
        assert_eq!(list_header(out)[1], 1024);
        if i >= WARM {
            total_chunk = add(total_chunk, c);
        }
        let (rc, c) =
            armed(|| unsafe { __wolf_rt_fs_read_at(fd, 0, 65536, &mut out as *mut i64 as i64) });
        assert_eq!(rc, 0, "read_at");
        assert_eq!(list_header(out)[1], 1024);
        if i >= WARM {
            total_at = add(total_at, c);
        }
    }
    __wolf_rt_fs_close(fd);
    let _ = std::fs::remove_dir_all(&dir);
    assert_steady("fs_read_chunk(fd, 65536)", total_chunk);
    assert_steady("fs_read_at(fd, 0, 65536)", total_at);
}

// -------------------------------------------------------------- writes --

/// `net_writev_head` with lobo's response shape: a `str` head and a
/// one-part body. The gather is a few words a part; it is not worth a
/// heap allocation and a free per response.
#[test]
fn a_gathered_write_allocates_nothing() {
    let (srv, cli, conn) = loopback();
    let head = "HTTP/1.1 200 OK\r\nServer: lobo\r\nContent-Type: text/html\r\nContent-Length: 1024\r\n\r\n";
    let (hp, hl) = pair(head);
    let body = byte_list(&[b'b'; 1024]);
    let parts = __wolf_rt_list_new(8);
    unsafe { __wolf_rt_list_push(parts, &body as *const i64 as i64) };
    let want = head.len() + 1024;
    let mut total = Counts::default();
    for i in 0..WARM + CALLS {
        let (rc, c) = armed(|| unsafe { __wolf_rt_net_writev_head(conn, hp, hl, parts) });
        assert_eq!(rc, 0, "writev_head");
        if i >= WARM {
            total = add(total, c);
        }
        let mut got = 0usize;
        while got < want {
            let mut out = 0i64;
            assert_eq!(
                unsafe { __wolf_rt_net_read_bytes(cli, 65536, &mut out as *mut i64 as i64) },
                0
            );
            got += list_header(out)[1] as usize;
        }
    }
    for fd in [cli, conn, srv] {
        __wolf_rt_net_close(fd);
    }
    assert_steady("net_writev_head(head, [body])", total);
}

/// An interpolation the size of a response head, built from eight
/// segments: `strbuf_new`, the appends, `finish`.
#[test]
fn an_interpolation_allocates_nothing_once_warm() {
    let segs = [
        "HTTP/1.1 ",
        "200 OK",
        "\r\nServer: ",
        "lobo",
        "\r\nContent-Type: ",
        "text/html",
        "\r\nContent-Length: 1024\r\nLast-Modified: Thu, 08 Oct 2026 12:00:00 GMT\r\n",
        "\r\n",
    ];
    let want: String = segs.concat();
    let mut total = Counts::default();
    for i in 0..WARM + CALLS {
        let mut out = [0i64; 2];
        let ((), c) = armed(|| unsafe {
            let h = __wolf_rt_strbuf_new();
            for s in segs {
                let (p, l) = pair(s);
                __wolf_rt_strbuf_str(h, p, l, 0);
            }
            __wolf_rt_strbuf_finish(h, out.as_mut_ptr() as i64);
        });
        let got = unsafe { std::slice::from_raw_parts(out[0] as *const u8, out[1] as usize) };
        assert_eq!(got, want.as_bytes());
        if i >= WARM {
            total = add(total, c);
        }
    }
    assert_steady("strbuf (8 segments, a head)", total);
}
