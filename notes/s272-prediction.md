# s272 — the OS surface: prediction, before the first change

Contract: wolffe-lang/wolf `sprints/compiler/s272-the-os-surface.md` at planning `3737daa`
(rules `sprints/the-lane-rules-1031.md` with its 2026-11-01 section, `sprints/RULES.md`).
Written 2026-11-01 20:25Z on nomad, before any change to the tree.

## Inputs, read
- Branch `s272` cut at origin/trunk `7332772f`.
- wolf-lang#686, #921, #922, #976, #975, whole (one comment, on #686).
- Spec 11 `[os.fs.stat]`, `[os.fs.error]`, `[os.signal.disp.one]`, `[os.signal.ignored]`,
  `[os.signal.send]`, `[os.term.mode]`, `[os.term.restore]`, `[os.host.sigs]`.
- s253's commits (`04d9ea3a` .. `b308dafc`) as the house shape of a hosted builtin end to end:
  wolf_rt, the clif symbol table, sema (prelude, signature, sandbox), WIR, the checked
  machine, the driver's capabilities, the counts' tests, spec, anchors, prelude.json, the
  corpus snapshots, the CHANGELOG.
- yew `1981d81` `notes/yw08-evidence/onlcr/probe.py` (the ONLCR witness's shape); boreutils
  `0f7a57b` `tools/roottest` (read, not moved: a throwaway container per case, the programs
  copied in, `--network none`, each case as a user and as root inside the container).
- Base, measured on nomad (`~/lanes/s272/ev/base`, wolf built at `7332772f`): each issue's
  witness is `E0301` at `wolf build` exit 2 (`fs_statfs`, `os_chroot`, `os_setpriority`,
  `os_signal_action_one`); `os_term_set_mode(0, 1 << 24)` answers `invalid`.
- Black box, Darwin 26.4.1 arm64 (`~/lanes/s272/ev/m0/sigget.c`): `sigaction(sig, NULL, &old)`
  is -1/EINVAL (22) for 9 (KILL), 17 (STOP), 0 and 32..64; 0 for HUP, INT, TERM, PIPE, SEGV, BUS.
- Hosts: kasumi is off limits (the only btrfs this org has); hasu has ext4 (`/`), tmpfs and vfat;
  a container adds overlay. ubuntu:24.04 and `localhost/r41-toolchain:1` (Ubuntu 24.04,
  glibc 2.39, GNU coreutils 9.4) are on hasu; hasu's own GNU coreutils is 9.11.

## The shapes (R-s272-N; each recommendation is what will be built)
- **R-s272-1, the file-system record.** `fs_statfs(path: str) -> List[int] ! {denied, io,
  not_found}`, twelve numbered words, `[os.fs.stat]`'s shape: 0 `have` (bit i set when word i was
  answered; bits 0 always), 1 block size (`f_bsize`; macOS `f_iosize`), 2 fragment size
  (`f_frsize`; macOS `f_bsize`), the unit of 3..5; 3 blocks, 4 free, 5 available, 6 files,
  7 free files, 8 name max, 9 flags, 10 type (`f_type`), 11 the file-system id. (A) this; (B)
  `have` at word 3 as `fs_stat`'s — there is no `fs_fstat` prefix to keep; (C) a pair of calls
  (space, inodes). Recommend A.
- **R-s272-2, the flags word.** (A) POSIX's two `statvfs` flags as wolf's bits, bit 0 read-only
  (`ST_RDONLY`), bit 1 no set-id (`ST_NOSUID`), the same on every host; (B) the host's raw flag
  word (linux `ST_*`, macOS `MNT_*`: one number, two meanings). Recommend A.
- **R-s272-3, macOS's name max.** macOS `statfs` has none. (A) `pathconf(path, _PC_NAME_MAX)`, a
  second call, stated; (B) `statvfs`'s `f_namemax` (a second call that also truncates the block
  counts to 32 bits, so only that word would be read); (C) absent from `have`. Recommend A.
- **R-s272-4, windows' units.** (A) `GetDiskFreeSpaceExW` alone: the space words in bytes,
  fragment size 1, block size and every inode, name, flag, type and id word absent; (B) a second
  call (`GetDiskFreeSpaceW`) for the cluster size and the counts divided by it. Recommend A.
- **R-s272-5, the root.** `os_chroot(path: str) -> () ! {denied, io, not_found, unsupported}`.
  (A) `chroot(2)` alone, the working directory left as it is (a program calls `os_chdir("/")`
  after, as coreutils' `chroot` does); (B) the change of root and a `chdir("/")` in one call.
  Recommend A. Errors: `EPERM`/`EACCES` `denied`, `ENOENT` `not_found`, anything else `io`
  (`os_chdir`'s map), windows `unsupported` (the path has no shape of its own to check first).
- **R-s272-6, the priority.** `os_getpriority() -> int ! {unsupported}` (`getpriority(PRIO_PROCESS,
  0)`, errno cleared first: -1 is a value) and `os_setpriority(delta: int) -> () ! {denied, io,
  unsupported}`: `nice(2)`'s shape, the value read, `delta` added (saturating), clamped to
  `[PRIO_MIN, PRIO_MAX]` and set. EPERM and EACCES are both `denied`, told apart by `os_error()`
  (1 or 13) and `os_error_text` — `[os.fs.error]`'s rule: a row is a response, the number a reason
  for a reader, and GNU `nice` treats both alike and prints the host's text. (A) this; (B) two
  tags (a new host-table word for one call); (C) the setter answers the new value (a third call
  to read it back). Recommend A.
- **R-s272-7, ONLCR.** (A) bit 24 of the mode word, `RAW_LF` (16777216): a written LF reaches the
  terminal as LF even with output processing on (`ONLCR` CLEARED) — s248's family rule, the bit
  names the setting a terminal does not usually have, so 0 is what a terminal is given and a
  mode read and set back restores `ONLCR` exactly; `invalid` moves to bit 25 and above; (B) a
  raw saved `termios` word (s219 R6 C rejected a raw image: not portable); (C) a positive `ONLCR`
  bit (every mode built by hand before it would turn `ONLCR` off). Recommend A.
- **R-s272-8, a signal's action.** `os_signal_action_one(meaning: int) -> int ! {invalid, io,
  unsupported}`: 0 default, 1 ignored, 2 another handler. #975 sketched a payload tag
  `QueryFailed(int)`; no host row carries a payload (`[os.host.sigs]`, D30's payload-less rows,
  `[os.fs.error]`'s design note). (A) the GET's refusal is `io` and its number is `os_error()`
  (Darwin's EINVAL on KILL and STOP is `io` with 22), `invalid` the shape alone; (B) a new
  payload-less tag `query_failed` (a new host-table word whose only reader is this call); (C)
  the payload tag (a new row shape on every machine and in lupin). Recommend A.
- **R-s272-9, which meanings it reads, and whose answer.** The SENDING shape (`[os.signal.send]`:
  one bit of the 28 or `-N`), 0 `invalid`; `KILL`, `STOP` and the C library's own numbers are
  asked of the host, which answers (linux: KILL and STOP default; Darwin: EINVAL). The answer is
  the program's view: a listened meaning is 2 (the runtime's trampoline), and `SEGMENT_FAULT`/
  `BUS_ERROR` under the runtime's stack guard answer the program's recorded choice for a sent
  one (`[os.signal.disp.one]`: 1 ignored, else 0), never the guard's own handler. (A) this; (B)
  the host's raw answer (the guard reads 2 though the program chose nothing). Recommend A.
- **R-s272-10, capabilities.** `fs_statfs` `fs` (the family); `os_chroot` and `os_setpriority`
  `exec` (with `os_set_ids` and `os_set_core_limit`: process control); `os_getpriority` and
  `os_signal_action_one` `env` (with `os_pgid` and `os_signal_ignored`: reads of this process).
  All five refused at comptime. Recommend as listed.
- **R-s272-11, the checked machine.** It makes the host's call for the `wolf` process it runs as
  (`statfs`, `chroot`, the priority — on linux the calling thread's, as for every machine), and
  for the action query refuses `SEGMENT_FAULT` and `BUS_ERROR` by name (the `wolf` process's own
  guard; `[os.signal.disp.one]`'s rule) and reads `BROKEN_PIPE` from its inherited record while
  the program has not touched it (`os_signal_ignored`'s rule). Recommend as listed.

## What each change will show
1. **The rows (committed first, red).** `corpus/os/statfs_rows.lu`, `chroot_rows.lu`,
   `priority_rows.lu`, `signal_action_rows.lu`, `term_lf_rows.lu`; fixtures under
   `crates/wolf_driver/tests/fixtures/os_surface/`; `crates/wolf_driver/tests/os_surface_lanes.rs`.
   At this base each new name is `E0301` (exit 2), which the gate fails on; the ONLCR rows answer
   `invalid` for bit 24 and see `ONLCR-A LF ONLCR-B` on a terminal started with ONLCR off.
2. **wolf_rt** (statfs, chroot, priority, ONLCR, the action query), **clif** (five shims, RT_SYMBOLS
   +5), **sema** (five signatures, sandbox categories), **WIR** (five lowerings), **the checked
   machine**, **the driver's capabilities**: each row turns green on checked, native and release;
   lupin 0.1.58 (the pairing) refuses each new name ("does not resolve"), pinned by version.
3. **The counts.** The host table 162 -> 166 lines (`os_term_set_mode` keeps its line), prelude
   functions +5, `RT_SYMBOLS` +5; `host_table_spec`, `prelude_names`, `sandbox_s272`,
   `freestanding_target` and `str_list_fs_native` move with them.
4. **Measured, within the same second.** `fs_statfs`'s words equal GNU `stat -f`'s (`%s %S %b %f %a
   %c %d %l %t %i`) on ext4, tmpfs and overlay (hasu, r41-toolchain's GNU 9.4), the free counts
   bracketed by a GNU read before and after; on macOS equal `/bin/df -k`'s 1024-blocks, used,
   available, iused and ifree. btrfs cannot be measured this week (kasumi only): said, not guessed.
5. **In throwaway containers (hasu rootless podman; CI's docker).** `os_chroot` as root: ok, and
   `/` is the new root after `os_chdir("/")`; as a user: `denied`, `os_error()` 1. A missing path
   `not_found` for both. `os_setpriority(+n)`: ok for both; a lowering: `denied` (13, "Permission
   denied") for the user and for root without CAP_SYS_NICE in the initial namespace (rootless
   podman, and docker's default set); ok for root with `--cap-add SYS_NICE` under docker. GNU
   `nice -n -1` prints the same host text in the same container.
6. **ONLCR.** On a pseudo-terminal the test opens, ONLCR on and off at start: the program reads
   `RAW_LF` as off/on, sets output processing with `ONLCR` (`RAW_OUTPUT` and `RAW_LF` clear), writes
   `ONLCR-A\nONLCR-B` to `/dev/tty`, and the wire reads `ONLCR-A\r\nONLCR-B` in both; after the
   program ends (by `os_term_set_mode` back, and by the restore registration on a return from
   `main`) the terminal's `ONLCR` is what it was at start — checked, native and release.
7. **The action query.** TERMINATE 0; after `os_signal_ignore_one(USER)` 1, back 0 after
   `default_one`; a listened RELOAD 2; KILL and STOP 0 on linux, `io` with `os_error()` 22 on
   Darwin (CI's macOS runner and nomad); 0 `invalid`; `os_signal_ignored()` the same before and
   after every query (no disposition moved).
8. **The plant** `s272-plant-statfs` (base + statfs reporting `free` as `available`) goes red by name
   in the GNU comparison row (`fs_statfs_matches_gnu_stat_f`) and the corpus row's `avail<=free`
   line stays true — so the comparison is the gate, measured.
