#!/usr/bin/env python3
"""s284 R-s284-3, MEASUREMENT ONLY (never pushed): one detector, applied the
same way to trunk's crates/wolf_rt/src/signal.rs and the fix's. Each event is
one raw write(2) line on fd 2 (no lock, no allocation). Hook points, all in
`mod sys` of signal.rs:
  (a) reader_loop, right after `back_from_poll();` returns:  AT_WORK = true
  (b) reader_loop, the loop's top, right before
      `READER_STATE.store(READER_POLLING, ...)`:               AT_WORK = false
  (c) fork_hold, on its successful POLLING -> HELD compare:  FORKING = true;
      AT_WORK set there writes "S284 CLAIM_AT_WORK"
  (d) fork_release, first statement:                          FORKING = false
  (e) reader_loop's drain branch: FORKING read right after the hub lock is
      taken and again after drain(), before the guard drops; either true
      writes "S284 LOCKED_IN_FORK" (the hub lock held during a fork's hold)
  (f) trunk only, back_from_poll's second look returning:     "S284 SLIP"
usage: measure2-patch.py <signal.rs>
"""
import sys

p = sys.argv[1]
s = open(p).read()


def rep(old, new, need=1):
    global s
    c = s.count(old)
    assert c == need, (old[:70], c)
    s = s.replace(old, new)


rep('''    /// s247: the drain thread back from its kernel wait — on, unless a fork
''', '''    /// s284 R-s284-3 MEASUREMENT ONLY (never pushed): the detector's flags.
    static S284_AT_WORK: AtomicBool = AtomicBool::new(false);
    static S284_FORKING: AtomicBool = AtomicBool::new(false);
    fn s284_note(msg: &[u8]) {
        // SAFETY: a write of a static buffer to fd 2.
        unsafe { libc::write(2, msg.as_ptr().cast(), msg.len()) };
    }

    /// s247: the drain thread back from its kernel wait — on, unless a fork
''')
# (f) trunk only
if '''            if READER_STATE.load(Ordering::SeqCst) != READER_HELD {
                return;
            }''' in s:
    rep('''            if READER_STATE.load(Ordering::SeqCst) != READER_HELD {
                return;
            }''', '''            if READER_STATE.load(Ordering::SeqCst) != READER_HELD {
                s284_note(b"S284 SLIP\\n");
                return;
            }''')
# (c)
rep('''                        .is_ok()
                    {
                        return;
                    }''', '''                        .is_ok()
                    {
                        S284_FORKING.store(true, Ordering::SeqCst);
                        if S284_AT_WORK.load(Ordering::SeqCst) {
                            s284_note(b"S284 CLAIM_AT_WORK\\n");
                        }
                        return;
                    }''')
# (d)
rep('''    pub fn fork_release() {
        if READER_STATE''', '''    pub fn fork_release() {
        S284_FORKING.store(false, Ordering::SeqCst);
        if READER_STATE''')
# (b)
rep('''            READER_STATE.store(READER_POLLING, Ordering::SeqCst);''',
    '''            S284_AT_WORK.store(false, Ordering::SeqCst);
            READER_STATE.store(READER_POLLING, Ordering::SeqCst);''')
# (a)
rep('''            back_from_poll();
''', '''            back_from_poll();
            S284_AT_WORK.store(true, Ordering::SeqCst);
''')
# (e)
rep('''                let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
                if drain(h, &mut st) {
                    h.cv.notify_all();
                }''', '''                let mut st = h.state.lock().unwrap_or_else(|p| p.into_inner());
                let in_fork = S284_FORKING.load(Ordering::SeqCst);
                if drain(h, &mut st) {
                    h.cv.notify_all();
                }
                if in_fork || S284_FORKING.load(Ordering::SeqCst) {
                    s284_note(b"S284 LOCKED_IN_FORK\\n");
                }''')
open(p, 'w').write(s)
print("patched", p)
