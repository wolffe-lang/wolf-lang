#!/usr/bin/env python3
"""s284's plant (one file, one hunk, off the line): the second look back in
the drain thread's `back_from_poll`. A claim that finds a fork's hold looks
at the state again and goes to work unless that look reads the hold too,
which is trunk's bug before s284 (wolf-lang#1022). Placed after the gap hook,
so the deterministic row stops the thread before the look exactly as it
stopped trunk's.
usage: plant-patch.py <crates/wolf_rt/src/signal.rs>"""
import sys

p = sys.argv[1]
s = open(p).read()
old = '''                Err(READER_HELD) => {
                    #[cfg(test)]
                    hook::after_failed_claim();
                    #[cfg(test)]
                    hook::PARKS.fetch_add(1, Ordering::SeqCst);'''
new = '''                Err(READER_HELD) => {
                    #[cfg(test)]
                    hook::after_failed_claim();
                    if READER_STATE.load(Ordering::SeqCst) != READER_HELD {
                        return;
                    }
                    #[cfg(test)]
                    hook::PARKS.fetch_add(1, Ordering::SeqCst);'''
assert s.count(old) == 1
open(p, "w").write(s.replace(old, new))
print("planted", p)
