//! s290, the one wait (`[os.ready.wait]`, `[os.ready.set]`,
//! `[os.ready.facts]`; wolf-std sc82's M1): one wait over files (the fs
//! table: pty masters, pipes, terminals, regular files), sockets (the net
//! table: listeners and streams) and signal meanings, answering EVERY ready
//! entry with its facts, and a meaning beside them.
//!
//! Two forms, one answer:
//!
//! - [`wait_ready`], the one-shot (`os_wait_ready`): a list of elements each
//!   wait — a handle, bit 61 for a net handle, bit 62 for write interest —
//!   answered `[index, facts, …]` in list order. `poll(2)` over a duplicate
//!   of each element's descriptor: O(N) a wait.
//! - the kernel-held set ([`set_open`], [`set_put`], [`set_wait`],
//!   [`set_close`]; `os_ready_*`): entries added once, keyed by the caller,
//!   answered `[key, facts, …]` in the kernel's order. `epoll` on linux (and
//!   PAX, which runs the linux binary), `kqueue` on macOS: O(ready) a wait.
//!
//! Served on linux and macOS, the hosts whose runtime has the signal hub
//! and the task layer; windows and the tier-2 hosts answer `unsupported`
//! by name (`os_ready_close` answers `io`: no set was ever made there).
//!   A descriptor the kernel set will not hold — every regular file and
//!   directory, decided by `fstat` at the add, and whatever `epoll_ctl`
//!   refuses EPERM or kqueue EINVAL — is a POLLED entry, `poll(2)`ed beside
//!   the set every wait.
//!
//! The meaning half is `crate::signal`'s ([`crate::signal::wait_looking`]):
//! the hub's wake pipe, registered with the hub before the queue's last
//! look, so a meaning that arrives after it ends the park; in the set form
//! the pipe is registered in the kernel set itself while waits ask for
//! meanings, so a wait with no polled entry is one `epoll_wait`/`kevent`.
//!
//! **A handle's close removes its entry from every set** ([`forget`], from
//! `fs_close` and `net_close`, before the descriptor closes): epoll alone
//! keeps a registration while any descriptor of the open file lives.
//! **A set is its process's**: made before `os_fork`, the child's set and
//! wait answer `io`, its closes leave the set alone, and its close frees
//! only its copy (on macOS the child was handed no kqueue at all, so the
//! number is not closed there).
//!
//! Lock order: a table's lock (`FILES`, `NET`) before [`SETS`], never the
//! reverse; a wait holds no lock while it parks.

#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::fd::{AsRawFd as _, OwnedFd, RawFd};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::os::proc_code;

/// Bit 61 of an element or a handle: a NET handle (a listener or a stream)
/// in the other bits, 2305843009213693952.
pub const NET: i64 = 1 << 61;
/// Bit 62 of a one-shot element: write interest (`[os.signal.wait.write]`'s
/// mark, kept).
pub const WRITE: i64 = 1 << 62;

/// The facts of a ready entry (`[os.ready.facts]`).
pub mod facts {
    pub const READ: i64 = 1;
    pub const WRITE: i64 = 2;
    pub const HANGUP: i64 = 4;
    pub const ERROR: i64 = 8;
}

/// Which of the runtime's tables a handle is a number of.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Table {
    Fs,
    Net,
}

/// An element as (table, handle, write interest); `None` for a negative
/// number, which names no handle (`io`).
pub fn decode(e: i64) -> Option<(Table, i64, bool)> {
    if e < 0 {
        return None;
    }
    let table = if e & NET != 0 { Table::Net } else { Table::Fs };
    Some((table, e & !(NET | WRITE), e & WRITE != 0))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
/// What one look saw of one entry, normalized across the hosts' waits:
/// `poll(2)`'s revents, an epoll event, a kqueue event.
#[derive(Clone, Copy, Default, Debug)]
struct Seen {
    /// POLLIN, EPOLLIN, kqueue's read filter.
    read: bool,
    /// POLLOUT, EPOLLOUT, kqueue's write filter.
    write: bool,
    /// The reading side's end: POLLRDHUP, EPOLLRDHUP, EV_EOF on the read
    /// filter.
    rdhup: bool,
    /// The writing side's end: EV_EOF on the write filter.
    wrhup: bool,
    /// Both sides: POLLHUP, EPOLLHUP.
    hup: bool,
    /// POLLERR, EPOLLERR, EV_ERROR, an EV_EOF carrying a socket's error.
    err: bool,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
/// `[os.ready.facts]`: the facts `s` gives an entry that asked `interest`
/// (1 read, 2 write, 3 both). A hangup or an error always comes with an
/// interest the entry asked, so a loop that tests read alone reads and
/// meets the end.
fn facts_of(s: Seen, interest: i64) -> i64 {
    let asked_r = interest & facts::READ != 0;
    let asked_w = interest & facts::WRITE != 0;
    let mut f = 0;
    if asked_r && s.read {
        f |= facts::READ;
    }
    if asked_w && s.write {
        f |= facts::WRITE;
    }
    if s.rdhup {
        f |= facts::HANGUP | (interest & facts::READ);
    }
    if s.wrhup {
        f |= facts::HANGUP | (interest & facts::WRITE);
    }
    if s.hup {
        f |= facts::HANGUP | interest;
    }
    if s.err {
        f |= facts::ERROR | interest;
    }
    f
}

// ------------------------------------------------------------ poll --

/// The `poll(2)` events an entry with `interest` asks: POLLIN (and, on
/// linux and so PAX, POLLRDHUP, so a peer's end reads as a hangup) for
/// read, POLLOUT for write. POLLHUP and POLLERR are always answered.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn poll_events(interest: i64) -> libc::c_short {
    let mut ev = 0;
    if interest & facts::READ != 0 {
        ev |= libc::POLLIN;
        #[cfg(target_os = "linux")]
        {
            ev |= libc::POLLRDHUP;
        }
    }
    if interest & facts::WRITE != 0 {
        ev |= libc::POLLOUT;
    }
    ev
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn seen_of_poll(re: libc::c_short) -> Seen {
    #[cfg(target_os = "linux")]
    let rdhup = re & libc::POLLRDHUP != 0;
    #[cfg(not(target_os = "linux"))]
    let rdhup = false;
    Seen {
        read: re & libc::POLLIN != 0,
        write: re & libc::POLLOUT != 0,
        rdhup,
        wrhup: false,
        hup: re & libc::POLLHUP != 0,
        err: re & libc::POLLERR != 0,
    }
}

/// One `poll(2)` over `fds` (each with its events) for at most
/// `timeout_ms` (-1: no limit), answering each one's revents in order.
/// Where the host's poll refuses a descriptor (POLLNVAL: macOS's
/// terminals by path and `/dev/null`, measured), `select(2)` on all of
/// them, under `FD_SETSIZE`, whose answer is POLLIN and POLLOUT alone: the
/// descriptor waits' rule (`[os.signal.wait.fd]`). `Err` is the errno.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn poll_many(fds: &[(RawFd, libc::c_short)], timeout_ms: i32) -> Result<Vec<libc::c_short>, i32> {
    let errno = || {
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO)
    };
    let mut pfds: Vec<libc::pollfd> = fds
        .iter()
        .map(|&(fd, events)| libc::pollfd {
            fd,
            events,
            revents: 0,
        })
        .collect();
    // SAFETY: `pfds.len()` live pollfds.
    if unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, timeout_ms) } < 0 {
        return Err(errno());
    }
    if pfds.iter().all(|p| p.revents & libc::POLLNVAL == 0) {
        return Ok(pfds.iter().map(|p| p.revents).collect());
    }
    if fds.iter().any(|&(fd, _)| i64::from(fd) >= libc::FD_SETSIZE as i64) {
        return Err(libc::EBADF);
    }
    // SAFETY: zeroed fd_sets filled by FD_ZERO/FD_SET with descriptors
    // under FD_SETSIZE; a timeval or none.
    unsafe {
        let mut rset: libc::fd_set = std::mem::zeroed();
        let mut wset: libc::fd_set = std::mem::zeroed();
        libc::FD_ZERO(&mut rset);
        libc::FD_ZERO(&mut wset);
        let mut top = 0;
        for &(fd, ev) in fds {
            if ev & libc::POLLIN != 0 {
                libc::FD_SET(fd, &mut rset);
            }
            if ev & libc::POLLOUT != 0 {
                libc::FD_SET(fd, &mut wset);
            }
            top = top.max(fd);
        }
        // `suseconds_t` is i64 on linux and i32 on macOS.
        #[allow(clippy::useless_conversion)]
        let mut tv = libc::timeval {
            tv_sec: (timeout_ms.max(0) / 1000).into(),
            tv_usec: ((timeout_ms.max(0) % 1000) * 1000).into(),
        };
        let tvp = if timeout_ms < 0 {
            std::ptr::null_mut()
        } else {
            &raw mut tv
        };
        if libc::select(top + 1, &mut rset, &mut wset, std::ptr::null_mut(), tvp) < 0 {
            return Err(errno());
        }
        Ok(fds
            .iter()
            .map(|&(fd, ev)| {
                let mut r = 0;
                if ev & libc::POLLIN != 0 && libc::FD_ISSET(fd, &rset) {
                    r |= libc::POLLIN;
                }
                if ev & libc::POLLOUT != 0 && libc::FD_ISSET(fd, &wset) {
                    r |= libc::POLLOUT;
                }
                r
            })
            .collect())
    }
}

/// A duplicate of the descriptor behind a live handle (close-on-exec),
/// made under its table's lock, so a close by another task cannot let its
/// number be reused while the wait holds it. `None`: a closed or forged
/// handle.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn dup_of(table: Table, h: i64) -> Option<OwnedFd> {
    match table {
        Table::Fs => crate::fs::dup_of(h).map(OwnedFd::from),
        Table::Net => crate::net::dup_of(h),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
/// A look's failure as the call's row: `io`, with the host's number in
/// `os_error()` when it gave one.
fn io_of(errno: i32) -> i64 {
    if errno != 0 {
        crate::fs::os_error_set(i64::from(errno));
    }
    proc_code::IO
}

/// The shape every wait checks before the host: a non-empty `set` held to
/// `[os.signal.set]`'s rule (`io`, `os_signal_wait_fds`'s spelling), then a
/// deadline below -1 (`invalid`).
fn shape(set: i64, deadline_ms: i64) -> Result<(), i64> {
    // `[os.signal.set]`'s one shape rule (`crate::signal::listenable_set`,
    // which a host without the signal hub does not build).
    const LISTENABLE: i64 = (1 << 20) - 1;
    if set != 0 && set & !LISTENABLE != 0 {
        return Err(proc_code::IO);
    }
    if deadline_ms < -1 {
        return Err(proc_code::INVALID);
    }
    Ok(())
}

/// `os_wait_ready(entries, set, deadline_ms)` (`[os.ready.wait]`): park
/// until at least one element is ready, a meaning in `set` is delivered,
/// or the deadline passes; answer `([index, facts, …], meaning)` — every
/// element ready at that moment, in list order, and the meaning taken (0
/// when none). Codes are `os::proc_code`'s.
pub fn wait_ready(entries: &[i64], set: i64, deadline_ms: i64) -> Result<(Vec<i64>, i64), i64> {
    shape(set, deadline_ms)?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        if entries.is_empty() && set == 0 && deadline_ms == -1 {
            return Err(proc_code::IO);
        }
        let mut held = Vec::with_capacity(entries.len());
        let mut interest = Vec::with_capacity(entries.len());
        for &e in entries {
            let Some((table, h, write)) = decode(e) else {
                return Err(proc_code::IO);
            };
            let Some(fd) = dup_of(table, h) else {
                return Err(proc_code::IO);
            };
            held.push(fd);
            interest.push(if write { facts::WRITE } else { facts::READ });
        }
        let fds: Vec<(RawFd, libc::c_short)> = held
            .iter()
            .zip(&interest)
            .map(|(f, &i)| (f.as_raw_fd(), poll_events(i)))
            .collect();
        let n = fds.len();
        let look = |wake: Option<RawFd>, timeout: i32| -> Result<Option<Vec<i64>>, i32> {
            let mut all = fds.clone();
            if let Some(w) = wake {
                all.push((w, libc::POLLIN));
            }
            let re = poll_many(&all, timeout)?;
            let mut out = Vec::new();
            for (i, (&r, &want)) in re[..n].iter().zip(&interest).enumerate() {
                let f = facts_of(seen_of_poll(r), want);
                if f != 0 {
                    out.push(i as i64);
                    out.push(f);
                }
            }
            Ok((!out.is_empty()).then_some(out))
        };
        let (ready, m) = crate::signal::wait_looking(set, deadline_ms, look).map_err(io_of)?;
        drop(held);
        Ok((ready.unwrap_or_default(), m))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = entries;
        Err(proc_code::UNSUPPORTED)
    }
}

// ------------------------------------------------ the kernel's set --

/// The token a wake-pipe registration carries in a kernel set: no slot's.
#[cfg(any(target_os = "linux", target_os = "macos"))]
const WAKE_TOKEN: u64 = u64::MAX;

/// One entry of a set.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[derive(Debug)]
struct Slot {
    raw: RawFd,
    interest: i64,
    key: i64,
    /// The slot's generation: an event harvested for a slot since freed
    /// and reused is dropped, never answered under the new entry's key.
    generation: u32,
    /// Held beside the kernel set and `poll(2)`ed each wait.
    polled: bool,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[derive(Debug)]
struct Set {
    handle: i64,
    /// The epoll or kqueue descriptor; a wait holds a clone while it parks.
    kfd: Arc<OwnedFd>,
    /// The process that made it (`[os.proc.fork]`'s rule).
    pid: u32,
    slots: Vec<Option<Slot>>,
    free: Vec<usize>,
    by_handle: std::collections::HashMap<(Table, i64), usize>,
    generation: u32,
    /// The wake pipe's read end while it is registered in the kernel set.
    wake: Option<RawFd>,
    /// Entries the kernel set holds (the harvest's room).
    in_kernel: usize,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Set {
    fn token(idx: usize, generation: u32) -> u64 {
        ((idx as u64) << 32) | u64::from(generation)
    }

    fn slot_of(&self, token: u64) -> Option<(usize, &Slot)> {
        let idx = usize::try_from(token >> 32).ok()?;
        let s = self.slots.get(idx)?.as_ref()?;
        (u64::from(s.generation) == token & 0xffff_ffff).then_some((idx, s))
    }

    /// Take the entry for `(table, h)` out of the set, the kernel's
    /// registration first; `false` when the set holds none.
    fn drop_entry(&mut self, key: (Table, i64)) -> bool {
        let Some(idx) = self.by_handle.remove(&key) else {
            return false;
        };
        if let Some(s) = self.slots[idx].take() {
            if !s.polled {
                let _ = kernel::del(&self.kfd, s.raw, s.interest);
                self.in_kernel = self.in_kernel.saturating_sub(1);
            }
        }
        self.free.push(idx);
        true
    }
}

/// Every set of this process; [`OPEN`] counts them so a close in a program
/// with none costs one atomic load.
#[cfg(any(target_os = "linux", target_os = "macos"))]
struct Registry {
    sets: Vec<Set>,
    next: i64,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
static SETS: Mutex<Registry> = Mutex::new(Registry {
    sets: Vec::new(),
    next: 0,
});

static OPEN: AtomicUsize = AtomicUsize::new(0);

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn sets() -> std::sync::MutexGuard<'static, Registry> {
    SETS.lock().unwrap_or_else(|p| p.into_inner())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
impl Registry {
    fn get_mut(&mut self, r: i64) -> Option<&mut Set> {
        self.sets.iter_mut().find(|s| s.handle == r)
    }
}

/// Whether a descriptor is a regular file or a directory: a polled entry on
/// every host (poll answers it always ready; epoll refuses it; kqueue's
/// read filter answers only before its end).
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn always_ready(raw: RawFd) -> bool {
    // SAFETY: a zeroed stat filled by fstat on a live descriptor.
    unsafe {
        let mut st: libc::stat = std::mem::zeroed();
        libc::fstat(raw, &mut st) == 0
            && matches!(st.st_mode & libc::S_IFMT, libc::S_IFREG | libc::S_IFDIR)
    }
}

/// `f` on the descriptor behind a live handle, under its table's lock;
/// `None` for a closed or forged handle.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn with_raw<R>(table: Table, h: i64, f: impl FnOnce(RawFd) -> R) -> Option<R> {
    match table {
        Table::Fs => crate::fs::with_handle(h, |file| f(file.as_raw_fd())),
        Table::Net => crate::net::with_raw(h, f),
    }
}

/// `os_ready_open()` (`[os.ready.set]`): an empty set's handle, or a code.
pub fn set_open() -> Result<i64, i64> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let kfd = kernel::open().map_err(|e| {
            crate::fs::os_error_record(&e);
            proc_code::IO
        })?;
        let mut reg = sets();
        let handle = reg.next;
        reg.next += 1;
        reg.sets.push(Set {
            handle,
            kfd: Arc::new(kfd),
            pid: std::process::id(),
            slots: Vec::new(),
            free: Vec::new(),
            by_handle: std::collections::HashMap::new(),
            generation: 0,
            wake: None,
            in_kernel: 0,
        });
        OPEN.fetch_add(1, Ordering::Relaxed);
        Ok(handle)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(proc_code::UNSUPPORTED)
    }
}

/// `os_ready_set(r, handle, interest, key)` (`[os.ready.set]`): add the
/// entry for `handle`, or change its interest and key; interest 0 removes
/// it. The order of refusal: interest outside 0..3 or a negative key
/// `invalid`; windows `unsupported`; a forged, closed or inherited set, a
/// forged or closed handle, the removal of a handle the set does not hold,
/// and a host refusal `io`.
pub fn set_put(r: i64, handle: i64, interest: i64, key: i64) -> Result<(), i64> {
    if !(0..=3).contains(&interest) || key < 0 {
        return Err(proc_code::INVALID);
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let Some((table, h, write)) = decode(handle) else {
            return Err(proc_code::IO);
        };
        if write {
            return Err(proc_code::IO);
        }
        let pid = std::process::id();
        if interest == 0 {
            let mut reg = sets();
            let Some(s) = reg.get_mut(r).filter(|s| s.pid == pid) else {
                return Err(proc_code::IO);
            };
            return if s.drop_entry((table, h)) {
                Ok(())
            } else {
                Err(proc_code::IO)
            };
        }
        // Under the handle's table lock, so the descriptor stays this
        // handle's until the kernel holds it (a close waits, then removes
        // it through `forget`).
        let done = with_raw(table, h, |raw| -> Result<(), i64> {
            let mut reg = sets();
            let Some(s) = reg.get_mut(r).filter(|s| s.pid == pid) else {
                return Err(proc_code::IO);
            };
            if let Some(&idx) = s.by_handle.get(&(table, h)) {
                let token = {
                    let slot = s.slots[idx].as_ref().expect("a held slot");
                    Set::token(idx, slot.generation)
                };
                let slot = s.slots[idx].as_mut().expect("a held slot");
                if !slot.polled && slot.interest != interest {
                    kernel::change(&s.kfd, slot.raw, slot.interest, interest, token)
                        .map_err(io_of)?;
                }
                slot.interest = interest;
                slot.key = key;
                return Ok(());
            }
            s.generation = s.generation.wrapping_add(1);
            let generation = s.generation;
            let idx = s.free.pop().unwrap_or_else(|| {
                s.slots.push(None);
                s.slots.len() - 1
            });
            let token = Set::token(idx, generation);
            let polled = if always_ready(raw) {
                true
            } else {
                match kernel::add(&s.kfd, raw, interest, token) {
                    Ok(()) => false,
                    Err(e) if kernel::refuses(e) => true,
                    Err(e) => {
                        s.free.push(idx);
                        return Err(io_of(e));
                    }
                }
            };
            if !polled {
                s.in_kernel += 1;
            }
            s.slots[idx] = Some(Slot {
                raw,
                interest,
                key,
                generation,
                polled,
            });
            s.by_handle.insert((table, h), idx);
            Ok(())
        });
        done.unwrap_or(Err(proc_code::IO))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (r, handle);
        Err(proc_code::UNSUPPORTED)
    }
}

/// One look at set `r`: `wake` registered in the kernel set (or taken out
/// of it, for a wait that asks for no meaning), then the kernel's harvest
/// and the polled entries; `[key, facts, …]` of every ready entry, `None`
/// when none was. `Err` is the errno (EBADF: the set was closed).
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn set_look(r: i64, wake: Option<RawFd>, timeout: i32) -> Result<Option<Vec<i64>>, i32> {
    let (kfd, polled, room) = {
        let mut reg = sets();
        let Some(s) = reg.get_mut(r) else {
            return Err(libc::EBADF);
        };
        match (wake, s.wake) {
            (Some(w), None) => {
                kernel::add(&s.kfd, w, facts::READ, WAKE_TOKEN)?;
                s.wake = Some(w);
            }
            (None, Some(w)) => {
                let _ = kernel::del(&s.kfd, w, facts::READ);
                s.wake = None;
            }
            _ => {}
        }
        let polled: Vec<(RawFd, libc::c_short, u64)> = s
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| {
                let slot = slot.as_ref()?;
                slot.polled.then(|| {
                    (
                        slot.raw,
                        poll_events(slot.interest),
                        Set::token(i, slot.generation),
                    )
                })
            })
            .collect();
        (s.kfd.clone(), polled, s.in_kernel + 1)
    };
    let mut seen: Vec<(u64, Seen)> = if polled.is_empty() {
        kernel::harvest(&kfd, room, timeout)?
    } else {
        let mut fds = Vec::with_capacity(polled.len() + 1);
        fds.push((kfd.as_raw_fd(), libc::POLLIN));
        fds.extend(polled.iter().map(|&(fd, ev, _)| (fd, ev)));
        let re = poll_many(&fds, timeout)?;
        let mut seen = if re[0] != 0 {
            kernel::harvest(&kfd, room, 0)?
        } else {
            Vec::new()
        };
        for (&(_, _, token), &r) in polled.iter().zip(&re[1..]) {
            if r != 0 {
                seen.push((token, seen_of_poll(r)));
            }
        }
        seen
    };
    // kqueue answers read and write of one entry as two events: one
    // answer each entry.
    seen.retain(|&(t, _)| t != WAKE_TOKEN);
    let reg = sets();
    let Some(s) = reg.sets.iter().find(|s| s.handle == r) else {
        return Err(libc::EBADF);
    };
    let mut answered: Vec<(usize, i64, i64)> = Vec::new();
    for (token, ev) in seen {
        let Some((idx, slot)) = s.slot_of(token) else {
            continue;
        };
        let f = facts_of(ev, slot.interest);
        if f == 0 {
            continue;
        }
        match answered.iter_mut().find(|a| a.0 == idx) {
            Some(a) => a.2 |= f,
            None => answered.push((idx, slot.key, f)),
        }
    }
    let out: Vec<i64> = answered.iter().flat_map(|&(_, k, f)| [k, f]).collect();
    Ok((!out.is_empty()).then_some(out))
}

/// `os_ready_wait(r, set, deadline_ms)` (`[os.ready.set]`): `[os.ready.wait]`
/// over the set's entries, `[key, facts, …]` in the kernel's order.
pub fn set_wait(r: i64, set: i64, deadline_ms: i64) -> Result<(Vec<i64>, i64), i64> {
    shape(set, deadline_ms)?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        {
            let mut reg = sets();
            let Some(s) = reg
                .get_mut(r)
                .filter(|s| s.pid == std::process::id())
            else {
                return Err(proc_code::IO);
            };
            if s.by_handle.is_empty() && set == 0 && deadline_ms == -1 {
                return Err(proc_code::IO);
            }
        }
        let look = |wake: Option<RawFd>, timeout: i32| set_look(r, wake, timeout);
        let (ready, m) = crate::signal::wait_looking(set, deadline_ms, look).map_err(io_of)?;
        Ok((ready.unwrap_or_default(), m))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = r;
        Err(proc_code::UNSUPPORTED)
    }
}

/// `os_ready_close(r)` (`[os.ready.set]`): the set closed; `io` for a
/// forged or closed one. In a fork's child the copy is freed, and on a
/// kqueue host, where the child was handed none, its number is not closed.
pub fn set_close(r: i64) -> Result<(), i64> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let gone = {
            let mut reg = sets();
            let Some(i) = reg.sets.iter().position(|s| s.handle == r) else {
                return Err(proc_code::IO);
            };
            reg.sets.remove(i)
        };
        OPEN.fetch_sub(1, Ordering::Relaxed);
        if gone.pid != std::process::id() && !kernel::INHERITED {
            if let Ok(fd) = Arc::try_unwrap(gone.kfd) {
                let _ = std::os::fd::IntoRawFd::into_raw_fd(fd);
            }
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = r;
        Err(proc_code::IO)
    }
}

/// A handle leaves its table (`fs_close`, `net_close`): its entry leaves
/// every set of this process, before the descriptor closes. A program with
/// no set open pays one atomic load.
pub(crate) fn forget(table: Table, h: i64) {
    if OPEN.load(Ordering::Relaxed) == 0 {
        return;
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let pid = std::process::id();
        let mut reg = sets();
        for s in reg.sets.iter_mut().filter(|s| s.pid == pid) {
            s.drop_entry((table, h));
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (table, h);
    }
}

// ------------------------------------------------------ the kernels --

/// linux (and PAX): epoll.
#[cfg(target_os = "linux")]
mod kernel {
    use super::{RawFd, Seen, facts};
    use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd};

    /// A fork's child shares its parent's epoll object (its copy is closed
    /// at its own `os_ready_close`).
    pub const INHERITED: bool = true;

    fn errno() -> i32 {
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO)
    }

    pub fn open() -> std::io::Result<OwnedFd> {
        // SAFETY: a plain syscall; the descriptor is ours.
        let fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: a fresh descriptor this call owns.
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    fn events(interest: i64) -> u32 {
        let mut ev = 0;
        if interest & facts::READ != 0 {
            ev |= (libc::EPOLLIN | libc::EPOLLRDHUP) as u32;
        }
        if interest & facts::WRITE != 0 {
            ev |= libc::EPOLLOUT as u32;
        }
        ev
    }

    fn ctl(k: &OwnedFd, op: libc::c_int, raw: RawFd, interest: i64, token: u64) -> Result<(), i32> {
        let mut ev = libc::epoll_event {
            events: events(interest),
            u64: token,
        };
        // SAFETY: a live epoll descriptor and an event it reads.
        if unsafe { libc::epoll_ctl(k.as_raw_fd(), op, raw, &mut ev) } < 0 {
            return Err(errno());
        }
        Ok(())
    }

    pub fn add(k: &OwnedFd, raw: RawFd, interest: i64, token: u64) -> Result<(), i32> {
        ctl(k, libc::EPOLL_CTL_ADD, raw, interest, token)
    }

    pub fn change(k: &OwnedFd, raw: RawFd, _old: i64, interest: i64, token: u64) -> Result<(), i32> {
        ctl(k, libc::EPOLL_CTL_MOD, raw, interest, token)
    }

    pub fn del(k: &OwnedFd, raw: RawFd, _interest: i64) -> Result<(), i32> {
        ctl(k, libc::EPOLL_CTL_DEL, raw, 0, 0)
    }

    /// EPERM: a descriptor epoll will not hold (a regular file, `/dev/null`;
    /// PAX's console).
    pub fn refuses(e: i32) -> bool {
        e == libc::EPERM
    }

    /// One `epoll_wait` with room for every registration.
    pub fn harvest(k: &OwnedFd, room: usize, timeout: i32) -> Result<Vec<(u64, Seen)>, i32> {
        let mut evs = vec![libc::epoll_event { events: 0, u64: 0 }; room.max(1)];
        let cap = i32::try_from(evs.len()).unwrap_or(i32::MAX);
        // SAFETY: `cap` writable events.
        let n = unsafe { libc::epoll_wait(k.as_raw_fd(), evs.as_mut_ptr(), cap, timeout) };
        if n < 0 {
            return Err(errno());
        }
        Ok(evs[..n as usize]
            .iter()
            .map(|e| {
                let v = e.events;
                let token = e.u64;
                (
                    token,
                    Seen {
                        read: v & libc::EPOLLIN as u32 != 0,
                        write: v & libc::EPOLLOUT as u32 != 0,
                        rdhup: v & libc::EPOLLRDHUP as u32 != 0,
                        wrhup: false,
                        hup: v & libc::EPOLLHUP as u32 != 0,
                        err: v & libc::EPOLLERR as u32 != 0,
                    },
                )
            })
            .collect())
    }
}

/// macOS: kqueue.
#[cfg(target_os = "macos")]
mod kernel {
    use super::{RawFd, Seen, facts};
    use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd};

    /// A fork's child is handed no kqueue (kqueue(2)): its number is never
    /// closed there.
    pub const INHERITED: bool = false;

    pub fn open() -> std::io::Result<OwnedFd> {
        // SAFETY: plain syscalls on a descriptor this call owns.
        unsafe {
            let fd = libc::kqueue();
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let owned = OwnedFd::from_raw_fd(fd);
            if libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(owned)
        }
    }

    fn change_of(raw: RawFd, filter: i16, flags: u16, token: u64) -> libc::kevent {
        // SAFETY: an all-zero kevent is a valid value.
        let mut k: libc::kevent = unsafe { std::mem::zeroed() };
        k.ident = raw as usize;
        k.filter = filter;
        k.flags = flags | libc::EV_RECEIPT;
        k.udata = token as usize as *mut libc::c_void;
        k
    }

    /// Apply `changes` with a receipt for each; the first change's error, if
    /// any (a delete of a filter the set does not hold is not one).
    fn apply(k: &OwnedFd, changes: &[libc::kevent]) -> Result<(), i32> {
        if changes.is_empty() {
            return Ok(());
        }
        // SAFETY: an all-zero kevent is a valid value.
        let mut out: Vec<libc::kevent> = vec![unsafe { std::mem::zeroed() }; changes.len()];
        let zero = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `changes.len()` readable and writable kevents.
        let n = unsafe {
            libc::kevent(
                k.as_raw_fd(),
                changes.as_ptr(),
                changes.len() as libc::c_int,
                out.as_mut_ptr(),
                out.len() as libc::c_int,
                &zero,
            )
        };
        if n < 0 {
            return Err(std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EIO));
        }
        for (c, r) in changes.iter().zip(&out[..n as usize]) {
            let e = r.data as i32;
            if r.flags & libc::EV_ERROR != 0 && e != 0 && !(c.flags & libc::EV_DELETE != 0 && e == libc::ENOENT) {
                return Err(e);
            }
        }
        Ok(())
    }

    fn filters(interest: i64) -> [(i16, bool); 2] {
        [
            (libc::EVFILT_READ, interest & facts::READ != 0),
            (libc::EVFILT_WRITE, interest & facts::WRITE != 0),
        ]
    }

    pub fn add(k: &OwnedFd, raw: RawFd, interest: i64, token: u64) -> Result<(), i32> {
        let changes: Vec<libc::kevent> = filters(interest)
            .iter()
            .filter(|f| f.1)
            .map(|&(f, _)| change_of(raw, f, libc::EV_ADD, token))
            .collect();
        let r = apply(k, &changes);
        if r.is_err() {
            // A filter that went in before the refusal comes out again.
            let _ = del(k, raw, interest);
        }
        r
    }

    pub fn change(k: &OwnedFd, raw: RawFd, old: i64, interest: i64, token: u64) -> Result<(), i32> {
        let mut changes = Vec::new();
        for ((f, now), (_, before)) in filters(interest).iter().zip(filters(old).iter()) {
            if *now && !*before {
                changes.push(change_of(raw, *f, libc::EV_ADD, token));
            } else if !*now && *before {
                changes.push(change_of(raw, *f, libc::EV_DELETE, token));
            }
        }
        apply(k, &changes)
    }

    pub fn del(k: &OwnedFd, raw: RawFd, interest: i64) -> Result<(), i32> {
        let changes: Vec<libc::kevent> = filters(interest)
            .iter()
            .filter(|f| f.1)
            .map(|&(f, _)| change_of(raw, f, libc::EV_DELETE, 0))
            .collect();
        apply(k, &changes)
    }

    /// EINVAL: a descriptor kqueue will not hold (macOS's `/dev/tty`,
    /// `/dev/null`; measured).
    pub fn refuses(e: i32) -> bool {
        e == libc::EINVAL || e == libc::ENODEV
    }

    /// One `kevent` with room for both filters of every registration.
    pub fn harvest(k: &OwnedFd, room: usize, timeout: i32) -> Result<Vec<(u64, Seen)>, i32> {
        // SAFETY: an all-zero kevent is a valid value.
        let mut evs: Vec<libc::kevent> = vec![unsafe { std::mem::zeroed() }; room.max(1) * 2];
        let ts;
        let tsp = if timeout < 0 {
            std::ptr::null()
        } else {
            ts = libc::timespec {
                tv_sec: (timeout / 1000).into(),
                tv_nsec: ((timeout % 1000) as libc::c_long) * 1_000_000,
            };
            &raw const ts
        };
        // SAFETY: `evs.len()` writable kevents, no changes.
        let n = unsafe {
            libc::kevent(
                k.as_raw_fd(),
                std::ptr::null(),
                0,
                evs.as_mut_ptr(),
                evs.len() as libc::c_int,
                tsp,
            )
        };
        if n < 0 {
            return Err(std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EIO));
        }
        Ok(evs[..n as usize]
            .iter()
            .map(|e| {
                let token = e.udata as usize as u64;
                let eof = e.flags & libc::EV_EOF != 0;
                let read = e.filter == libc::EVFILT_READ;
                (
                    token,
                    Seen {
                        read,
                        write: !read,
                        rdhup: read && eof,
                        wrhup: !read && eof,
                        hup: false,
                        err: e.flags & libc::EV_ERROR != 0 || (eof && e.fflags != 0),
                    },
                )
            })
            .collect())
    }
}

// --------------------------------------------------------- the shims --

/// The answer list minted for the program, and the pair through `out`.
///
/// # Safety
///
/// `out` must address 16 writable bytes.
unsafe fn answer(out: i64, ready: &[i64], meaning: i64) {
    let list = crate::list::new_list(8);
    for &v in ready {
        crate::list::push_int(list, v);
    }
    unsafe { crate::str::write_pair(out, list as i64, meaning) };
}

/// `os_wait_ready(entries: List[int], set: int, deadline_ms: int) ->
/// (List[int], int) ! {invalid, io, unsupported}` — 0 and the pair (the
/// minted `[index, facts, …]` list, the meaning) through `out`, or an
/// `os::proc_code`. The elements are the list's words, copied before the
/// wait.
///
/// # Safety
///
/// `entries` must be a live `List[int]` header; `out` must address 16
/// writable bytes.
unsafe fn __wolf_rt_os_wait_ready_parks(entries: i64, set: i64, deadline_ms: i64, out: i64) -> i64 {
    crate::fs::os_error_clear();
    let Some(elems) = (unsafe { crate::list::i64_elems(entries) }).map(<[i64]>::to_vec) else {
        return proc_code::IO;
    };
    match wait_ready(&elems, set, deadline_ms) {
        Ok((ready, m)) => {
            unsafe { answer(out, &ready, m) };
            proc_code::OK
        }
        Err(c) => c,
    }
}

/// The exported shim: [`__wolf_rt_os_wait_ready_parks`], then a killed
/// compiled task leaves instead of returning into the program.
///
/// # Safety
///
/// As [`__wolf_rt_os_wait_ready_parks`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_wait_ready(entries: i64, set: i64, deadline_ms: i64, out: i64) -> i64 {
    let r = unsafe { __wolf_rt_os_wait_ready_parks(entries, set, deadline_ms, out) };
    // SAFETY: the shim's last step; the body's locals are dropped.
    unsafe { crate::leave_if_killed() };
    r
}

/// `os_ready_open() -> int ! {io, unsupported}` — the set's handle (>= 0),
/// or `-code`.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_ready_open() -> i64 {
    crate::fs::os_error_clear();
    match set_open() {
        Ok(h) => h,
        Err(c) => -c,
    }
}

/// `os_ready_set(r, handle, interest, key) -> () ! {invalid, io,
/// unsupported}` — 0, or the code.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_ready_set(r: i64, handle: i64, interest: i64, key: i64) -> i64 {
    crate::fs::os_error_clear();
    match set_put(r, handle, interest, key) {
        Ok(()) => proc_code::OK,
        Err(c) => c,
    }
}

/// `os_ready_wait(r, set, deadline_ms) -> (List[int], int) ! {invalid, io,
/// unsupported}` — 0 and the pair through `out`, or the code.
///
/// # Safety
///
/// `out` must address 16 writable bytes.
unsafe fn __wolf_rt_os_ready_wait_parks(r: i64, set: i64, deadline_ms: i64, out: i64) -> i64 {
    crate::fs::os_error_clear();
    match set_wait(r, set, deadline_ms) {
        Ok((ready, m)) => {
            unsafe { answer(out, &ready, m) };
            proc_code::OK
        }
        Err(c) => c,
    }
}

/// The exported shim: [`__wolf_rt_os_ready_wait_parks`], then a killed
/// compiled task leaves instead of returning into the program.
///
/// # Safety
///
/// As [`__wolf_rt_os_ready_wait_parks`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_os_ready_wait(r: i64, set: i64, deadline_ms: i64, out: i64) -> i64 {
    let rc = unsafe { __wolf_rt_os_ready_wait_parks(r, set, deadline_ms, out) };
    // SAFETY: the shim's last step; the body's locals are dropped.
    unsafe { crate::leave_if_killed() };
    rc
}

/// `os_ready_close(r) -> () ! {io}` — 0, or the code.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_os_ready_close(r: i64) -> i64 {
    crate::fs::os_error_clear();
    match set_close(r) {
        Ok(()) => proc_code::OK,
        Err(c) => c,
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;

    fn pipe() -> (i64, i64) {
        let (r, w) = std::io::pipe().expect("a pipe");
        let r = crate::fs::mint_file(std::fs::File::from(OwnedFd::from(r)));
        let w = crate::fs::mint_file(std::fs::File::from(OwnedFd::from(w)));
        (r, w)
    }

    fn put_byte(w: i64) {
        crate::fs::with_handle(w, |f| {
            use std::io::Write as _;
            (&*f).write_all(b"x").expect("a byte");
        })
        .expect("live");
    }

    #[test]
    fn decode_reads_the_two_bits() {
        assert_eq!(decode(5), Some((Table::Fs, 5, false)));
        assert_eq!(decode(5 + NET), Some((Table::Net, 5, false)));
        assert_eq!(decode(5 + WRITE), Some((Table::Fs, 5, true)));
        assert_eq!(decode(5 + NET + WRITE), Some((Table::Net, 5, true)));
        assert_eq!(decode(-1), None);
    }

    #[test]
    fn facts_come_with_an_interest_asked() {
        let hup = Seen {
            hup: true,
            ..Seen::default()
        };
        assert_eq!(facts_of(hup, 1), 5);
        assert_eq!(facts_of(hup, 2), 6);
        assert_eq!(facts_of(hup, 3), 7);
        let rdhup = Seen {
            rdhup: true,
            write: true,
            ..Seen::default()
        };
        assert_eq!(facts_of(rdhup, 3), 7);
        assert_eq!(facts_of(rdhup, 2), 6);
        let err = Seen {
            err: true,
            write: true,
            ..Seen::default()
        };
        assert_eq!(facts_of(err, 2), 10);
        let quiet = Seen {
            write: true,
            ..Seen::default()
        };
        assert_eq!(facts_of(quiet, 1), 0);
    }

    #[test]
    fn the_one_shot_answers_every_ready_element() {
        let (r, w) = pipe();
        assert_eq!(wait_ready(&[r, w + WRITE], 0, 0), Ok((vec![1, 2], 0)));
        put_byte(w);
        assert_eq!(wait_ready(&[r, w + WRITE, r], 0, 0), Ok((vec![0, 1, 1, 2, 2, 1], 0)));
        assert_eq!(wait_ready(&[], 0, -1), Err(proc_code::IO));
        assert_eq!(wait_ready(&[r], 0, -2), Err(proc_code::INVALID));
        assert_eq!(wait_ready(&[1_000_000], 0, 0), Err(proc_code::IO));
        assert_eq!(wait_ready(&[NET + 1_000_000], 0, 0), Err(proc_code::IO));
        assert_eq!(wait_ready(&[-3], 0, 0), Err(proc_code::IO));
        assert_eq!(wait_ready(&[], 0, 0), Ok((vec![], 0)));
        let t0 = std::time::Instant::now();
        let (rr, ww) = pipe();
        assert_eq!(wait_ready(&[rr], 0, 25), Ok((vec![], 0)));
        assert!(t0.elapsed() >= std::time::Duration::from_millis(25));
        let _ = crate::fs::__wolf_rt_fs_close(ww);
        assert_eq!(wait_ready(&[rr], 0, 1000), Ok((vec![0, 5], 0)));
        for h in [r, w, rr] {
            let _ = crate::fs::__wolf_rt_fs_close(h);
        }
    }

    #[test]
    fn the_set_survives_changes_and_forgets_a_closed_handle() {
        let (r, w) = pipe();
        let s = set_open().expect("a set");
        assert_eq!(set_put(s, r, 4, 1), Err(proc_code::INVALID));
        assert_eq!(set_put(s, r, 1, -1), Err(proc_code::INVALID));
        assert_eq!(set_put(s, r, 0, 1), Err(proc_code::IO));
        assert_eq!(set_put(s, 1_000_000, 1, 1), Err(proc_code::IO));
        assert_eq!(set_put(s, r + WRITE, 1, 1), Err(proc_code::IO));
        set_put(s, r, 1, 7).expect("added");
        set_put(s, w, 2, 8).expect("added");
        let (got, m) = set_wait(s, 0, 0).expect("a wait");
        assert_eq!((got, m), (vec![8, 2], 0));
        put_byte(w);
        let (mut got, _) = set_wait(s, 0, 0).expect("a wait");
        if got[0] != 7 {
            got.rotate_left(2);
        }
        assert_eq!(got, vec![7, 1, 8, 2]);
        set_put(s, r, 1, 9).expect("changed");
        set_put(s, w, 0, 0).expect("removed");
        assert_eq!(set_wait(s, 0, 0), Ok((vec![9, 1], 0)));
        let _ = crate::fs::__wolf_rt_fs_close(w);
        assert_eq!(set_wait(s, 0, 1000), Ok((vec![9, 5], 0)));
        let _ = crate::fs::__wolf_rt_fs_close(r);
        assert_eq!(set_wait(s, 0, 0), Ok((vec![], 0)));
        assert_eq!(set_wait(s, 0, -1), Err(proc_code::IO));
        assert_eq!(set_put(s, r, 1, 1), Err(proc_code::IO));
        set_close(s).expect("closed");
        assert_eq!(set_close(s), Err(proc_code::IO));
        assert_eq!(set_wait(s, 0, 0), Err(proc_code::IO));
    }

    #[test]
    fn a_regular_file_is_a_polled_entry_and_always_ready() {
        let dir = std::env::temp_dir().join(format!("wolf-rt-ready-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("f");
        std::fs::write(&path, b"f").unwrap();
        let f = crate::fs::mint_file(std::fs::File::open(&path).unwrap());
        let s = set_open().expect("a set");
        set_put(s, f, 3, 4).expect("added");
        assert_eq!(set_wait(s, 0, -1), Ok((vec![4, 3], 0)));
        assert_eq!(wait_ready(&[f, f + WRITE], 0, 0), Ok((vec![0, 1, 1, 2], 0)));
        set_close(s).expect("closed");
        let _ = crate::fs::__wolf_rt_fs_close(f);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
