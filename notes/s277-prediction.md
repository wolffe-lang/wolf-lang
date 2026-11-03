# s277 — the descriptor walk: prediction, before the first change

Contract: the s277 lane contract and the lane rules it cites. Written 2026-11-03 on nomad,
before any change to the tree.

## Inputs, read
- Branch `s277` cut at origin/trunk `0d6b3a64`.
- wolf-lang#707 (and its comment: a downstream package reader stages an archive into a
  caller-owned private directory), #1029, #1030, #1031, #1032, #1033, whole; #910 and #714 for
  the follow flag and the handle family.
- Spec 11 `[os.fs.path]`, `[os.fs.open]`, `[os.fs.open.nofollow]`, `[os.fs.stat]`,
  `[os.fs.fstat]`, `[os.fs.fstat.record]`, `[os.fs.readdir]`, `[os.fs.remove]`, `[os.fs.mode]`,
  `[os.fs.create.mode]`, `[os.fs.symlink]`, `[os.fs.link]`, `[os.fs.owner]`, `[os.fs.sync]`,
  `[os.fs.change.note]`, `[os.fs.gaps.note]`, `[os.host.sigs]`.
- s272's commits (`05e80e44` .. `0d6b3a64`) as the house shape of a builtin family end to end;
  s272's rig (`notes/s272-evidence/rig/`) read and copied by hand.
- wolf-std `78b9d3c` `std/fs/fs.lu` (`File { fd }`, `set_file_owner` over `fs_set_owner_fd`):
  where the std wrapper lane will sit. Not edited here.
- Clean room: the Darwin man pages on nomad and the Linux man-pages project's pages (rendered
  in a throwaway `ubuntu:24.04` image on hasu: open(2), openat(2), statx(2), unlinkat(2),
  mkdirat(2), symlinkat(2), linkat(2), fchmod(2), fdopendir(3), readdir(3)), and black-box
  runs. No libc or kernel source.

## Base, measured
- nomad, wolf built at `0d6b3a64`: every issue's witness is `E0301` at `wolf build` exit 2 and
  `fail(E0301)` on the checked machine: `fs_remove_at` (bu21's census program, verbatim),
  `fs_open_at`, `fs_lstat_at`, `fs_create_dir_at`, `fs_symlink_at`, `fs_link_at`,
  `fs_read_dir_entries_at`, `fs_set_mode_fd`; `fs_open_mode(file, 32)` answers `invalid` with
  `os_error()` 0 (bit 32 is outside the set).
- Black box (`notes/s277-evidence/probe/`), Darwin 26.4.1 arm64 and linux 7.2.4 (rootless
  podman on hasu, ext4, as uid 1000):
  - `O_DIRECTORY` on a file, a fifo with no writer (no park), a link to a file, `file/x`:
    `ENOTDIR` (20) on both. On a link to a directory with `O_NOFOLLOW` too: `ENOTDIR` on both
    (not `ELOOP`). With `O_WRONLY` or `O_RDWR`: `EISDIR`; with `O_CREAT`: `EINVAL` (nothing
    made) on both.
  - `openat` on a file's descriptor `ENOTDIR`; an empty name `ENOENT`; an absolute name
    ignores the handle (`/` opened); `O_NOFOLLOW` on a final link `ELOOP` (40 linux, 62 Darwin).
  - `unlinkat(d, dir, 0)`: linux `EISDIR` (21), Darwin `EPERM` (1) — `[os.fs.remove]`'s split.
    `AT_REMOVEDIR` on a file or a link to a directory `ENOTDIR`; not empty `ENOTEMPTY` (39, 66).
  - `symlinkat("", …)`: linux `ENOENT`, Darwin makes the link — `[os.fs.symlink]`'s split.
  - `linkat` flag 0 links a link itself, `AT_SYMLINK_FOLLOW` its target, on both; a dangling
    link followed `ENOENT`; a directory `EPERM`.
  - `fchmod` on a pipe: linux 0, Darwin `EINVAL` (22); on a directory handle 0; `04755` kept.
  - `fdopendir` on a duplicate, rewound, read twice: the same entries both times, every one
    typed by `d_type`; on a file's duplicate `ENOTDIR`.
  - 100 levels of 64-byte names (6504 bytes) made and removed by handle; the path form's
    `stat` and `rmdir` `ENAMETOOLONG` (36 linux, 63 Darwin).
  - linux `statx(2)` through `syscall(2)` with a struct laid out from the man page's field
    list (natural alignment, 256 bytes): every word equal to `fstatat(2)`'s for a file, a
    directory, a link (followed and not), a fifo and `/dev/null`; the birth time answered.

## The shapes (R-s277-N; each recommendation is what will be built)
- **R-s277-1, names.** `..` and multi-component names: (A) served as the host resolves them —
  a caller that needs confinement walks one component at a time with `O_NOFOLLOW` and
  `O_DIRECTORY`; (B) refused `invalid` (a `/` or a `..` component); (C) a beneath-only
  resolution (linux `openat2(RESOLVE_BENEATH)`, Darwin `O_RESOLVE_BENEATH`) — two different
  calls, none on older kernels. **Recommend A** (the contract's).
- **R-s277-2, the handle and the spellings.** (A) `dir: int`, a handle of the fs table (std's
  `File { fd }` wraps it in the next lane), one table for files and directories; a directory
  handle is told from a file handle by asking (`fs_fstat_record` kind 1) or by opening with the
  new bit, and a file handle passed as a directory is the host's `ENOTDIR`; names
  `fs_<path form>_at`, the handle first, the path form's arguments after it in their order:
  `fs_open_at(dir, name, open, perm)` (`fs_open_perm`'s), `fs_stat_at(dir, name)`,
  `fs_lstat_at(dir, name)`, `fs_remove_at(dir, name)`, `fs_remove_dir_at(dir, name)`,
  `fs_create_dir_at(dir, name, mode)` (`fs_create_dir_mode`'s), `fs_symlink_at(target, dir,
  name)` (symlinkat's order, `fs_symlink`'s target first), `fs_link_at(src_dir, src, dst_dir,
  dst, follow)`, `fs_read_dir_entries_at(dir)`; and `fs_set_mode_fd(fd, mode)`
  (`fs_set_owner_fd`'s spelling). (B) a `Dir` type of its own — no host builtin takes or answers
  a type the prelude does not have (`[os.host.sigs]`). **Recommend A.**
- **R-s277-3, `O_DIRECTORY`.** (A) **bit 32** of the open word of `fs_open_mode`,
  `fs_open_perm` and `fs_open_at`, beside bit 16 (`[os.fs.open.nofollow]`): it rides the two
  read modes (0 and 5) only, and with modes 1–4 it is `invalid` before the host (a directory is
  never opened for writing — `EISDIR` — and `O_CREAT` with it is `EINVAL` on both hosts,
  measured); windows refuses the bit `invalid` before the host, as it refuses bit 16. Bits 32
  and 16 together are one open. The only widening of an existing call is this one: 32..63 were
  `invalid` before, and every argument served before answers as it did. (B) a call of its own,
  `fs_open_dir(path)`; (C) the bit on every mode, the host's answer. **Recommend A.**
- **R-s277-4, the refusals' row.** `ENOTDIR` (the bit, a file handle as `dir`, a middle
  component) and `ELOOP` (`O_NOFOLLOW`, a loop on the way): (A) **`invalid`**, the host's number
  in `os_error()` — s254's R2 row for the no-follow refusal and `fs_sync_dir`'s for a name that is
  no directory; on every `…_at` call both numbers are `invalid`; on `fs_open_mode` and
  `fs_open_perm` only under the bit that asked for the refusal, so no answer without the bit
  moves; (B) two new marks (`not_dir`, `link`); (C) `io` with the number. **Recommend A.**
- **R-s277-5, a name's shape.** (A) an empty name and an absolute one are `invalid` before the
  host (an absolute name ignores the handle — the escape the family exists to close — and an
  empty one names nothing; both hosts answer it `ENOENT`, measured); a NUL in it is `io`, the
  path family's rule; (B) the host's answers. **Recommend A.**
- **R-s277-6, the record by handle and name.** (A) linux: one `statx(2)` relative to the handle
  (`AT_SYMLINK_NOFOLLOW` for `fs_lstat_at`), through `syscall(2)` with the struct laid out from
  the man page (the binding's `statx` is absent on musl, measured on hasu), so the birth time is
  answered as `fs_stat`'s is, and `fstatat(2)` where the kernel refuses `statx`
  (`ENOSYS`/`EPERM`); every other unix one `fstatat(2)`, the birth time from `st_birthtime`;
  (B) `fstatat(2)` alone, linux's birth time absent from `have`; (C) an `O_PATH` open and a
  stat on it (three calls). **Recommend A.**
- **R-s277-7, the held directory's listing.** (A) a duplicate of the handle, rewound and read
  with `fdopendir(3)`/`readdir(3)` and closed — the listing starts at the first entry every time,
  and the duplicate shares the open directory's offset, so two listings of one handle at once
  share it (stated); a kind the entry does not carry is `fstatat` relative to the handle,
  no-follow; (B) `openat(dir, ".")` — a fresh description, but it needs search permission a
  listing does not; (C) `getdents64` (linux only). **Recommend A.**
- **R-s277-8, windows.** (A) every `…_at` call and `fs_set_mode_fd` answer **`unsupported`**
  after the shape: no handle of the table is a directory there (`fs_open` refuses one,
  `[os.fs.fstat]`), and Win32's create has no directory-relative form (a root handle is the
  native API's, `NtCreateFile`); windows has no permission bits for `fchmod`
  (`[os.fs.mode]`); (B) `CreateFileW` on a joined path — the path walk this lane removes.
  **Recommend A.**
- **R-s277-9, `fchmod`.** (A) `fs_set_mode_fd(fd, mode) -> () ! {denied, invalid, io,
  unsupported}` — `fs_set_mode`'s twelve bits on the file the handle holds (`fchmod(2)`, std's
  `File::set_permissions`), 0..2 the standard streams, `fs_set_owner_fd`'s row set; the handle
  family is `fs_set_owner_fd`, this call and `futimens` (#714), which stays out; (B) a method on
  `File` — std's lane. **Recommend A.**
- **R-s277-10, capabilities.** All ten are `fs`, with the family, refused at comptime.
- **R-s277-11, the checked machine.** (A) it makes the host's own calls on its own handles (the
  name is relative to the handle, never to the machine's working directory); (B) refuses the
  family by name. **Recommend A.**
- **R-s277-12, lupin.** The mirror is filed, not built (one wolf-interp issue, drafted for the
  orchestrator). lupin admits no `unsafe` and std has no `openat`: the draft asks it to resolve
  the ten names, serve bit 32 (std's `custom_flags`) and `fs_set_mode_fd` (`set_permissions`),
  and refuse the `…_at` calls by name. lupin 0.1.58, the pairing, resolves none: pinned here.

## What each change will show
1. **The rows, committed first, red.** Corpus rows `corpus/fs/at_rows.lu` (#707: bu21's
   `fs_remove_at(3, "name")` on a forged handle `io`, openat and fstatat relative to a handle
   whose path was renamed), `corpus/fs/mkdir_at.lu` (#1029), `corpus/fs/symlink_at.lu` (#1030),
   `corpus/fs/link_at.lu` (#1031), `corpus/fs/open_directory.lu` (#1032), `corpus/fs/mode_fd.lu`
   (#1033); each asserts both sides (served on unix, `unsupported`/`invalid` on windows). The
   lanes test `crates/wolf_driver/tests/descriptor_walk_lanes.rs` over
   `fixtures/descriptor_walk/`. At `0d6b3a64` every row is `E0301` (exit 2), which the gate
   fails on; the bit-32 row answers `invalid` without a host number.
2. **The race row.** A program makes its checks — `tree` and `tree/d` held by handle
   (`O_DIRECTORY|O_NOFOLLOW`), `tree/e` asked by `fs_lstat_at` (a directory), `tree/f` asked by
   path (a directory) — writes a marker and waits; the test (a second process) moves `d`, `e`
   and `f` aside and puts a link to a directory outside the tree in each place, then says go.
   The handle walk stays inside: `x` removed through the held `d` is the moved `d`'s, and the
   open of `e` (`O_DIRECTORY|O_NOFOLLOW`, relative to `tree`'s handle) is refused `invalid`, so
   the outside's files survive; the path walk in the same row removes `tree/f/z` by name and the
   outside's `z` is gone — the escape shown beside the walk that has none. Every machine but
   lupin.
3. **Depth.** 80 levels of 64-byte names (5200 bytes, past linux's 4096 and macOS's 1024) made
   by handle, the whole path's `fs_stat` and `fs_create_dir` refused (`io`, `ENAMETOOLONG`), and
   the tree removed level by level by handle.
4. **The refusals by name** (one fixture, each host's numbers): bit 32 on a file `invalid` 20;
   bits 32|16 on a link to a directory `invalid` 20; `fs_open_at` bit 16 on a final link `invalid`
   (40 / 62); bit 32 with a write mode, an absolute name, an empty name, a bad perm `invalid` 0;
   a file handle as `dir` `invalid` 20; a forged handle `io` 0; `fs_remove_at` on a directory
   linux `io` 21 / Darwin `denied` 1; `fs_remove_dir_at` on a file and on a link `invalid` 20,
   not empty `io`; `fs_create_dir_at` `exists`/`not_found`; `fs_symlink_at` empty target
   `invalid`, taken name `exists`; `fs_link_at` a directory `denied`, a dangling link followed
   `not_found`; `fs_set_mode_fd` on a pipe linux ok / Darwin `io` 22, a bad mode `invalid`.
5. **`fchmod`.** A file held open, another file renamed over its name: `fs_set_mode_fd` changes
   the held inode's mode (read back through `fs_fstat_record`) and the name's file keeps its
   own.
6. **Privilege, in throwaway rootless containers on hasu** (s272's roottest shape: an image
   from `ubuntu:24.04` with the programs copied in, `--network none`, each case as uid 4242 and
   as root inside the container): a hard link by `fs_link_at` to a file another user owns (the
   user: `denied` where the host protects hard links; root: ok); `fs_set_mode_fd` on a
   root-owned file through a read handle (the user `denied`, root ok); `fs_create_dir_at` and
   `fs_remove_at` under a root-owned 0755 directory and a sticky one (the user `denied`).
7. **The machines.** wolf_rt (`fs_at.rs`), clif (RT_SYMBOLS 262 -> 272), sema (ten
   signatures, prelude, comptime refusal), WIR, the checked machine, the driver's capabilities:
   every row green on checked, native and release; lupin 0.1.58 refuses each name ("does not
   resolve"), pinned by version.
8. **The counts.** The host table 167 -> 177 lines, prelude functions 178 -> 188, W0304 names
   211 -> 221 (each new name's row marks), RT_SYMBOLS 262 -> 272.
9. **The plant** `s277-plant-nofollow`: one hunk in `wolf_rt`'s `fs_at.rs`, `fs_open_at` drops
   `O_NOFOLLOW` before the call. Predicted red by name: `FAIL race_handle_walk` (the open of `e`
   follows the planted link out of the tree and its removal reaches the outside) and the
   refusal row's `FAIL open_at_nofollow`, on native and release; the checked machine is
   untouched by the plant.
