//! WIR function → CLIF translation (s28).
//!
//! The easy 80% is structural: WIR and CLIF are both phi-free
//! block-parameter SSA (same lineage), so blocks, params and branches
//! map 1:1 for scalars. The engineered 20%:
//!
//! - **Tokens erase to program order** — a `mem.rN`/`io`-typed value
//!   becomes nothing; instruction order within a block IS the memory
//!   order Cranelift sees.
//! - **Checked arithmetic** (X3) lowers to explicit overflow checks
//!   branching to per-kind trap blocks that CALL `__wolf_rt_trap`, so
//!   the trap identity survives to the process boundary (verdict
//!   `trap(overflow)`, not "SIGILL somewhere"). Narrow widths compute
//!   widened in i64 and range-check; i64 uses Cranelift's native
//!   `*_overflow` ops.
//! - **Aggregates and error unions** have no CLIF type: every
//!   `{…}`/`eu{…}` value is an explicit stack slot addressed via
//!   `wolf_backend::layout`. Aggregate-typed BLOCK PARAMETERS own a
//!   slot; every entering edge copies into it (loads before stores, so
//!   swapping edges can't clobber); `br` edges with aggregate args go
//!   through per-edge trampoline blocks.
//! - **Call boundaries execute `wolf_backend::abi` plans** (s29):
//!   classification lives THERE, this file is mechanical. Small
//!   aggregates scalarize into register units; MEMORY-class values go
//!   by pointer (wolf) or byval/sret (C membrane); `c.*` callees are
//!   unmangled libc imports under the SysV plan; error-union returns
//!   keep the discriminant in the first INTEGER return register.
//! - **Region ops** call the `wolf_rt` shims; `stack.alloc` is a real
//!   stack slot.

use std::collections::{HashMap, HashSet};

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{
    AbiParam, BlockArg, InstBuilder, JumpTableData, MemFlagsData, Signature, SourceLoc, StackSlot,
    StackSlotData, StackSlotKind, TrapCode, Value as CValue, types as ctypes,
};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module as _;
use cranelift_object::ObjectModule;

use wolf_backend::abi::{self, CTarget, Conv, ParamPass, RegClass, RetPass, Unit};
use wolf_backend::layout::{self, Layout};
use wolf_backend::{BackendError, DebugTy};
use wolf_wir::ir::{Aux, Block as WBlock, Function as WFunction, Module as WModule, SigId};
use wolf_wir::ops::{FloatCc, IntCc, Opcode, TrapKind};
use wolf_wir::types::{TypeData, TypeId};

/// The runtime symbols compiled code may reference (the s28/s29 symbol
/// contract with `wolf_rt`): name, param count, has-result.
/// `__wolf_rt_main_err` is the eu-main exit path (s29): tag id, name
/// length, and the tag's name bytes packed little-endian into four
/// i64 words (32-byte cap) — reports `error: <name>` on stdout and
/// exits 1, the documented D30 process behavior for a `main` that
/// returns an error value.
pub const RT_SYMBOLS: [(&str, usize, bool); 257] = [
    ("__wolf_rt_assert_buf", 2, false),
    ("__wolf_rt_assert_detail", 3, false),
    ("__wolf_rt_trap", 1, false),
    // s125: the sited trap — kind, then the site as immediates the
    // per-site cold block materializes: file path rodata (ptr, len)
    // and 1-based line/col. First stderr line byte-identical to
    // `__wolf_rt_trap`'s; the site is its own additive line.
    ("__wolf_rt_trap_at", 5, false),
    ("__wolf_rt_region_new", 0, true),
    ("__wolf_rt_region_alloc", 2, true),
    ("__wolf_rt_region_free", 1, false),
    ("__wolf_rt_region_freeze", 1, false),
    // s76: the ambient-region seam ([mem.region.create.3]). Lowering
    // brackets every construct that opens a region; `enter` returns the
    // previous handle and `leave` restores it on the X4 cleanup chain,
    // so container allocation (`wolf_rt::list`) lands in the region at
    // the allocation site instead of the process root.
    ("__wolf_rt_region_ambient_enter", 1, true),
    ("__wolf_rt_region_ambient_leave", 1, false),
    // s131 (#187): the region accounting queries — the ledger the
    // runtime keeps unconditionally, read into a register.
    ("__wolf_rt_region_bytes", 1, true),
    ("__wolf_rt_live_region_bytes", 0, true),
    // s132 (D68, #187): the creation-time byte budget — installed
    // right after region.new, compared on every charge; a breach is
    // trap(alloc-contract) at the allocating site.
    ("__wolf_rt_region_set_cap", 2, false),
    ("__wolf_rt_main_err", 6, false),
    // s239 (#750, `[os.net.reactor]`): the readiness set made before
    // `main` — the entry shim's first call in a program that names a
    // parking net call; a refusal is one stderr line and exit 71.
    ("__wolf_rt_reactor_start", 0, false),
    ("__wolf_rt_stack_start", 0, false),
    // s267 (#962, `[abi.target.main]`): the entry shim's first call, the C
    // entry's `(argc, argv, envp)` (`argc` crosses as i32) for the argv
    // readers, then the main thread's start-up `stack_start` made alone.
    ("__wolf_rt_main_start", 3, false),
    // s243 (#763, `[os.signal.hub]`): the signal hub made before `main`
    // in a program that names a call which makes it; a refusal is one
    // stderr line and exit 71.
    ("__wolf_rt_signal_start", 0, false),
    // s246 (#781, `[os.signal.wait.fd]`): the descriptor waits' shared
    // wake pipe, made before `main` after the hub in a program that names
    // `os_signal_wait_fd`; a refusal is one stderr line and exit 71.
    ("__wolf_rt_signal_wait_fd_start", 0, false),
    // The s31 print path: `print` lowers to per-segment writes — bytes
    // (ptr, len), decimal i64, `true`/`false` — flushed per call so no
    // buffered bytes outlive the C-shim `main` return.
    ("__wolf_rt_print_str", 2, false),
    ("__wolf_rt_print_i64", 1, false),
    ("__wolf_rt_print_bool", 1, false),
    // D43: one `print` statement is one atomic line. Lowering brackets
    // the segment calls; the runtime buffers between them and locks
    // the stream once.
    ("__wolf_rt_print_begin", 0, false),
    ("__wolf_rt_print_end", 1, false),
    // The s38 io path: stream-parameterized writes (stdout/stderr)
    // with the comptime-packed format spec as a trailing i64 immediate
    // (0 = none) — `wolf_rt::io`'s symbol contract.
    ("__wolf_rt_write_str", 4, false),
    ("__wolf_rt_write_i64", 3, false),
    ("__wolf_rt_write_bool", 3, false),
    ("__wolf_rt_write_f64", 3, false),
    // s121 (D58): the char hole — the scalar widened to i64, printed
    // as its UTF-8 encoding, never the code-point number.
    ("__wolf_rt_write_char", 3, false),
    // The s40 str/List/fs families (wolf_rt::{str,list,fs}): token
    // params in the WIR sigs erase here, so the counts below are the
    // REAL machine params — pointers/lens/codes as i64 (str values
    // travel as {ptr, len} pairs through caller out slots), and one
    // f64 for the float strbuf hole.
    ("__wolf_rt_strbuf_new", 0, true),
    ("__wolf_rt_strbuf_str", 4, false),
    ("__wolf_rt_strbuf_i64", 3, false),
    ("__wolf_rt_strbuf_bool", 3, false),
    ("__wolf_rt_strbuf_f64", 3, false),
    // s121 (D58): the char strbuf hole — same contract as write_char.
    ("__wolf_rt_strbuf_char", 3, false),
    ("__wolf_rt_strbuf_finish", 2, false),
    ("__wolf_rt_str_eq", 4, true),
    ("__wolf_rt_str_cmp", 4, true),
    ("__wolf_rt_str_get", 5, true),
    ("__wolf_rt_str_find", 5, true),
    ("__wolf_rt_str_probe", 5, true),
    ("__wolf_rt_str_count", 4, true),
    ("__wolf_rt_str_trim", 4, false),
    ("__wolf_rt_str_case", 4, false),
    ("__wolf_rt_str_strip", 6, true),
    ("__wolf_rt_str_repeat", 4, false),
    ("__wolf_rt_str_replace", 7, false),
    // s142 (wolf-lang#263): the parse — pair in, code out, the word
    // through the out slot.
    ("__wolf_rt_str_to_int", 3, true),
    ("__wolf_rt_str_split", 5, true),
    ("__wolf_rt_str_bytes", 2, true),
    // s120 (#17): code-point iteration — `(sp, sl) -> List[int]` of
    // Unicode scalar values, the materializing `chars()`.
    ("__wolf_rt_str_chars", 2, true),
    // s81 (#58): the validating byte source. `(hdr, out) -> code` —
    // the one entry that builds a `str` from arbitrary numbers, and a
    // rejection comes back as a code lowering turns into the `utf8`
    // row.
    ("__wolf_rt_str_from_utf8", 2, true),
    ("__wolf_rt_list_new", 1, true),
    ("__wolf_rt_list_push", 2, false),
    ("__wolf_rt_list_pop", 2, true),
    ("__wolf_rt_list_read", 3, true),
    ("__wolf_rt_list_write", 3, true),
    ("__wolf_rt_list_len", 1, true),
    ("__wolf_rt_list_clear", 1, false),
    // s255 (wolf-lang#844, `[type.list.reserve]`): the capacity hint —
    // the header and the count; grows the buffer in the list's birth
    // region, a no-op when the capacity is already there.
    ("__wolf_rt_list_reserve", 2, false),
    // s152 (`[type.map]`, wolf_rt::map): the keyed store — `new` takes
    // the entry layout (key kind, key size, value offset and size,
    // stride); `get`/`set` take the header and one entry-shaped slot
    // (the key read from it; the value written into it on a hit, or
    // read from it on a set); `pairs` mints a `List[(K, V)]`.
    ("__wolf_rt_map_new", 5, true),
    ("__wolf_rt_map_get", 2, true),
    ("__wolf_rt_map_set", 2, false),
    ("__wolf_rt_map_pairs", 1, true),
    ("__wolf_rt_map_clear", 1, false),
    // s165 (#344): the key erase — the get slot shape, a hit flag.
    ("__wolf_rt_map_remove", 2, true),
    // s165 (#384): `copy` of a List and of a Map — a fresh header and
    // buffer in the ambient region.
    ("__wolf_rt_list_copy", 1, true),
    ("__wolf_rt_map_copy", 1, true),
    // s228 (#663): `xs[lo..hi]` — the header and the two resolved
    // endpoints in, a fresh header out; one copy, not a push a slot.
    ("__wolf_rt_list_slice", 3, true),
    // s173 (#31/#268, `wolf_rt::pool`): the generational slot arena.
    // `new` takes the payload size; every other entry takes the
    // header and a packed handle word. `addr` is the odd one out and
    // the reason wolf-lang#31 lowers at all: it hands back the live
    // slot's ADDRESS so a place write stores through it, and 0 for a
    // stale handle, which lowering turns into trap(stale-handle).
    ("__wolf_rt_pool_new", 1, true),
    ("__wolf_rt_pool_reserve", 1, true),
    ("__wolf_rt_pool_addr", 2, true),
    ("__wolf_rt_pool_read", 3, true),
    ("__wolf_rt_pool_write", 3, true),
    ("__wolf_rt_pool_remove", 2, true),
    ("__wolf_rt_pool_alive", 2, true),
    ("__wolf_rt_pool_len", 1, true),
    ("__wolf_rt_pool_capacity", 1, true),
    ("__wolf_rt_pool_clear", 1, false),
    ("__wolf_rt_pool_next", 2, true),
    ("__wolf_rt_fs_read_text", 3, true),
    ("__wolf_rt_fs_write_text", 4, true),
    ("__wolf_rt_fs_open", 3, true),
    ("__wolf_rt_fs_read", 3, true),
    ("__wolf_rt_fs_write", 3, true),
    ("__wolf_rt_fs_close", 1, true),
    ("__wolf_rt_fs_remove", 2, true),
    ("__wolf_rt_fs_exists", 2, true),
    // s90 (#51/#52): bytes, directories, metadata, rename. List
    // results and single-word results ride caller out slots exactly
    // like str pairs do; `List[int]` arguments arrive as one header
    // pointer. `fs_open`'s third parameter is now a MODE, not a
    // create flag — same arity, wider meaning.
    ("__wolf_rt_fs_read_bytes", 3, true),
    ("__wolf_rt_fs_write_bytes", 3, true),
    ("__wolf_rt_fs_read_chunk", 3, true),
    ("__wolf_rt_fs_write_chunk", 2, true),
    ("__wolf_rt_fs_read_dir", 3, true),
    ("__wolf_rt_fs_create_dir", 3, true),
    ("__wolf_rt_fs_remove_dir", 3, true),
    ("__wolf_rt_fs_is", 3, true),
    ("__wolf_rt_fs_stat", 4, true),
    // s142 (#261): the stat on an open handle — fd in, code out, the
    // `List[int]` header through the out slot.
    ("__wolf_rt_fs_fstat", 2, true),
    // s218 (#625, #626): the full stat record by path (path pair,
    // follow, out), the link's target (path pair, out) and the typed
    // listing (path pair, out) — code out, the list through the slot.
    ("__wolf_rt_fs_stat_record", 4, true),
    ("__wolf_rt_fs_read_link", 3, true),
    ("__wolf_rt_fs_read_dir_entries", 3, true),
    // s230 (#346, #698, #699, #700, #702, #704): the changers — each
    // named for its builtin, so the freestanding target refuses each by
    // that name. A path pair and a mode; the open with a creation's
    // bits; two path pairs for a link; a path pair, four time words and
    // `follow`; a handle and a length; the mask in, the old mask out.
    ("__wolf_rt_fs_set_mode", 3, true),
    ("__wolf_rt_fs_open_perm", 4, true),
    ("__wolf_rt_fs_create_dir_mode", 3, true),
    ("__wolf_rt_fs_symlink", 4, true),
    ("__wolf_rt_fs_link", 4, true),
    ("__wolf_rt_fs_set_times", 7, true),
    ("__wolf_rt_fs_truncate", 2, true),
    // s236 (#701): the owner — a path pair, the two ids and `follow`;
    // a handle and the two ids.
    ("__wolf_rt_fs_set_owner", 5, true),
    ("__wolf_rt_fs_set_owner_fd", 3, true),
    // s248 (#796, #797, `[os.fs.sync]`, `[os.fs.realpath]`): a handle's
    // sync (a code out), a directory's (the path pair, a code out), and
    // the canonical path (the path pair, the str pair through the
    // 16-byte out slot).
    ("__wolf_rt_fs_sync", 1, true),
    ("__wolf_rt_fs_sync_dir", 2, true),
    ("__wolf_rt_fs_realpath", 3, true),
    // s254 (#820, #818; `[os.fs.access]`, `[os.fs.lock]`,
    // `[os.fs.fstat.record]`, `[os.fs.mkfifo]`, `[os.fs.nonblock]`): the
    // access query (the path pair, the mode), the lock (handle, kind) and
    // its release, the handle's record (handle, the list through the
    // 8-byte slot), the fifo (the path pair, the bits), the descriptor's
    // flag (handle, the bool widened) and the one write (handle, the list
    // header, the count through the 8-byte slot).
    ("__wolf_rt_fs_access", 3, true),
    ("__wolf_rt_fs_lock", 2, true),
    ("__wolf_rt_fs_unlock", 1, true),
    ("__wolf_rt_fs_fstat_record", 2, true),
    ("__wolf_rt_fs_mkfifo", 3, true),
    ("__wolf_rt_fs_set_nonblock", 2, true),
    ("__wolf_rt_fs_write_some", 3, true),
    ("__wolf_rt_os_umask", 1, true),
    // s199 (#426): the handle's offset — seek (fd, off, whence, out),
    // tell (fd, out), and the positional read (fd, off, max, out).
    ("__wolf_rt_fs_seek", 4, true),
    ("__wolf_rt_fs_tell", 2, true),
    ("__wolf_rt_fs_read_at", 4, true),
    // s200 (#417, `[os.fs.copy]`): the fused chunk copy (src, dst,
    // max, out) — code out, the count through the slot.
    ("__wolf_rt_fs_copy_chunk", 4, true),
    // s200 (#407, `[os.fs.error]`): the task's host code, and its text as
    // a str pair through the slot (code, out; no result).
    ("__wolf_rt_os_error", 0, true),
    ("__wolf_rt_os_error_text", 2, false),
    // s200 (#411, `[mem.list.bytes]`): the byte scan — (hdr, byte,
    // from) -> index or -1, and (hdr, byte) -> count.
    ("__wolf_rt_bytes_find", 3, true),
    ("__wolf_rt_bytes_count", 2, true),
    ("__wolf_rt_fs_rename", 4, true),
    ("__wolf_rt_read_line", 1, true),
    // The s106 net family (wolf_rt::net, #118's first crossing): fd
    // handles and codes as i64, addresses/payloads as str pairs, the
    // read result through a caller out slot — the fs discipline over
    // the NetTable that was waiting. `net_deadline` arms the s35
    // reactor budget (#45's builtin half).
    ("__wolf_rt_net_listen", 2, true),
    ("__wolf_rt_net_port", 1, true),
    ("__wolf_rt_net_accept", 1, true),
    ("__wolf_rt_net_connect", 2, true),
    ("__wolf_rt_net_read", 3, true),
    ("__wolf_rt_net_write", 3, true),
    // s115/#137: the byte twins — `List[int]` in/out, no UTF-8 gate,
    // so a wolf server can carry a binary body. `read_bytes` writes a
    // list header through the out slot (the `fs_read_bytes` shape);
    // `write_bytes` reads the caller's `List[int]` header.
    ("__wolf_rt_net_read_bytes", 3, true),
    ("__wolf_rt_net_write_bytes", 2, true),
    // s141/#254: the gathered write (a `List[List[byte]]` header in)
    // and the stream option (fd, on-as-i64) — `[os.net.writev]`,
    // `[os.net.nodelay]`.
    ("__wolf_rt_net_writev", 2, true),
    ("__wolf_rt_net_writev_head", 4, true),
    ("__wolf_rt_net_nodelay", 2, true),
    ("__wolf_rt_net_close", 1, true),
    ("__wolf_rt_net_deadline", 2, true),
    // s136/#227: the unix-domain pair — a path pair in, the fd (or the
    // negated code) back, `[os.net.unix]`.
    ("__wolf_rt_net_listen_unix", 2, true),
    ("__wolf_rt_net_connect_unix", 2, true),
    // s137/#234 + #235: listener options (addr pair, reuse as i64,
    // backlog) and the adopt half of descriptor inheritance (an OS
    // descriptor number in, the table handle or the negated code
    // back) — `[os.net.listen.opts]`, `[os.proc.inherit]`.
    ("__wolf_rt_net_listen_with", 4, true),
    ("__wolf_rt_net_adopt_listener", 1, true),
    // s137/#127: readiness over a SET — the handle list header and a
    // deadline in, the ready subset minted through the caller out
    // slot, `[os.net.wait]`.
    ("__wolf_rt_net_wait", 3, true),
    // The s107 json family (wolf_rt::json, #118's last crossing):
    // pure query kernels — two str pairs in, a code back, the
    // rendered node through a caller out slot; `json_len` rides the
    // handle convention (the count >= 0, or the negated code).
    ("__wolf_rt_json_valid", 2, true),
    ("__wolf_rt_json_get", 5, true),
    ("__wolf_rt_json_type", 5, true),
    ("__wolf_rt_json_len", 4, true),
    // The s40 os/env/time families (wolf_rt::{os,time}): same shape
    // discipline — pointers/lens/codes as i64, str results through
    // caller out slots, list results as header pointers. `os_exit`
    // never returns (the lowering seals the block with a residual
    // trap edge).
    ("__wolf_rt_env_arg0", 1, false),
    ("__wolf_rt_env_args_bytes", 0, true),
    ("__wolf_rt_env_args", 0, true),
    ("__wolf_rt_env_get", 3, true),
    ("__wolf_rt_env_set", 4, true),
    // s225 (#534, `[os.env.unset]`): the name's str pair, 0 or 1 back.
    ("__wolf_rt_env_unset", 2, true),
    ("__wolf_rt_env_vars", 0, true),
    ("__wolf_rt_os_cwd", 1, true),
    // s90 (#69): the running executable's path — the rig that spawns
    // ITSELF as its child.
    ("__wolf_rt_os_exe", 1, true),
    ("__wolf_rt_os_exit", 1, false),
    // s107 (#118): the process trio over the ChildTable. `os_spawn`
    // reads the caller's `List[str]` header (one pointer in, the
    // handle >= 0 or the negated code back); `os_wait` writes the
    // exit code through the out word and its 0-code REAPS.
    ("__wolf_rt_os_spawn", 1, true),
    // s137 (#235): the spawn with an inherit set — the exe as a str
    // pair, the args `List[str]` header and the inherit `List[int]`
    // header (both read by the shim), the handle or the negated code.
    ("__wolf_rt_os_spawn_with", 4, true),
    // s215 (pelt's H2): the spawn with a descriptor map (the exe pair,
    // the args and map headers), the pipe (its two handles through a
    // 16-byte out slot), chdir (a str pair) and isatty (handle in, the
    // answer through an out word).
    ("__wolf_rt_os_spawn_fds", 4, true),
    ("__wolf_rt_os_pipe", 1, true),
    ("__wolf_rt_os_chdir", 2, true),
    ("__wolf_rt_os_isatty", 2, true),
    // s225 (#534, `[os.proc.exec]`): the exe pair and the argv, env and
    // map headers; returns only on a failed exec, with the row's code.
    ("__wolf_rt_os_exec", 5, true),
    // s137/#233: the schedulable core count through a caller out
    // word, the `os_cwd` shape without the string — `[os.cpus]`.
    ("__wolf_rt_os_cpus", 1, true),
    ("__wolf_rt_os_wait", 2, true),
    ("__wolf_rt_os_kill", 1, true),
    // signal RECEPTION (s114, #126): set in, code/meaning out — plain
    // i64 in/out, so `rt_ref` needs no special-casing.
    ("__wolf_rt_os_signal_listen", 1, true),
    ("__wolf_rt_os_signal_wait", 1, true),
    ("__wolf_rt_os_signal_raise", 1, true),
    // s219 (#622): the dispositions and the poll (set in, code or
    // meaning out), the job spawn (exe pair, args and map headers, group,
    // tty, defaults), the child's pid, its status through an out word,
    // this process's group, and the terminal's foreground and mode.
    ("__wolf_rt_os_signal_ignore", 1, true),
    ("__wolf_rt_os_signal_default", 1, true),
    // s246 (#783, #784): one meaning in the sending shape ignored or set
    // back to its default, and the end of this process by a signal.
    ("__wolf_rt_os_signal_ignore_one", 1, true),
    ("__wolf_rt_os_signal_default_one", 1, true),
    ("__wolf_rt_os_exit_signal", 1, true),
    ("__wolf_rt_os_signal_poll", 1, true),
    ("__wolf_rt_os_spawn_job", 7, true),
    ("__wolf_rt_os_proc_pid", 1, true),
    ("__wolf_rt_os_wait_status", 2, true),
    ("__wolf_rt_os_pgid", 0, true),
    ("__wolf_rt_os_term_foreground", 1, true),
    ("__wolf_rt_os_term_set_foreground", 2, true),
    ("__wolf_rt_os_term_mode", 1, true),
    ("__wolf_rt_os_term_set_mode", 2, true),
    // s248 (#794, `[os.term.restore]`): the registration — the handle,
    // the mode and the bytes' `List[byte]` header in, a code out — and the
    // restore the entry shim calls on a return from `main`.
    ("__wolf_rt_os_term_restore_at_exit", 3, true),
    ("__wolf_rt_term_restore_exit", 0, false),
    // s232 (#679, #681, #691, #141): the ignored set (none in, set or
    // code out), the send (pid and meaning in, code out), a meaning's
    // number, this process's id and its parent's.
    ("__wolf_rt_os_signal_ignored", 0, true),
    ("__wolf_rt_os_signal_send", 2, true),
    ("__wolf_rt_os_signal_number", 1, true),
    // s241 (#739, #740, #741, #762): the group and all sends, this
    // process into a group, the host's last number, its real-time range
    // (a 16-byte out slot) and a signal's words (a str through a 16-byte
    // out slot).
    ("__wolf_rt_os_signal_send_group", 2, true),
    ("__wolf_rt_os_signal_send_all", 1, true),
    ("__wolf_rt_os_set_pgid", 1, true),
    ("__wolf_rt_os_signal_last", 0, true),
    ("__wolf_rt_os_signal_realtime", 1, true),
    ("__wolf_rt_os_signal_text", 2, true),
    // s242 (#680): the poll of the children — the (handle, status) pair
    // through a 16-byte out slot.
    ("__wolf_rt_os_wait_poll", 1, true),
    // s246 (#785; ruling #88 R3 D): the wait and the poll that report a
    // core, the released two's shapes.
    ("__wolf_rt_os_wait_status_core", 2, true),
    ("__wolf_rt_os_wait_poll_core", 1, true),
    // s247 (ruling #90, `[os.proc.fork]`): the fork — the (pid, handle)
    // pair through a 16-byte out slot, `os_wait_poll`'s shape.
    ("__wolf_rt_os_fork", 1, true),
    ("__wolf_rt_os_pid", 0, true),
    ("__wolf_rt_os_ppid", 0, true),
    // s234 (#719): the window's size (handle in, the pair through a
    // 16-byte out slot), the special characters (handle in, a minted
    // List[int] header through the out slot), and the wait on a
    // descriptor and a signal set (handle and set in, meaning or 0 or a
    // negated code out).
    ("__wolf_rt_os_term_size", 2, true),
    ("__wolf_rt_os_term_chars", 2, true),
    ("__wolf_rt_os_signal_wait_fd", 2, true),
    // s248 (#795, `[os.signal.wait.fds]`): the handles' `List[int]`
    // header, the set and the deadline in, the (index, meaning) pair
    // through a 16-byte out slot.
    ("__wolf_rt_os_signal_wait_fds", 4, true),
    // s236 (#633, #742): the four ids (none in, the id or a negated
    // code out), the supplementary groups (a minted List[int] header
    // through the out slot), and the drop (the two ids and the groups'
    // header in, a code out).
    ("__wolf_rt_os_uid", 0, true),
    ("__wolf_rt_os_euid", 0, true),
    ("__wolf_rt_os_gid", 0, true),
    ("__wolf_rt_os_egid", 0, true),
    ("__wolf_rt_os_groups", 1, true),
    ("__wolf_rt_os_set_ids", 3, true),
    // The OS random source (s118, #143): count in, a minted List[int]
    // header through the out slot; nonzero rc is trap(assert) in
    // lowering ([os.random.trap]) — the one os shim with no row.
    ("__wolf_rt_os_random", 2, true),
    ("__wolf_rt_time_now_ms", 0, true),
    ("__wolf_rt_time_unix_ms", 0, true),
    // s248 (#798, `[os.clock.ns]`): the monotonic clock in nanoseconds,
    // `time_now_ms`'s shape.
    ("__wolf_rt_os_clock_ns", 0, true),
    // s253 (#819, `[os.uname]`): the OS and the architecture, each a str
    // pair through a slot, no row (`os_error_text`'s shape); (#811,
    // `[os.proc.core]`) the core limit's pair through a slot, its set and
    // the end with no core, each a code; (#815, `[os.proc.adopt]`) an
    // inherited descriptor's handle, or the negated code (`fs_open`'s).
    ("__wolf_rt_os_name", 1, false),
    ("__wolf_rt_os_arch", 1, false),
    ("__wolf_rt_os_core_limit", 1, true),
    ("__wolf_rt_os_set_core_limit", 2, true),
    ("__wolf_rt_os_core_off", 0, true),
    ("__wolf_rt_fs_adopt", 1, true),
    ("__wolf_rt_time_sleep_ms", 1, false),
    // The s73 conc families (wolf_rt::task, the frozen s32–s36 ABI +
    // the conc_abi additions): handles/ids/words as i64|ptr; the
    // narrow returns (i32 statuses, the i8 killed poll) and select's
    // i8 `has_else` ride `rt_ref`'s per-symbol exceptions. Env/slot
    // token params erase — counts are machine params.
    ("__wolf_rt_scope_new", 2, true),
    ("__wolf_rt_scope_spawn", 5, false),
    // s179 (#431): a task env copied into its scope when the spawning
    // frame did not open the scope (scope, env, len) -> the copy.
    ("__wolf_rt_scope_env_copy", 3, true),
    ("__wolf_rt_scope_join_free", 1, true),
    // s166: `xs.par(f)` — `[conc.task.par]`'s chunked map (list, shim,
    // record, result element size, out slot) -> failure tag.
    ("__wolf_rt_par_map", 5, true),
    ("__wolf_rt_task_killed", 0, true),
    // s143 (#268): a proc body's `int` result, stashed by its entry
    // shim for `[conc.proc.exit]`'s `normal(value)`.
    ("__wolf_rt_task_value", 1, false),
    // s150 (#300): a capturing closure's callable record, bump-
    // allocated in the thread's ambient region — size in, the record
    // pointer back (`[abi.native.closure]`).
    ("__wolf_rt_closure_alloc", 1, true),
    ("__wolf_rt_chan_new", 1, true),
    // s150 (`[conc.chan.payload]`): a channel of payload boxes, and
    // the boxes — `box_new` fills one from a caller slot (size, src)
    // and answers the word; `box_take` empties one into a caller slot
    // (word, size, dst) and frees it.
    ("__wolf_rt_chan_new_boxed", 1, true),
    ("__wolf_rt_box_new", 2, true),
    ("__wolf_rt_box_take", 3, false),
    ("__wolf_rt_chan_send", 2, true),
    ("__wolf_rt_chan_send_region", 2, true),
    ("__wolf_rt_chan_recv", 2, true),
    ("__wolf_rt_chan_close", 1, false),
    ("__wolf_rt_select", 6, true),
    ("__wolf_rt_sync_new", 1, true),
    ("__wolf_rt_sync_get", 1, true),
    ("__wolf_rt_sync_set", 2, false),
    ("__wolf_rt_when_acquire", 2, true),
    ("__wolf_rt_when_release", 2, false),
    ("__wolf_rt_region_adopt", 1, true),
    ("__wolf_rt_proc_spawn_outcome", 5, true),
    ("__wolf_rt_proc_monitor", 1, true),
    ("__wolf_rt_proc_join", 1, true),
    ("__wolf_rt_proc_kill", 1, false),
    ("__wolf_rt_proc_cancel", 1, false),
    ("__wolf_rt_proc_link", 2, false),
];

/// How one WIR parameter crosses the call boundary (the mechanical
/// execution of [`wolf_backend::abi::ParamPass`] — classification
/// lives THERE; this enum only adds what plan execution needs).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Slot {
    /// Passed directly as one CLIF value.
    Direct(cranelift_codegen::ir::Type),
    /// Aggregate scalarized into ≤ 2 eightbyte register units.
    Split(Vec<Unit>),
    /// Aggregate by pointer (wolf-native MEMORY class).
    Ref,
    /// Aggregate byval on the stack (C MEMORY class, SysV x86-64),
    /// by size.
    CStruct(u32),
    /// Aggregate INDIRECT (C MEMORY class, Apple-arm64 — AAPCS64
    /// B.4): a plain pointer to a caller-owned copy of `size` bytes.
    /// The callee reads through the pointer; the call site copies the
    /// value to a fresh slot first (the callee owns the copy).
    CIndirect(u32),
    /// Effect token: erased, crosses as nothing.
    Token,
}

/// How one WIR result crosses the call boundary.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum RSlot {
    /// Returned directly as one CLIF value.
    Direct(cranelift_codegen::ir::Type),
    /// Aggregate in ≤ 2 return registers (eu: tag is unit 0).
    Split(Vec<Unit>),
    /// Caller-allocated out-slot of `size` bytes. Wolf: plain pointer
    /// param (trailing), plus an `i64` discriminant return when
    /// `disc_in_reg` (big error unions). C: a true psABI sret param
    /// (first; Cranelift returns the pointer in `%rax` itself).
    Sret {
        disc_in_reg: bool,
        c: bool,
        size: u32,
    },
    /// Effect token: erased.
    Token,
}

/// Where a call lands: a resolved `FuncRef` (direct) or a callee
/// VALUE through an imported signature (`call.ind`, s97).
enum CallTarget {
    Direct(cranelift_codegen::ir::FuncRef),
    Indirect(cranelift_codegen::ir::SigRef, CValue),
}

pub(crate) struct SigInfo {
    pub params: Vec<Slot>,
    pub rets: Vec<RSlot>,
    pub clif: Signature,
    /// The out-slot pointer params sit FIRST in the CLIF signature
    /// (C convention) rather than trailing (wolf-native).
    pub sret_first: bool,
}

/// kw02 (`[abi.c.export]`): an export's narrow result widened by the
/// callee, as its C callers on Apple arm64 expect (`Function::ret_ext`).
/// Applied to the signature the definition is DECLARED and DEFINED
/// with, so the two agree.
pub(crate) fn with_ret_ext(mut si: SigInfo, ext: wolf_wir::ir::Mode) -> SigInfo {
    if let (Some(RSlot::Direct(_)), 1) = (si.rets.first(), si.clif.returns.len()) {
        let r = si.clif.returns[0];
        si.clif.returns[0] = match ext {
            wolf_wir::ir::Mode::Sext => r.sext(),
            wolf_wir::ir::Mode::Zext => r.uext(),
            _ => r,
        };
    }
    si
}

fn scalar_clif_ty(m: &WModule, ty: TypeId) -> Option<cranelift_codegen::ir::Type> {
    Some(match m.types.get(ty) {
        TypeData::I8 | TypeData::Bool => ctypes::I8,
        TypeData::I16 => ctypes::I16,
        TypeData::I32 => ctypes::I32,
        TypeData::I64 | TypeData::Ptr => ctypes::I64,
        TypeData::F32 => ctypes::F32,
        TypeData::F64 => ctypes::F64,
        TypeData::Mem(_)
        | TypeData::Io
        | TypeData::Agg(_)
        | TypeData::Eu { .. }
        | TypeData::Union(_) => return None,
    })
}

/// Aggregates, error unions and union slots (s257): every value with
/// no CLIF type, addressed through a stack slot.
fn is_agg(m: &WModule, ty: TypeId) -> bool {
    matches!(
        m.types.get(ty),
        TypeData::Agg(_) | TypeData::Eu { .. } | TypeData::Union(_)
    )
}

/// The CLIF value type carrying one eightbyte unit: INTEGER units ride
/// `i64` (partial eightbytes are chunk-assembled), SSE units ride
/// `f32`/`f64` by covered width.
fn unit_clif_ty(u: &Unit) -> cranelift_codegen::ir::Type {
    match u.class {
        RegClass::Int => ctypes::I64,
        RegClass::Sse => {
            if u.bytes <= 4 {
                ctypes::F32
            } else {
                ctypes::F64
            }
        }
    }
}

/// The [`CTarget`] a calling convention executes against — how this
/// backend names its host to the abi module's per-target seam (s59).
pub(crate) fn c_target(cc: CallConv) -> CTarget {
    match cc {
        CallConv::AppleAarch64 => CTarget::AppleArm64,
        CallConv::WindowsFastcall => CTarget::Win64,
        _ => CTarget::SysvX64,
    }
}

/// Build the CLIF signature + slot map for one WIR signature under one
/// convention, executing the shared plan from [`wolf_backend::abi`]
/// (where ALL classification lives — this function is mechanical).
/// `Err` when the plan records a refusal-by-shape (s59: the
/// register-exhaustion corners the target's C contract reaches but
/// this backend cannot express) — loud, never silently wrong.
pub(crate) fn sig_info(
    m: &WModule,
    sig: SigId,
    conv: Conv,
    cc: CallConv,
) -> Result<SigInfo, BackendError> {
    let data = &m.sigs[sig];
    let plan = abi::plan_sig(&m.types, data, conv, c_target(cc));
    if let Some(why) = plan.refusals.first() {
        return Err(BackendError::Unsupported(format!(
            "C-membrane signature shape: {why}"
        )));
    }
    let mut clif = Signature::new(cc);
    let sret_first = conv == Conv::C;
    let mut rets = Vec::with_capacity(data.results.len());
    for (pass, &r) in plan.rets.iter().zip(data.results.iter()) {
        match pass {
            RetPass::Token => rets.push(RSlot::Token),
            RetPass::Direct(ty) => {
                let t = scalar_clif_ty(m, *ty).expect("scalar result");
                clif.returns.push(AbiParam::new(t));
                rets.push(RSlot::Direct(t));
            }
            RetPass::Split(units) => {
                for u in units {
                    clif.returns.push(AbiParam::new(unit_clif_ty(u)));
                }
                rets.push(RSlot::Split(units.clone()));
            }
            RetPass::Sret { disc_in_reg } => {
                let size = layout::layout_of(&m.types, r).expect("sized result").size;
                if sret_first {
                    // psABI sret: pointer first (%rdi); Cranelift
                    // returns it in %rax itself — no explicit return.
                    clif.params.push(AbiParam::special(
                        ctypes::I64,
                        cranelift_codegen::ir::ArgumentPurpose::StructReturn,
                    ));
                } else if *disc_in_reg {
                    // Big error union: the discriminant STILL rides
                    // the first INTEGER return register ([abi.err]).
                    clif.returns.push(AbiParam::new(ctypes::I64));
                }
                rets.push(RSlot::Sret {
                    disc_in_reg: *disc_in_reg,
                    c: sret_first,
                    size,
                });
            }
        }
    }
    let mut params = Vec::with_capacity(data.params.len());
    for (pass, p) in plan.params.iter().zip(data.params.iter()) {
        match pass {
            ParamPass::Token => params.push(Slot::Token),
            ParamPass::Direct(ty) => {
                let t = scalar_clif_ty(m, *ty).expect("scalar param");
                // kw02: a narrow C argument is extended by the caller
                // (`Mode::Sext`/`Zext`, set only on `c.*` imports).
                let ap = match p.mode {
                    wolf_wir::ir::Mode::Sext => AbiParam::new(t).sext(),
                    wolf_wir::ir::Mode::Zext => AbiParam::new(t).uext(),
                    _ => AbiParam::new(t),
                };
                clif.params.push(ap);
                params.push(Slot::Direct(t));
            }
            ParamPass::Split(units) => {
                for u in units {
                    clif.params.push(AbiParam::new(unit_clif_ty(u)));
                }
                params.push(Slot::Split(units.clone()));
            }
            ParamPass::Memory => match conv {
                Conv::Wolf => {
                    clif.params.push(AbiParam::new(ctypes::I64));
                    params.push(Slot::Ref);
                }
                Conv::C => {
                    let size = layout::layout_of(&m.types, p.ty).expect("sized param").size;
                    match c_target(cc) {
                        CTarget::SysvX64 => {
                            // psABI MEMORY: byval on the stack —
                            // cranelift's StructArgument does the copy.
                            clif.params.push(AbiParam::special(
                                ctypes::I64,
                                cranelift_codegen::ir::ArgumentPurpose::StructArgument(
                                    size.next_multiple_of(8),
                                ),
                            ));
                            params.push(Slot::CStruct(size));
                        }
                        CTarget::AppleArm64 | CTarget::Win64 => {
                            // AAPCS64 B.4: indirect — a plain pointer
                            // parameter (cranelift's arm64 has no
                            // StructArgument; C does not want one).
                            // Win64's > 8-byte rule is the same
                            // pointer-to-copy shape; its plan refuses
                            // every aggregate at s60a, so this arm is
                            // the lowering for the day the refusal
                            // lifts (≤ 8-byte composites will need
                            // their bits-in-register form first).
                            clif.params.push(AbiParam::new(ctypes::I64));
                            params.push(Slot::CIndirect(size));
                        }
                    }
                }
            },
        }
    }
    // Wolf-native out-slot pointers trail the declared params, in
    // result order (a PLAIN pointer param — no C sret obligations).
    if !sret_first {
        for r in &rets {
            if matches!(r, RSlot::Sret { .. }) {
                clif.params.push(AbiParam::new(ctypes::I64));
            }
        }
    }
    Ok(SigInfo {
        params,
        rets,
        clif,
        sret_first,
    })
}

/// A WIR load/store's memory flags: `trusted` (no trap, aligned)
/// unless the access carries an alignment below its natural one (kw08,
/// a packed field, `[abi.layout.packed]`), where the `aligned` promise
/// is dropped so Cranelift never assumes it.
fn access_flags(aux: wolf_wir::ir::Aux) -> MemFlagsData {
    match aux {
        wolf_wir::ir::Aux::Int(_) => MemFlagsData::new().with_notrap(),
        _ => MemFlagsData::trusted(),
    }
}

/// How a WIR value exists in CLIF.
#[derive(Clone, Copy, Debug)]
enum Repr {
    /// One CLIF value (ints, floats, bools as i8, ptrs as i64).
    Scalar(CValue),
    /// An aggregate/error-union: the address of its bytes.
    Addr(CValue),
    /// An effect token: nothing.
    Token,
}

struct Tx<'a, 'b> {
    b: FunctionBuilder<'b>,
    m: &'a WModule,
    f: &'a WFunction,
    dense_matches: HashMap<wolf_wir::ir::Inst, crate::dense::DenseMatch>,
    hoisted_constants: HashSet<wolf_wir::ir::Value>,
    /// The convention THIS function is defined under (wolf-native for
    /// module functions; C for `export`ed membranes).
    conv: Conv,
    om: &'a mut ObjectModule,
    funcs: &'a HashMap<String, FuncEntry>,
    rt: &'a mut HashMap<&'static str, cranelift_module::FuncId>,
    /// C membrane imports declared so far (`c.*` callee → func id).
    imports: &'a mut HashMap<String, cranelift_module::FuncId>,
    variadic_shims: &'a mut Vec<VariadicShim>,
    va_context: Option<CValue>,
    /// Module data blobs defined so far (`data.addr` targets, s31):
    /// WIR data index → object data id. Lazily defined on first
    /// reference, so an object carries exactly the data it uses.
    data_ids: &'a mut HashMap<u32, cranelift_module::DataId>,
    /// Source files trap sites resolve against (s125) — FileId index →
    /// path + line starts; empty means every trap stays site-less.
    site_files: &'a HashMap<u32, wolf_backend::dwarf::SourceFile>,
    /// Per-file rodata path symbols defined so far (module-level, like
    /// `data_ids`): FileId index → object data id for the path bytes.
    site_file_data: &'a mut HashMap<u32, cranelift_module::DataId>,
    /// The freestanding target (kw04, `[abi.target.none]`): every trap
    /// reports through the one hook `wolf_trap`, and nothing else of
    /// the runtime may be referenced.
    freestanding: bool,
    /// The srcspan `lo` of the instruction being translated (s125):
    /// what a trap check emitted from it names as its site.
    cur_span_lo: Option<u32>,
    blocks: HashMap<WBlock, cranelift_codegen::ir::Block>,
    vals: HashMap<wolf_wir::ir::Value, Repr>,
    read_lends: HashMap<wolf_wir::ir::Value, wolf_wir::ir::Value>,
    /// Slots owned by aggregate-typed block params of non-entry blocks.
    param_slots: HashMap<(WBlock, usize), cranelift_codegen::ir::StackSlot>,
    /// sret addresses (entry params), in aggregate-result order.
    sret: Vec<CValue>,
    /// Lazily created cold trap blocks, one per (kind, site) — the
    /// s125 trade: sites split the old per-kind sharing so the cold
    /// body can materialize `  at file:line:col`; checks from the same
    /// statement still share ((kind, line, col) dedup). Site-less
    /// checks keep the shared per-kind block.
    trap_blocks: HashMap<(i32, Option<(u64, u64)>), cranelift_codegen::ir::Block>,
    /// FuncRefs imported into this function, by callee name, with the
    /// convention their calls lower under.
    fref_cache: HashMap<String, (cranelift_codegen::ir::FuncRef, Conv)>,
    /// s30 debug: WIR value → indices into `debug_slots` (a value may
    /// carry several names — GVN dedup; a name may have many values —
    /// rebindings, all stored to ONE slot).
    debug_names: HashMap<wolf_wir::ir::Value, Vec<usize>>,
    /// s30 debug: one frame slot per named scalar binding —
    /// (name, scalar shape, slot, is-param). The debug tier forces
    /// named locals addressable (s28 decision) so `DW_OP_fbreg`
    /// locations are whole-function truths.
    debug_slots: Vec<(String, DebugTy, StackSlot, bool)>,
}

/// One declared function the translator can call.
pub(crate) struct FuncEntry {
    pub fid: cranelift_module::FuncId,
    pub sig: SigId,
    pub core: Option<cranelift_module::FuncId>,
    pub symbol: String,
}

/// Code inserted after CLIF object finalization. Imported placeholders let
/// ordinary CLIF calls refer to these entries without a fake CLIF body.
pub(crate) struct VariadicShim {
    pub fid: cranelift_module::FuncId,
    pub target: String,
    pub code: abi::variadic::x64::Code,
    pub linkage: wolf_backend::Linkage,
}

pub(crate) fn check_variadic_target(om: &ObjectModule) -> Result<(), BackendError> {
    let triple = om.isa().triple();
    if triple.architecture != target_lexicon::Architecture::X86_64
        || !matches!(
            triple.operating_system,
            target_lexicon::OperatingSystem::Linux | target_lexicon::OperatingSystem::None_
        )
    {
        return Err(nyi(format!(
            "C variable arguments are unsupported on {triple}; require x86-64 Linux or x86_64-unknown-none (SysV AMD64)"
        )));
    }
    Ok(())
}

fn ice(msg: impl Into<String>) -> BackendError {
    BackendError::Internal(msg.into())
}

fn nyi(msg: impl Into<String>) -> BackendError {
    BackendError::Unsupported(msg.into())
}

/// Translate one function. Returns the named-variable frame slots the
/// debug tier spilled (s30): the caller maps them to frame-pointer-
/// relative offsets once the frame layout is known.
#[allow(clippy::too_many_arguments)]
pub(crate) fn translate_function(
    b: FunctionBuilder<'_>,
    m: &WModule,
    f: &WFunction,
    conv: Conv,
    si: &SigInfo,
    om: &mut ObjectModule,
    funcs: &HashMap<String, FuncEntry>,
    rt: &mut HashMap<&'static str, cranelift_module::FuncId>,
    imports: &mut HashMap<String, cranelift_module::FuncId>,
    variadic_shims: &mut Vec<VariadicShim>,
    data_ids: &mut HashMap<u32, cranelift_module::DataId>,
    site_files: &HashMap<u32, wolf_backend::dwarf::SourceFile>,
    site_file_data: &mut HashMap<u32, cranelift_module::DataId>,
    freestanding: bool,
) -> Result<Vec<(String, DebugTy, StackSlot, bool)>, BackendError> {
    let mut tx = Tx {
        b,
        m,
        f,
        dense_matches: crate::dense::find(m, f),
        hoisted_constants: HashSet::new(),
        conv,
        om,
        funcs,
        rt,
        imports,
        variadic_shims,
        va_context: None,
        data_ids,
        site_files,
        site_file_data,
        freestanding,
        cur_span_lo: None,
        blocks: HashMap::new(),
        vals: HashMap::new(),
        read_lends: wolf_backend::read_lend::borrowed_aggregates(m, f),
        param_slots: HashMap::new(),
        sret: Vec::new(),
        trap_blocks: HashMap::new(),
        fref_cache: HashMap::new(),
        debug_names: HashMap::new(),
        debug_slots: Vec::new(),
    };
    tx.prepare_debug_vars();
    tx.run(si)?;
    let cfg = tx.om.isa().frontend_config();
    tx.b.finalize(cfg);
    Ok(tx.debug_slots)
}

impl<'a, 'b> Tx<'a, 'b> {
    /// Build the named-binding tables (s30): one frame slot per named
    /// SCALAR binding (aggregates already live in addressable slots;
    /// their DIEs are s31+), shared by every rebinding of the name.
    /// Shadowing with a DIFFERENT scalar shape keeps the first
    /// binding's slot and shape — later shadows of another type are
    /// not stored (v0 honesty: one DIE per name).
    fn prepare_debug_vars(&mut self) {
        let mut by_name: HashMap<&str, usize> = HashMap::new();
        for dv in &self.f.debug_vars {
            let ty = self.f.value_ty(dv.val);
            let Some(dty) = debug_ty(self.m, ty) else {
                continue; // aggregate/token-shaped: no scalar slot at v0
            };
            let idx = match by_name.get(dv.name.as_str()) {
                Some(&i) => {
                    if self.debug_slots[i].1 != dty {
                        continue; // shadow of a different shape
                    }
                    if dv.param {
                        self.debug_slots[i].3 = true;
                    }
                    i
                }
                None => {
                    let slot = self.b.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    self.debug_slots
                        .push((dv.name.clone(), dty, slot, dv.param));
                    by_name.insert(dv.name.as_str(), self.debug_slots.len() - 1);
                    self.debug_slots.len() - 1
                }
            };
            self.debug_names.entry(dv.val).or_default().push(idx);
        }
    }

    /// If `wv` is a named scalar binding, store its value into the
    /// name's frame slot at the current point (the definition site) —
    /// the debug tier's forced spill.
    fn debug_spill(&mut self, wv: wolf_wir::ir::Value) {
        let Some(idxs) = self.debug_names.get(&wv).cloned() else {
            return;
        };
        let Some(&Repr::Scalar(cv)) = self.vals.get(&wv) else {
            return;
        };
        for i in idxs {
            let slot = self.debug_slots[i].2;
            let addr = self.b.ins().stack_addr(ctypes::I64, slot, 0);
            self.b.ins().store(MemFlagsData::trusted(), cv, addr, 0);
        }
    }

    fn run(&mut self, si: &SigInfo) -> Result<(), BackendError> {
        let Some(entry) = self.f.entry() else {
            return Err(ice("function has no entry block"));
        };
        // The WALK order (s74, #72): reverse postorder, not `layout`.
        // `layout` is a printing order and lowering may append a block
        // that USES a value before the block that defines it (a
        // `select` arm inside a loop does exactly that — the latch is
        // created before the arm body). This translator is one forward
        // pass, so it needs defs first, which RPO guarantees.
        let order = wolf_wir::block_order(self.f);
        // 1. Create every CLIF block; give non-entry blocks their
        // scalar params (aggregate params own slots instead).
        for &wb in &order {
            let cb = self.b.create_block();
            self.blocks.insert(wb, cb);
        }
        let entry_cb = self.blocks[&entry];
        // Entry: CLIF params mirror the signature.
        self.b.switch_to_block(entry_cb);
        for abi in &si.clif.params {
            self.b.append_block_param(entry_cb, abi.value_type);
        }
        let cparams: Vec<CValue> = self.b.block_params(entry_cb).to_vec();
        let n_sret = si
            .rets
            .iter()
            .filter(|r| matches!(r, RSlot::Sret { .. }))
            .count();
        // C convention: out-slot pointers sit FIRST; wolf trails them.
        let mut next = if si.sret_first { n_sret } else { 0 };
        let wparams = self.f.block_params(entry);
        if wparams.len() != si.params.len() {
            return Err(ice("entry params do not match the signature"));
        }
        for (i, &wv) in wparams.iter().enumerate() {
            match &si.params[i] {
                Slot::Token => {
                    self.vals.insert(wv, Repr::Token);
                }
                Slot::Direct(_) => {
                    self.vals.insert(wv, Repr::Scalar(cparams[next]));
                    next += 1;
                }
                Slot::Split(units) => {
                    // Materialize the register units into a local slot
                    // (rounded to whole eightbytes — the store-side
                    // invariant of [`Tx::store_unit`]).
                    let units = units.clone();
                    let addr = self.units_slot_addr(&units);
                    for u in &units {
                        let v = cparams[next];
                        next += 1;
                        self.store_unit(addr, u, v);
                    }
                    self.vals.insert(wv, Repr::Addr(addr));
                }
                Slot::Ref | Slot::CStruct(_) | Slot::CIndirect(_) => {
                    self.vals.insert(wv, Repr::Addr(cparams[next]));
                    next += 1;
                }
            }
        }
        if self.m.sigs[self.f.sig].variadic.is_some() {
            check_variadic_target(self.om)?;
            self.va_context = cparams.last().copied();
        }
        let sret_base = if si.sret_first { 0 } else { next };
        for k in 0..n_sret {
            self.sret.push(cparams[sret_base + k]);
        }
        // Named parameters spill to their debug slots in the prologue
        // (s30): readable the moment a `break main`-style breakpoint
        // lands (prologue_end marks the first user row).
        for &wv in &wparams {
            self.debug_spill(wv);
        }
        // Non-entry blocks: scalar params become CLIF block params;
        // aggregate params own a stack slot filled by entering edges.
        for &wb in &order {
            if wb == entry {
                continue;
            }
            let cb = self.blocks[&wb];
            for (i, wv) in self.f.block_params(wb).into_iter().enumerate() {
                let ty = self.f.value_ty(wv);
                if self.m.types.is_token(ty) {
                    self.vals.insert(wv, Repr::Token);
                } else if is_agg(self.m, ty) {
                    let slot = self.new_slot(self.layout_of(ty)?);
                    self.param_slots.insert((wb, i), slot);
                    // The address is minted at block entry below.
                } else {
                    let cty = scalar_clif_ty(self.m, ty).expect("scalar");
                    let cv = self.b.append_block_param(cb, cty);
                    self.vals.insert(wv, Repr::Scalar(cv));
                }
            }
        }
        // Bypassed tests can define literals used by an arm or default.
        // Emit them in the entry so both dispatch and non-chain incoming
        // paths retain those definitions. Sort to keep object bytes stable.
        let mut constants: Vec<_> = self
            .dense_matches
            .values()
            .flat_map(|table| table.constants.iter().copied())
            .collect();
        constants.sort_by_key(|&(v, _)| v);
        constants.dedup_by_key(|&mut (v, _)| v);
        for (wv, n) in constants {
            let ty = scalar_clif_ty(self.m, self.f.value_ty(wv)).expect("integer constant");
            let cv = self.b.ins().iconst(ty, n);
            self.vals.insert(wv, Repr::Scalar(cv));
            self.hoisted_constants.insert(wv);
        }
        // 2. Translate every block. The entry is first in the walk order
        // and the builder is ALREADY positioned there (step 1 may have
        // emitted param-materialization stores into it — switching,
        // even to the same block, would trip the fill-before-switch
        // rule).
        for &wb in &order {
            let cb = self.blocks[&wb];
            if wb != entry {
                self.b.switch_to_block(cb);
            }
            // Aggregate block params: bind their slot addresses. Named
            // scalar params (merged rebindings, loop variables) re-
            // spill to their debug slots at block entry (s30).
            for (i, wv) in self.f.block_params(wb).into_iter().enumerate() {
                if let Some(&slot) = self.param_slots.get(&(wb, i)) {
                    let addr = self.b.ins().stack_addr(ctypes::I64, slot, 0);
                    self.vals.insert(wv, Repr::Addr(addr));
                }
                if wb != entry {
                    self.debug_spill(wv);
                }
            }
            for ii in 0..self.f.blocks[wb].insts.len() {
                let inst = self.f.blocks[wb].insts[ii];
                // s30: the WIR span rides into the machine buffer as a
                // Cranelift srcloc (bits = span lo; the byte-exact s07
                // chain ends in .debug_line).
                let sp = self.f.srcspan(inst);
                let loc = match sp {
                    Some(sp) => SourceLoc::new(sp.lo),
                    None => SourceLoc::default(),
                };
                self.b.set_srcloc(loc);
                // s125: a trap check emitted from this instruction
                // names this span's start as its site.
                self.cur_span_lo = sp.map(|sp| sp.lo);
                self.inst(inst)?;
                for rv in self.results_of(inst) {
                    self.debug_spill(rv);
                }
            }
        }
        self.fill_trap_blocks()?;
        self.b.seal_all_blocks();
        Ok(())
    }

    // ---- small helpers ---------------------------------------------------

    fn layout_of(&self, ty: TypeId) -> Result<Layout, BackendError> {
        layout::layout_of(&self.m.types, ty).ok_or_else(|| ice("layout of a token type"))
    }

    fn new_slot(&mut self, l: Layout) -> cranelift_codegen::ir::StackSlot {
        let align_shift = l.align.trailing_zeros() as u8;
        self.b.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            l.size.max(1),
            align_shift,
        ))
    }

    /// A module function's reference as this object sees it: its own
    /// entry when it lowered here, else an `Import` of its mangled
    /// symbol (#116; s134 for the LLVM tier; s251 extends it from
    /// `func.addr` to a static callable record's slot,
    /// `[abi.native.closure.object]`). `None` when no module function
    /// carries the name.
    fn module_func_ref(
        &mut self,
        callee: &str,
    ) -> Result<Option<cranelift_module::FuncId>, BackendError> {
        if let Some(entry) = self.funcs.get(callee) {
            return Ok(Some(entry.fid));
        }
        if let Some(&fid) = self.imports.get(callee) {
            return Ok(Some(fid));
        }
        let Some(target) = self.m.funcs.values().find(|f| f.name == callee) else {
            return Ok(None);
        };
        let sig = target.sig;
        let cc = self.om.isa().default_call_conv();
        let si_w = sig_info(self.m, sig, Conv::Wolf, cc)?;
        let symbol = wolf_backend::mangle(self.m, callee, sig);
        let fid = self
            .om
            .declare_function(&symbol, cranelift_module::Linkage::Import, &si_w.clif)
            .map_err(|e| ice(e.to_string()))?;
        self.imports.insert(callee.to_string(), fid);
        Ok(Some(fid))
    }

    fn scalar(&self, v: wolf_wir::ir::Value) -> Result<CValue, BackendError> {
        match self.vals.get(&v) {
            Some(Repr::Scalar(c)) => Ok(*c),
            // Name the function and the value's def site: `None` means
            // the def was never translated, which is a walk-order fault,
            // and the def site is the only way to find which one from a
            // bug report (s74). The id is the internal entity index, not
            // the dump's canonical number.
            other => Err(ice(format!(
                "expected scalar repr in @{}, found {other:?} for the value defined at {:?}",
                self.f.name, self.f.values[v].def,
            ))),
        }
    }

    fn addr(&self, v: wolf_wir::ir::Value) -> Result<CValue, BackendError> {
        match self.vals.get(&v) {
            Some(Repr::Addr(c)) => Ok(*c),
            other => Err(ice(format!(
                "expected aggregate repr in @{}, found {other:?} for the value defined at {:?}",
                self.f.name, self.f.values[v].def,
            ))),
        }
    }

    fn args_of(&self, inst: wolf_wir::ir::Inst) -> Vec<wolf_wir::ir::Value> {
        self.f.vpool.get(self.f.insts[inst].args)
    }

    fn results_of(&self, inst: wolf_wir::ir::Inst) -> Vec<wolf_wir::ir::Value> {
        self.f.vpool.get(self.f.insts[inst].results)
    }

    /// The trap SITE the instruction being translated names (s125):
    /// its srcspan start resolved to 1-based line:col through the
    /// backend's source-file table. `None` — no span, no known file,
    /// or no table (tests, synthetic functions) — keeps the trap
    /// site-less.
    fn cur_trap_site(&self) -> Option<(u64, u64)> {
        let file = self.f.src_file?;
        let sf = self.site_files.get(&file)?;
        Some(sf.line_col(self.cur_span_lo?))
    }

    /// The rodata symbol holding this function's source-file path
    /// (`_W.site.<idx>`, defined once per object): (data id, byte
    /// length). Only called when the site resolved, so the file is in
    /// the table.
    fn site_file_symbol(&mut self) -> Result<(cranelift_module::DataId, usize), BackendError> {
        let idx = self
            .f
            .src_file
            .ok_or_else(|| ice("trap site without a src file"))?;
        let path = &self
            .site_files
            .get(&idx)
            .ok_or_else(|| ice("trap site names an unmapped file"))?
            .path;
        let len = path.len();
        if let Some(&did) = self.site_file_data.get(&idx) {
            return Ok((did, len));
        }
        let sym = format!("_W.site.{idx}");
        let did = self
            .om
            .declare_data(&sym, cranelift_module::Linkage::Local, false, false)
            .map_err(|e| ice(e.to_string()))?;
        let mut desc = cranelift_module::DataDescription::new();
        desc.define(path.clone().into_bytes().into_boxed_slice());
        self.om
            .define_data(did, &desc)
            .map_err(|e| ice(e.to_string()))?;
        self.site_file_data.insert(idx, did);
        Ok((did, len))
    }

    /// The (empty, cold) trap block for one (kind, site). Bodies are
    /// emitted at the end of translation ([`Tx::fill_trap_blocks`]) —
    /// the frontend forbids switching away from an unfilled block, so
    /// they cannot be filled mid-instruction.
    fn trap_block(&mut self, code: i32, site: Option<(u64, u64)>) -> cranelift_codegen::ir::Block {
        if let Some(&tb) = self.trap_blocks.get(&(code, site)) {
            return tb;
        }
        let tb = self.b.create_block();
        self.b.set_cold_block(tb);
        self.trap_blocks.insert((code, site), tb);
        tb
    }

    /// Emit every pending trap block's body: call the trap reporter —
    /// `__wolf_rt_trap_at(code, file, len, line, col)` for a sited
    /// block, `__wolf_rt_trap(code)` for a site-less one (both report
    /// the kind on the byte-identical first line, never return) — then
    /// a CLIF trap for the unreachable tail.
    fn fill_trap_blocks(&mut self) -> Result<(), BackendError> {
        let mut pending: Vec<_> = std::mem::take(&mut self.trap_blocks).into_iter().collect();
        // Deterministic emission order (HashMap iteration is not).
        pending.sort_by_key(|&(k, _)| k);
        for ((code, site), tb) in pending {
            self.b.switch_to_block(tb);
            let c = self.b.ins().iconst(ctypes::I32, code as i64);
            if self.freestanding {
                self.hook_trap_call(c, site)?;
            } else {
                match site {
                    Some((line, col)) => {
                        let (did, len) = self.site_file_symbol()?;
                        let gv = self.om.declare_data_in_func(did, self.b.func);
                        let fref = self.rt_ref("__wolf_rt_trap_at")?;
                        let p = self.b.ins().symbol_value(ctypes::I64, gv);
                        let l = self.b.ins().iconst(ctypes::I64, len as i64);
                        let ln = self.b.ins().iconst(ctypes::I64, line as i64);
                        let cl = self.b.ins().iconst(ctypes::I64, col as i64);
                        self.b.ins().call(fref, &[c, p, l, ln, cl]);
                    }
                    None => {
                        let fref = self.rt_ref("__wolf_rt_trap")?;
                        self.b.ins().call(fref, &[c]);
                    }
                }
            }
            self.b
                .ins()
                .trap(TrapCode::user(code as u8).ok_or_else(|| ice("trap code 0"))?);
        }
        Ok(())
    }

    /// The freestanding trap report (kw04, K8(a), `[abi.target.none.hooks]`):
    /// `wolf_trap(kind, file, file_len, line, col)` — the site when this
    /// trap has one, `(null, 0, 0, 0)` otherwise. The caller emits the
    /// CLIF `trap` (`ud2`) after it: the hook must not return.
    fn hook_trap_call(
        &mut self,
        code: CValue,
        site: Option<(u64, u64)>,
    ) -> Result<(), BackendError> {
        let fref = self.hook_ref()?;
        let (p, len, line, col) = match site {
            Some((line, col)) => {
                let (did, len) = self.site_file_symbol()?;
                let gv = self.om.declare_data_in_func(did, self.b.func);
                let p = self.b.ins().symbol_value(ctypes::I64, gv);
                (p, len as i64, line as i64, col as i64)
            }
            None => (self.b.ins().iconst(ctypes::I64, 0), 0, 0, 0),
        };
        let l = self.b.ins().iconst(ctypes::I64, len);
        let ln = self.b.ins().iconst(ctypes::I64, line);
        let cl = self.b.ins().iconst(ctypes::I64, col);
        self.b.ins().call(fref, &[code, p, l, ln, cl]);
        Ok(())
    }

    /// Declare (once) and import (per function) the trap hook
    /// `wolf_trap(i32, ptr, i64, i64, i64)` under the platform C
    /// convention (kw04). Not a runtime shim: the program supplies it.
    fn hook_ref(&mut self) -> Result<cranelift_codegen::ir::FuncRef, BackendError> {
        let name = wolf_backend::target::TRAP_HOOK;
        if let Some(&(fr, _)) = self.fref_cache.get(name) {
            return Ok(fr);
        }
        let fid = match self.rt.get(name) {
            Some(&fid) => fid,
            None => {
                let mut sig = Signature::new(self.om.isa().default_call_conv());
                sig.params.push(AbiParam::new(ctypes::I32));
                for _ in 0..4 {
                    sig.params.push(AbiParam::new(ctypes::I64));
                }
                let fid = self
                    .om
                    .declare_function(name, cranelift_module::Linkage::Import, &sig)
                    .map_err(|e| ice(e.to_string()))?;
                self.rt.insert(name, fid);
                fid
            }
        };
        let fr = self.om.declare_func_in_func(fid, self.b.func);
        self.fref_cache.insert(name.to_string(), (fr, Conv::C));
        Ok(fr)
    }

    /// Branch to the (kind, current site) trap block when `cond` is
    /// nonzero, and continue in a fresh block otherwise.
    fn trap_if(&mut self, cond: CValue, kind: TrapKind) -> Result<(), BackendError> {
        let code = trap_code(kind);
        let site = self.cur_trap_site();
        let tb = self.trap_block(code, site);
        let cont = self.b.create_block();
        let empty: [BlockArg; 0] = [];
        self.b
            .ins()
            .brif(cond, tb, empty.iter(), cont, empty.iter());
        self.b.switch_to_block(cont);
        Ok(())
    }

    /// Declare (once) and import (per function) a runtime shim.
    fn rt_ref(
        &mut self,
        name: &'static str,
    ) -> Result<cranelift_codegen::ir::FuncRef, BackendError> {
        if let Some(&(fr, _)) = self.fref_cache.get(name) {
            return Ok(fr);
        }
        if self.freestanding && !wolf_backend::target::none_rt_provides(name) {
            // The driver refuses these by name before any backend runs
            // (`wolf_backend::target::freestanding_refusal`); this is
            // the defensive twin, so no runtime symbol the freestanding
            // runtime (kw12, `libwolf_rt_none.a`) does not define can
            // reach a freestanding object whatever the caller skipped.
            let (construct, _) = wolf_backend::target::runtime_construct(name);
            return Err(BackendError::Unsupported(format!(
                "{construct} needs the hosted runtime (target {})",
                wolf_backend::target::FREESTANDING
            )));
        }
        let fid = match self.rt.get(name) {
            Some(&fid) => fid,
            None => {
                // Every runtime symbol links on every host the clif
                // gate opens for: the s60a bring-up's by-name refusal
                // table (`windows_unserved` — the task layer, procs,
                // channels, `sync`/`when`, `os.signal`, `net`
                // deadlines) emptied one target at a time at s60b and
                // retired with its last row.
                let (_, nparams, has_ret) = RT_SYMBOLS
                    .iter()
                    .find(|(n, _, _)| *n == name)
                    .copied()
                    .ok_or_else(|| ice(format!("unknown runtime symbol {name}")))?;
                let mut sig = Signature::new(self.om.isa().default_call_conv());
                for i in 0..nparams {
                    // The trap code and the C entry's argc (s267) are
                    // i32; the bool shims take an i8 where the value
                    // crosses (WIR bool crosses as I8); the f64 write
                    // shim takes a real f64; every other shim param is a
                    // pointer/size/packed-spec i64.
                    let ty = if (name == "__wolf_rt_trap"
                        || name == "__wolf_rt_trap_at"
                        || name == "__wolf_rt_main_start")
                        && i == 0
                    {
                        ctypes::I32
                    } else if (name == "__wolf_rt_print_bool" && i == 0)
                        || ((name == "__wolf_rt_write_bool" || name == "__wolf_rt_strbuf_bool")
                            && i == 1)
                        // s73: select's `has_else` flag crosses as i8.
                        || (name == "__wolf_rt_select" && i == 3)
                    {
                        ctypes::I8
                    } else if (name == "__wolf_rt_write_f64" || name == "__wolf_rt_strbuf_f64")
                        && i == 1
                    {
                        ctypes::F64
                    } else {
                        ctypes::I64
                    };
                    sig.params.push(AbiParam::new(ty));
                }
                if has_ret {
                    // s73: the conc status returns are i32; the killed
                    // poll is i8; everything else stays i64.
                    let rty = match name {
                        "__wolf_rt_chan_send"
                        | "__wolf_rt_chan_send_region"
                        | "__wolf_rt_chan_recv"
                        | "__wolf_rt_when_acquire" => ctypes::I32,
                        "__wolf_rt_task_killed" => ctypes::I8,
                        _ => ctypes::I64,
                    };
                    sig.returns.push(AbiParam::new(rty));
                }
                let fid = self
                    .om
                    .declare_function(name, cranelift_module::Linkage::Import, &sig)
                    .map_err(|e| ice(e.to_string()))?;
                self.rt.insert(name, fid);
                fid
            }
        };
        let fr = self.om.declare_func_in_func(fid, self.b.func);
        self.fref_cache.insert(name.to_string(), (fr, Conv::Wolf));
        Ok(fr)
    }

    /// A libm math symbol, `(ty, ty) -> ty` under the C convention.
    /// Keyed `libm::<name>` in the shared caches so a wolf function
    /// that happens to be called `fmod` can never collide with it —
    /// `lower_call` reads `fref_cache` by the callee's own name.
    fn libm_ref(
        &mut self,
        symbol: &'static str,
        ty: ctypes::Type,
    ) -> Result<cranelift_codegen::ir::FuncRef, BackendError> {
        let key = format!("libm::{symbol}");
        if let Some(&(fr, _)) = self.fref_cache.get(&key) {
            return Ok(fr);
        }
        if self.freestanding {
            return Err(BackendError::Unsupported(format!(
                "a floating-point value (`{symbol}`) on target {} (K10)",
                wolf_backend::target::FREESTANDING
            )));
        }
        let fid = match self.imports.get(&key) {
            Some(&fid) => fid,
            None => {
                let mut sig = Signature::new(self.om.isa().default_call_conv());
                sig.params.push(AbiParam::new(ty));
                sig.params.push(AbiParam::new(ty));
                sig.returns.push(AbiParam::new(ty));
                let fid = self
                    .om
                    .declare_function(symbol, cranelift_module::Linkage::Import, &sig)
                    .map_err(|e| ice(e.to_string()))?;
                self.imports.insert(key.clone(), fid);
                fid
            }
        };
        let fr = self.om.declare_func_in_func(fid, self.b.func);
        self.fref_cache.insert(key, (fr, Conv::C));
        Ok(fr)
    }

    /// A fresh slot big enough to hold `units` whole eightbytes (the
    /// [`Tx::store_unit`] invariant: full-width unit stores are always
    /// in bounds), returning its address.
    fn units_slot_addr(&mut self, units: &[Unit]) -> CValue {
        let slot = self.new_slot(Layout {
            size: (units.len() as u32).max(1) * 8,
            align: 8,
        });
        self.b.ins().stack_addr(ctypes::I64, slot, 0)
    }

    /// Store one register unit into `base + u.offset`. INTEGER units
    /// store the full eightbyte (callers guarantee a rounded slot —
    /// [`Tx::units_slot_addr`]); bytes beyond `u.bytes` are undefined
    /// slack, exactly like the register's high bytes.
    fn store_unit(&mut self, base: CValue, u: &Unit, v: CValue) {
        self.b
            .ins()
            .store(MemFlagsData::trusted(), v, base, u.offset as i32);
    }

    /// Load one register unit from `base + u.offset`. INTEGER units
    /// with partial coverage assemble from exact chunks — `base` may
    /// be an interior view of a larger aggregate, so no byte beyond
    /// the value's real extent is ever read.
    fn load_unit(&mut self, base: CValue, u: &Unit) -> CValue {
        let flags = MemFlagsData::trusted();
        match u.class {
            RegClass::Sse => {
                let ty = unit_clif_ty(u);
                self.b.ins().load(ty, flags, base, u.offset as i32)
            }
            RegClass::Int => {
                if u.bytes >= 8 {
                    return self.b.ins().load(ctypes::I64, flags, base, u.offset as i32);
                }
                let mut acc: Option<CValue> = None;
                let mut off = 0u32;
                let bytes = u.bytes as u32;
                for (chunk, ty) in [(4u32, ctypes::I32), (2, ctypes::I16), (1, ctypes::I8)] {
                    while bytes - off >= chunk {
                        let v = self.b.ins().load(ty, flags, base, (u.offset + off) as i32);
                        let wide = self.b.ins().uextend(ctypes::I64, v);
                        let shifted = if off == 0 {
                            wide
                        } else {
                            self.b.ins().ishl_imm_s(wide, (off * 8) as i64)
                        };
                        acc = Some(match acc {
                            None => shifted,
                            Some(a) => self.b.ins().bor(a, shifted),
                        });
                        off += chunk;
                    }
                }
                acc.unwrap_or_else(|| self.b.ins().iconst(ctypes::I64, 0))
            }
        }
    }

    /// Copy `size` bytes from `src` to `dst` (both addresses), in
    /// straight-line chunks. Sizes are small compile-time constants.
    fn copy_bytes(&mut self, dst: CValue, src: CValue, size: u32) {
        let flags = MemFlagsData::trusted();
        let mut off = 0u32;
        for (chunk, ty) in [
            (8u32, ctypes::I64),
            (4, ctypes::I32),
            (2, ctypes::I16),
            (1, ctypes::I8),
        ] {
            while size - off >= chunk {
                let v = self.b.ins().load(ty, flags, src, off as i32);
                self.b.ins().store(flags, v, dst, off as i32);
                off += chunk;
            }
        }
    }

    /// Load `size` bytes from `src` into chunk values (phase one of a
    /// clobber-safe copy).
    fn load_chunks(&mut self, src: CValue, size: u32) -> Vec<(u32, CValue)> {
        let flags = MemFlagsData::trusted();
        let mut out = Vec::new();
        let mut off = 0u32;
        for (chunk, ty) in [
            (8u32, ctypes::I64),
            (4, ctypes::I32),
            (2, ctypes::I16),
            (1, ctypes::I8),
        ] {
            while size - off >= chunk {
                out.push((off, self.b.ins().load(ty, flags, src, off as i32)));
                off += chunk;
            }
        }
        out
    }

    fn store_chunks(&mut self, dst: CValue, chunks: &[(u32, CValue)]) {
        let flags = MemFlagsData::trusted();
        for &(off, v) in chunks {
            self.b.ins().store(flags, v, dst, off as i32);
        }
    }

    // ---- edges -----------------------------------------------------------

    /// Prepare one branch edge: aggregate args are copied into the
    /// target's param slots (loads first, then stores — see module
    /// docs); scalar args become CLIF branch args.
    fn edge(
        &mut self,
        bc: wolf_wir::ir::BlockCall,
    ) -> Result<(cranelift_codegen::ir::Block, Vec<BlockArg>), BackendError> {
        let target = bc.block;
        let args = self.f.vpool.get(bc.args);
        let params = self.f.block_params(target);
        let mut scalars: Vec<BlockArg> = Vec::new();
        // Phase 1: load every aggregate arg's bytes.
        let mut copies: Vec<(cranelift_codegen::ir::StackSlot, Vec<(u32, CValue)>)> = Vec::new();
        for (i, (&av, &pv)) in args.iter().zip(params.iter()).enumerate() {
            let ty = self.f.value_ty(pv);
            if self.m.types.is_token(ty) {
                continue;
            }
            if is_agg(self.m, ty) {
                let slot = *self
                    .param_slots
                    .get(&(target, i))
                    .ok_or_else(|| ice("aggregate block param has no slot"))?;
                let src = self.addr(av)?;
                let size = self.layout_of(ty)?.size;
                let chunks = self.load_chunks(src, size);
                copies.push((slot, chunks));
            } else {
                scalars.push(BlockArg::Value(self.scalar(av)?));
            }
        }
        // Phase 2: store into the target slots.
        for (slot, chunks) in copies {
            let dst = self.b.ins().stack_addr(ctypes::I64, slot, 0);
            self.store_chunks(dst, &chunks);
        }
        Ok((self.blocks[&target], scalars))
    }

    // ---- instruction dispatch --------------------------------------------

    fn inst(&mut self, inst: wolf_wir::ir::Inst) -> Result<(), BackendError> {
        let data = self.f.insts[inst];
        let op = data.op;
        let args = self.args_of(inst);
        let results = self.results_of(inst);
        match op {
            Opcode::Iconst => {
                if self.hoisted_constants.contains(&results[0]) {
                    return Ok(());
                }
                let Aux::Int(n) = data.aux else {
                    return Err(ice("iconst without payload"));
                };
                let ty = scalar_clif_ty(self.m, self.f.value_ty(results[0]))
                    .ok_or_else(|| ice("iconst of non-scalar"))?;
                let c = self.b.ins().iconst(ty, n);
                self.vals.insert(results[0], Repr::Scalar(c));
            }
            Opcode::Fconst => {
                let Aux::FloatBits(bits) = data.aux else {
                    return Err(ice("fconst without payload"));
                };
                let rty = self.f.value_ty(results[0]);
                let c = if matches!(self.m.types.get(rty), TypeData::F32) {
                    self.b
                        .ins()
                        .f32const(cranelift_codegen::ir::immediates::Ieee32::with_bits(
                            bits as u32,
                        ))
                } else {
                    self.b
                        .ins()
                        .f64const(cranelift_codegen::ir::immediates::Ieee64::with_bits(bits))
                };
                self.vals.insert(results[0], Repr::Scalar(c));
            }
            Opcode::Bconst => {
                let Aux::Bool(v) = data.aux else {
                    return Err(ice("bconst without payload"));
                };
                let c = self.b.ins().iconst(ctypes::I8, i64::from(v));
                self.vals.insert(results[0], Repr::Scalar(c));
            }

            // ---- checked / wrapping / saturating integer arithmetic ----
            Opcode::IaddChk
            | Opcode::IsubChk
            | Opcode::ImulChk
            | Opcode::UaddChk
            | Opcode::UsubChk
            | Opcode::UmulChk => {
                let r = self.checked_arith(op, args[0], args[1], results[0])?;
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::IdivChk | Opcode::IremChk | Opcode::UdivChk | Opcode::UremChk => {
                let r = self.checked_div(op, args[0], args[1], results[0])?;
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::IaddWrap | Opcode::IsubWrap | Opcode::ImulWrap => {
                let a = self.scalar(args[0])?;
                let b = self.scalar(args[1])?;
                let r = match op {
                    Opcode::IaddWrap => self.b.ins().iadd(a, b),
                    Opcode::IsubWrap => self.b.ins().isub(a, b),
                    _ => self.b.ins().imul(a, b),
                };
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::IaddSat | Opcode::IsubSat | Opcode::ImulSat => {
                let r = self.sat_arith(op, args[0], args[1], results[0])?;
                self.vals.insert(results[0], Repr::Scalar(r));
            }

            // ---- bit ops (shift amounts mask in CLIF, matching WIR) ----
            Opcode::Band | Opcode::Bor | Opcode::Bxor => {
                let a = self.scalar(args[0])?;
                let b = self.scalar(args[1])?;
                let r = match op {
                    Opcode::Band => self.b.ins().band(a, b),
                    Opcode::Bor => self.b.ins().bor(a, b),
                    _ => self.b.ins().bxor(a, b),
                };
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::Shl | Opcode::Lshr | Opcode::Ashr => {
                let a = self.scalar(args[0])?;
                let b = self.scalar(args[1])?;
                let r = match op {
                    Opcode::Shl => self.b.ins().ishl(a, b),
                    Opcode::Lshr => self.b.ins().ushr(a, b),
                    _ => self.b.ins().sshr(a, b),
                };
                self.vals.insert(results[0], Repr::Scalar(r));
            }

            // ---- floats (IEEE, no traps) ----
            Opcode::Fadd | Opcode::Fsub | Opcode::Fmul | Opcode::Fdiv => {
                let a = self.scalar(args[0])?;
                let b = self.scalar(args[1])?;
                let r = match op {
                    Opcode::Fadd => self.b.ins().fadd(a, b),
                    Opcode::Fsub => self.b.ins().fsub(a, b),
                    Opcode::Fmul => self.b.ins().fmul(a, b),
                    _ => self.b.ins().fdiv(a, b),
                };
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            // Cranelift has no `frem` instruction, and the naive
            // `a - trunc(a / b) * b` expansion is NOT `fmod` — it
            // loses precision badly once `|a|` is far above `|b|`. The
            // ruled semantics are C's, so C's function is what this
            // calls (`[type.float.rem]`, s156): the same routine LLVM's
            // own `frem` lowers to, so the two backends agree by
            // construction rather than by testing. libm needs no new
            // link flag on the targets this gate opens for — it is
            // inside libSystem on darwin and inside libc on glibc
            // ≥ 2.34, both of which the runtime already pulls.
            Opcode::Frem => {
                let a = self.scalar(args[0])?;
                let b = self.scalar(args[1])?;
                let f32ty = self.b.func.dfg.value_type(a) == ctypes::F32;
                let (symbol, ty) = if f32ty {
                    ("fmodf", ctypes::F32)
                } else {
                    ("fmod", ctypes::F64)
                };
                let fr = self.libm_ref(symbol, ty)?;
                let call = self.b.ins().call(fr, &[a, b]);
                let r = self.b.inst_results(call)[0];
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::Fneg => {
                let a = self.scalar(args[0])?;
                let r = self.b.ins().fneg(a);
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::Fma => {
                let (a, b, c) = (
                    self.scalar(args[0])?,
                    self.scalar(args[1])?,
                    self.scalar(args[2])?,
                );
                let r = self.b.ins().fma(a, b, c);
                self.vals.insert(results[0], Repr::Scalar(r));
            }

            // ---- compares ----
            Opcode::Icmp => {
                let Aux::IntCc(cc) = data.aux else {
                    return Err(ice("icmp without condition"));
                };
                let a = self.scalar(args[0])?;
                let b = self.scalar(args[1])?;
                let r = self.b.ins().icmp(int_cc(cc), a, b);
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::Fcmp => {
                let Aux::FloatCc(cc) = data.aux else {
                    return Err(ice("fcmp without condition"));
                };
                let a = self.scalar(args[0])?;
                let b = self.scalar(args[1])?;
                let r = self.b.ins().fcmp(float_cc(cc), a, b);
                self.vals.insert(results[0], Repr::Scalar(r));
            }

            // ---- conversions ----
            Opcode::Fpromote => {
                let a = self.scalar(args[0])?;
                let r = self.b.ins().fpromote(ctypes::F64, a);
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::Sext | Opcode::Zext | Opcode::Itrunc => {
                let a = self.scalar(args[0])?;
                let ty = scalar_clif_ty(self.m, self.f.value_ty(results[0]))
                    .ok_or_else(|| ice("conversion to non-scalar"))?;
                let r = match op {
                    Opcode::Sext => self.b.ins().sextend(ty, a),
                    Opcode::Zext => self.b.ins().uextend(ty, a),
                    _ => self.b.ins().ireduce(ty, a),
                };
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::Sitofp | Opcode::Uitofp => {
                // int → float (D54.4): the free widening direction, no
                // trap. Source signedness (encoded in the opcode) picks
                // signed vs unsigned conversion.
                let a = self.scalar(args[0])?;
                let ty = scalar_clif_ty(self.m, self.f.value_ty(results[0]))
                    .ok_or_else(|| ice("int→float to non-scalar"))?;
                let r = if op == Opcode::Sitofp {
                    self.b.ins().fcvt_from_sint(ty, a)
                } else {
                    self.b.ins().fcvt_from_uint(ty, a)
                };
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::FtosiChk | Opcode::FtouiChk => {
                // float → int (D54.4): truncate toward zero, TRAP
                // (overflow) on out-of-range or NaN. The value is the
                // SATURATING conversion (defined for every input, so no
                // hardware trap); the trap is an explicit range+NaN test
                // branching to `__wolf_rt_trap`, exactly the
                // checked-arith machinery. The range check runs in f64
                // (an f32 source promotes losslessly) so the boundary
                // constants are exact.
                let a = self.scalar(args[0])?;
                let signed = op == Opcode::FtosiChk;
                let int_ty = scalar_clif_ty(self.m, self.f.value_ty(results[0]))
                    .ok_or_else(|| ice("float→int to non-scalar"))?;
                let f64ty = cranelift_codegen::ir::types::F64;
                let check = if self.f.value_ty(args[0]) == wolf_wir::types::F32 {
                    self.b.ins().fpromote(f64ty, a)
                } else {
                    a
                };
                let (lo, hi) = wolf_wir::types::ftoi_bounds(signed, int_ty.bits());
                let lo_c =
                    self.b
                        .ins()
                        .f64const(cranelift_codegen::ir::immediates::Ieee64::with_bits(
                            lo.to_bits(),
                        ));
                let hi_c =
                    self.b
                        .ins()
                        .f64const(cranelift_codegen::ir::immediates::Ieee64::with_bits(
                            hi.to_bits(),
                        ));
                let oob_lo = self.b.ins().fcmp(FloatCC::LessThanOrEqual, check, lo_c);
                let oob_hi = self.b.ins().fcmp(FloatCC::GreaterThanOrEqual, check, hi_c);
                let is_nan = self.b.ins().fcmp(FloatCC::Unordered, check, check);
                let bad0 = self.b.ins().bor(oob_lo, oob_hi);
                let bad = self.b.ins().bor(bad0, is_nan);
                self.trap_if(bad, TrapKind::Overflow)?;
                let r = if signed {
                    self.b.ins().fcvt_to_sint_sat(int_ty, a)
                } else {
                    self.b.ins().fcvt_to_uint_sat(int_ty, a)
                };
                self.vals.insert(results[0], Repr::Scalar(r));
            }

            // kw06 (`[mem.prov.expose]`): a pointer is one 64-bit word
            // on this tier, so both bridges are the identity.
            Opcode::PtrToInt | Opcode::PtrFromInt => {
                let a = self.scalar(args[0])?;
                self.vals.insert(results[0], Repr::Scalar(a));
            }

            // ---- memory ----
            Opcode::PtrOff => {
                let Aux::Scale(scale) = data.aux else {
                    return Err(ice("ptr.off without scale"));
                };
                let p = self.scalar(args[0])?;
                let i = self.scalar(args[1])?;
                // The index is i64 in WIR ([mem] lowering); scale is a
                // positive immediate.
                let scaled = self.b.ins().imul_imm_s(i, scale as i64);
                let r = self.b.ins().iadd(p, scaled);
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::Load => {
                let p = self.scalar(args[0])?;
                let rty = self.f.value_ty(results[0]);
                if is_agg(self.m, rty) {
                    let l = self.layout_of(rty)?;
                    let slot = self.new_slot(l);
                    let dst = self.b.ins().stack_addr(ctypes::I64, slot, 0);
                    self.copy_bytes(dst, p, l.size);
                    self.vals.insert(results[0], Repr::Addr(dst));
                } else {
                    let ty =
                        scalar_clif_ty(self.m, rty).ok_or_else(|| ice("load of token type"))?;
                    let r = self.b.ins().load(ty, access_flags(data.aux), p, 0);
                    self.vals.insert(results[0], Repr::Scalar(r));
                }
            }
            Opcode::Store => {
                let vty = self.f.value_ty(args[0]);
                let p = self.scalar(args[1])?;
                if is_agg(self.m, vty) {
                    let src = self.addr(args[0])?;
                    let size = self.layout_of(vty)?.size;
                    self.copy_bytes(p, src, size);
                } else {
                    let v = self.scalar(args[0])?;
                    self.b.ins().store(access_flags(data.aux), v, p, 0);
                }
                // The successor token.
                self.vals.insert(results[0], Repr::Token);
            }

            // ---- aggregates ----
            Opcode::AggMake => {
                if let Some(&ptr) = self.read_lends.get(&results[0]) {
                    let addr = self.scalar(ptr)?;
                    self.vals.insert(results[0], Repr::Addr(addr));
                    return Ok(());
                }
                let rty = self.f.value_ty(results[0]);
                let TypeData::Agg(fields) = self.m.types.get(rty).clone() else {
                    return Err(ice("agg.make of non-aggregate"));
                };
                let l = self.layout_of(rty)?;
                let offs = layout::field_offsets(&self.m.types, &fields)
                    .ok_or_else(|| ice("aggregate with token field"))?;
                let slot = self.new_slot(l);
                let dst = self.b.ins().stack_addr(ctypes::I64, slot, 0);
                for ((&av, &fty), &off) in args.iter().zip(fields.iter()).zip(offs.iter()) {
                    self.store_field(dst, off, fty, av)?;
                }
                self.vals.insert(results[0], Repr::Addr(dst));
            }
            Opcode::AggGet => {
                let Aux::Int(k) = data.aux else {
                    return Err(ice("agg.get without index"));
                };
                let aty = self.f.value_ty(args[0]);
                let TypeData::Agg(fields) = self.m.types.get(aty).clone() else {
                    return Err(ice("agg.get of non-aggregate"));
                };
                let offs = layout::field_offsets(&self.m.types, &fields)
                    .ok_or_else(|| ice("aggregate with token field"))?;
                let base = self.addr(args[0])?;
                let off = offs[k as usize];
                let fty = fields[k as usize];
                let repr = self.load_field(base, off, fty)?;
                self.vals.insert(results[0], repr);
            }

            // ---- union slots (s257): the member at offset 0 ----
            Opcode::UnionMake => {
                let rty = self.f.value_ty(results[0]);
                let l = self.layout_of(rty)?;
                let slot = self.new_slot(l);
                let dst = self.b.ins().stack_addr(ctypes::I64, slot, 0);
                self.store_field(dst, 0, self.f.value_ty(args[0]), args[0])?;
                self.vals.insert(results[0], Repr::Addr(dst));
            }
            Opcode::UnionView => {
                let rty = self.f.value_ty(results[0]);
                let base = self.addr(args[0])?;
                let repr = self.load_field(base, 0, rty)?;
                self.vals.insert(results[0], repr);
            }

            // ---- error unions (value ops; branching is plain br) ----
            Opcode::EuMakeOk => {
                let rty = self.f.value_ty(results[0]);
                let l = self.layout_of(rty)?;
                let slot = self.new_slot(l);
                let dst = self.b.ins().stack_addr(ctypes::I64, slot, 0);
                let zero = self.b.ins().iconst(ctypes::I64, 0);
                self.b.ins().store(MemFlagsData::trusted(), zero, dst, 0);
                if let Some(&ok) = args.first() {
                    let off = layout::eu_ok_offset(&self.m.types, rty)
                        .ok_or_else(|| ice("eu.make.ok operand for a unit ok"))?;
                    self.store_field(dst, off, self.f.value_ty(ok), ok)?;
                }
                self.vals.insert(results[0], Repr::Addr(dst));
            }
            Opcode::EuMakeErr => {
                let rty = self.f.value_ty(results[0]);
                let l = self.layout_of(rty)?;
                let slot = self.new_slot(l);
                let dst = self.b.ins().stack_addr(ctypes::I64, slot, 0);
                let tag = self.scalar(args[0])?;
                self.b.ins().store(MemFlagsData::trusted(), tag, dst, 0);
                let slot_offs = layout::eu_slot_offset(&self.m.types, rty);
                let TypeData::Eu { slots, .. } = self.m.types.get(rty).clone() else {
                    return Err(ice("eu.make.err of non-eu"));
                };
                for (i, &pv) in args.iter().skip(1).enumerate() {
                    self.store_field(dst, slot_offs[i], slots[i], pv)?;
                }
                self.vals.insert(results[0], Repr::Addr(dst));
            }
            Opcode::EuIsErr => {
                let base = self.addr(args[0])?;
                let tag = self
                    .b
                    .ins()
                    .load(ctypes::I64, MemFlagsData::trusted(), base, 0);
                let r = self.b.ins().icmp_imm_s(IntCC::NotEqual, tag, 0);
                self.vals.insert(results[0], Repr::Scalar(r));
            }
            Opcode::EuOk => {
                let ety = self.f.value_ty(args[0]);
                let base = self.addr(args[0])?;
                let off = layout::eu_ok_offset(&self.m.types, ety)
                    .ok_or_else(|| ice("eu.ok of a unit ok"))?;
                let TypeData::Eu { ok: Some(okt), .. } = self.m.types.get(ety).clone() else {
                    return Err(ice("eu.ok of non-eu"));
                };
                let repr = self.load_field(base, off, okt)?;
                self.vals.insert(results[0], repr);
            }
            Opcode::EuErr => {
                let ety = self.f.value_ty(args[0]);
                let base = self.addr(args[0])?;
                match data.aux {
                    Aux::Int(k) => {
                        let offs = layout::eu_slot_offset(&self.m.types, ety);
                        let TypeData::Eu { slots, .. } = self.m.types.get(ety).clone() else {
                            return Err(ice("eu.err of non-eu"));
                        };
                        let repr = self.load_field(base, offs[k as usize], slots[k as usize])?;
                        self.vals.insert(results[0], repr);
                    }
                    _ => {
                        let tag = self
                            .b
                            .ins()
                            .load(ctypes::I64, MemFlagsData::trusted(), base, 0);
                        self.vals.insert(results[0], Repr::Scalar(tag));
                    }
                }
            }

            // ---- module data (s31: string literals in rodata) ----
            Opcode::DataAddr => {
                let Aux::Data(idx) = data.aux else {
                    return Err(ice("data.addr without payload"));
                };
                let did = match self.data_ids.get(&idx) {
                    Some(&did) => did,
                    // kw09 (`[mem.static]`, `[abi.link.extern]`): module
                    // state another object defines, or a link-time
                    // symbol — imported under its own name. (The
                    // defining object registered its definition first.)
                    None if self
                        .m
                        .data
                        .get(idx as usize)
                        .is_some_and(|d| d.stat.is_some()) =>
                    {
                        let d = &self.m.data[idx as usize];
                        let writable = d
                            .stat
                            .as_ref()
                            .is_some_and(|st| st.kind == wolf_wir::ir::StaticKind::Var);
                        let did = self
                            .om
                            .declare_data(
                                &d.name,
                                cranelift_module::Linkage::Import,
                                writable,
                                false,
                            )
                            .map_err(|e| ice(e.to_string()))?;
                        self.data_ids.insert(idx, did);
                        did
                    }
                    None => {
                        let d = self
                            .m
                            .data
                            .get(idx as usize)
                            .ok_or_else(|| ice("data.addr names missing data"))?
                            .clone();
                        // Local read-only bytes; the `_W.` prefix keeps
                        // wolf data out of the C namespace like `_W`
                        // function mangling does.
                        let sym = format!("_W.{}", d.name);
                        let did = self
                            .om
                            .declare_data(&sym, cranelift_module::Linkage::Local, false, false)
                            .map_err(|e| ice(e.to_string()))?;
                        let mut desc = cranelift_module::DataDescription::new();
                        if d.funcs.is_empty() {
                            desc.define(d.bytes.clone().into_boxed_slice());
                        } else {
                            // s98: a vtable — pointer-sized slots, each
                            // a function relocation ([abi.native.dyn]).
                            // REAL zero bytes, not `define_zeroinit`:
                            // zeroinit is Init::Zeros, which
                            // cranelift-object places in BSS — and a
                            // relocation into a zero-fill section is
                            // malformed (Apple's ld SEGFAULTS on it;
                            // found the day the s59 gate lifted). With
                            // byte contents the table lands in
                            // read-only-data-with-relocs, where a
                            // vtable belongs on every format.
                            desc.define(vec![0u8; d.funcs.len() * 8].into_boxed_slice());
                            // Pointer-slot alignment (Apple ld warns
                            // on the default 1 and unaligned function
                            // pointers are wrong everywhere).
                            desc.set_align(8);
                            // s251 (#806, #807): a slot's function
                            // need not live in this object's subset
                            // — the symbol is a link-time constant
                            // exactly as `func.addr`'s is (#116), so
                            // an out-of-subset slot imports the
                            // mangled name (`[abi.native.closure.object]`).
                            for (i, fname) in d.funcs.iter().enumerate() {
                                let Some(fid) = self.module_func_ref(fname)? else {
                                    return Err(nyi(format!(
                                        "a vtable slot `@{fname}` names no module function"
                                    )));
                                };
                                let fref = self.om.declare_func_in_data(fid, &mut desc);
                                desc.write_function_addr((i * 8) as u32, fref);
                            }
                        }
                        self.om
                            .define_data(did, &desc)
                            .map_err(|e| ice(e.to_string()))?;
                        self.data_ids.insert(idx, did);
                        did
                    }
                };
                let gv = self.om.declare_data_in_func(did, self.b.func);
                let p = self.b.ins().symbol_value(ctypes::I64, gv);
                self.vals.insert(results[0], Repr::Scalar(p));
            }

            // ---- C variable arguments ----
            Opcode::VaStart => {
                check_variadic_target(self.om)?;
                let p = self
                    .va_context
                    .ok_or_else(|| nyi("va_start outside a variadic definition"))?;
                self.vals.insert(results[0], Repr::Scalar(p));
            }
            Opcode::VaArg => {
                self.lower_va_arg(&args, &results)?;
            }
            Opcode::VaEnd => {
                check_variadic_target(self.om)?;
                self.vals.insert(results[0], Repr::Token);
            }

            // ---- calls ----
            Opcode::Call => {
                let Aux::Callee(ef) = data.aux else {
                    return Err(ice("call without callee"));
                };
                let callee = self.f.ext_funcs[ef].clone();
                if let Some((store, width)) = wolf_wir::volatile_intrinsic(&callee.name) {
                    // kw07 (`[mem.unsafe.volatile.2]`): one load or store
                    // of the width, in place. This tier runs Cranelift
                    // at opt level `none` and no WIR mid-end, so the
                    // instruction is emitted in program order and kept;
                    // the flags claim alignment (the clause's) and NOT
                    // `notrap` — a device address may fault.
                    let p = self.scalar(args[0])?;
                    let flags = MemFlagsData::new().with_aligned();
                    let mut rest = &results[..];
                    if store {
                        let v = self.scalar(args[1])?;
                        self.b.ins().store(flags, v, p, 0);
                    } else {
                        let ty = match width {
                            1 => ctypes::I8,
                            2 => ctypes::I16,
                            4 => ctypes::I32,
                            _ => ctypes::I64,
                        };
                        let r = self.b.ins().load(ty, flags, p, 0);
                        self.vals.insert(results[0], Repr::Scalar(r));
                        rest = &results[1..];
                    }
                    for &tok in rest {
                        self.vals.insert(tok, Repr::Token);
                    }
                } else if let Some(a) = wolf_wir::atomic_intrinsic(&callee.name) {
                    self.lower_atomic(a, &args, &results)?;
                } else if wolf_wir::fence_intrinsic(&callee.name).is_some() {
                    // kw11 (`[conc.mm.fence]`): Cranelift's one fence is
                    // sequentially consistent, so every order is it.
                    self.b.ins().fence();
                    for &tok in &results {
                        self.vals.insert(tok, Repr::Token);
                    }
                } else {
                    self.lower_call(&callee.name, callee.sig, &args, &results)?;
                }
            }
            Opcode::CallInd => {
                let Aux::Sig(sig) = data.aux else {
                    return Err(ice("call.ind without a signature"));
                };
                self.lower_call_ind(sig, &args, &results)?;
            }
            Opcode::FuncAddr => {
                // s73: the compiled task-entry pointer — a module
                // function's address as an opaque i64/ptr (the same
                // PIC regime as `symbol_value`).
                let Aux::Callee(ef) = data.aux else {
                    return Err(ice("func.addr without callee"));
                };
                let callee = self.f.ext_funcs[ef].name.clone();
                // #116: a wolf function OUTSIDE this object's subset
                // (s31 per-module objects) takes the same
                // mangled-symbol import the CALL fallback takes; an
                // address is just the import without the call.
                let Some(fid) = self.module_func_ref(&callee)? else {
                    return Err(nyi(format!(
                        "func.addr of `@{callee}` names no module function"
                    )));
                };
                let fr = self.om.declare_func_in_func(fid, self.b.func);
                let p = self.b.ins().func_addr(ctypes::I64, fr);
                self.vals.insert(results[0], Repr::Scalar(p));
            }

            // ---- the memory story (runtime shims) ----
            Opcode::RegionNew => {
                let fref = self.rt_ref("__wolf_rt_region_new")?;
                let call = self.b.ins().call(fref, &[]);
                let h = self.b.inst_results(call)[0];
                self.vals.insert(results[0], Repr::Scalar(h));
                self.vals.insert(results[1], Repr::Token);
            }
            Opcode::RegionAlloc => {
                let h = self.scalar(args[0])?;
                let size = self.scalar(args[1])?;
                let fref = self.rt_ref("__wolf_rt_region_alloc")?;
                let call = self.b.ins().call(fref, &[h, size]);
                let p = self.b.inst_results(call)[0];
                self.vals.insert(results[0], Repr::Scalar(p));
                self.vals.insert(results[1], Repr::Token);
            }
            Opcode::RegionFree => {
                let h = self.scalar(args[0])?;
                let fref = self.rt_ref("__wolf_rt_region_free")?;
                self.b.ins().call(fref, &[h]);
            }
            Opcode::SyncFreeze => {
                let h = self.scalar(args[0])?;
                let fref = self.rt_ref("__wolf_rt_region_freeze")?;
                self.b.ins().call(fref, &[h]);
                self.vals.insert(results[0], Repr::Token);
            }
            Opcode::StackAlloc => {
                // The size is an iconst by the op's contract.
                let size = self.const_of(args[0]).ok_or_else(|| {
                    nyi("stack.alloc with a non-constant size (not emitted by lowering)")
                })?;
                let size32 =
                    u32::try_from(size).map_err(|_| ice("stack.alloc size out of range"))?;
                let slot = self.new_slot(Layout {
                    size: size32,
                    align: 16,
                });
                let p = self.b.ins().stack_addr(ctypes::I64, slot, 0);
                self.vals.insert(results[0], Repr::Scalar(p));
                self.vals.insert(results[1], Repr::Token);
            }
            // s75: a token root over runtime-owned storage — ordering
            // discipline only, no machine value, nothing to emit.
            Opcode::RegionForeign => {
                self.vals.insert(results[0], Repr::Token);
            }
            Opcode::RcDup | Opcode::RcDrop => {
                return Err(nyi(
                    "shared-tier rc ops (the runtime shape lands with the shared cells)",
                ));
            }
            Opcode::SyncTransfer => {
                return Err(nyi("sync.transfer is reserved (concurrency campaign)"));
            }

            // ---- terminators ----
            Opcode::Jmp => {
                let Aux::Jump(bc) = data.aux else {
                    return Err(ice("jmp without edge"));
                };
                let (tb, cargs) = self.edge(bc)?;
                self.b.ins().jump(tb, cargs.iter());
            }
            Opcode::Br => {
                if let Some(table) = self.dense_matches.remove(&inst) {
                    self.dense_match(table)?;
                    return Ok(());
                }
                let Aux::Br(t, e) = data.aux else {
                    return Err(ice("br without edges"));
                };
                let cond = self.scalar(args[0])?;
                // Aggregate-carrying edges get per-edge trampolines so
                // the not-taken edge's copies never run.
                let needs_tramp = |tx: &Self, bc: wolf_wir::ir::BlockCall| {
                    let params = tx.f.block_params(bc.block);
                    params.iter().any(|&p| is_agg(tx.m, tx.f.value_ty(p)))
                };
                if needs_tramp(self, t) || needs_tramp(self, e) {
                    let tt = self.b.create_block();
                    let te = self.b.create_block();
                    let empty: [BlockArg; 0] = [];
                    self.b.ins().brif(cond, tt, empty.iter(), te, empty.iter());
                    for (tramp, bc) in [(tt, t), (te, e)] {
                        self.b.switch_to_block(tramp);
                        let (tb, cargs) = self.edge(bc)?;
                        self.b.ins().jump(tb, cargs.iter());
                    }
                } else {
                    let (tb, targs) = self.edge(t)?;
                    let (eb, eargs) = self.edge(e)?;
                    self.b.ins().brif(cond, tb, targs.iter(), eb, eargs.iter());
                }
            }
            Opcode::Ret => {
                let si = sig_info(
                    self.m,
                    self.f.sig,
                    self.conv,
                    self.om.isa().default_call_conv(),
                )?;
                let mut scalars: Vec<CValue> = Vec::new();
                let mut sret_i = 0usize;
                for (&rv, slot) in args.iter().zip(si.rets.iter()) {
                    match slot {
                        RSlot::Token => {}
                        RSlot::Direct(_) => scalars.push(self.scalar(rv)?),
                        RSlot::Split(units) => {
                            let src = self.addr(rv)?;
                            for u in units {
                                let v = self.load_unit(src, u);
                                scalars.push(v);
                            }
                        }
                        RSlot::Sret {
                            disc_in_reg,
                            c,
                            size,
                        } => {
                            let dst = self.sret[sret_i];
                            sret_i += 1;
                            let src = self.addr(rv)?;
                            self.copy_bytes(dst, src, *size);
                            if *disc_in_reg && !c {
                                // [abi.err.repr]: the discriminant
                                // rides the first INTEGER return
                                // register even for out-slot unions
                                // (tag is at offset 0 by layout).
                                let tag =
                                    self.b
                                        .ins()
                                        .load(ctypes::I64, MemFlagsData::trusted(), src, 0);
                                scalars.push(tag);
                            }
                        }
                    }
                }
                self.b.ins().return_(&scalars);
            }
            Opcode::Trap => {
                let kind = match data.aux {
                    Aux::Trap(k) => k,
                    _ => TrapKind::Assert,
                };
                let code = trap_code(kind);
                let c = self.b.ins().iconst(ctypes::I32, code as i64);
                // s125: an unconditional trap is already cold where it
                // stands — report its site inline, same immediates as
                // a sited cold block.
                if self.freestanding {
                    let site = self.cur_trap_site();
                    self.hook_trap_call(c, site)?;
                } else {
                    match self.cur_trap_site() {
                        Some((line, col)) => {
                            let (did, len) = self.site_file_symbol()?;
                            let gv = self.om.declare_data_in_func(did, self.b.func);
                            let fref = self.rt_ref("__wolf_rt_trap_at")?;
                            let p = self.b.ins().symbol_value(ctypes::I64, gv);
                            let l = self.b.ins().iconst(ctypes::I64, len as i64);
                            let ln = self.b.ins().iconst(ctypes::I64, line as i64);
                            let cl = self.b.ins().iconst(ctypes::I64, col as i64);
                            self.b.ins().call(fref, &[c, p, l, ln, cl]);
                        }
                        None => {
                            let fref = self.rt_ref("__wolf_rt_trap")?;
                            self.b.ins().call(fref, &[c]);
                        }
                    }
                }
                self.b
                    .ins()
                    .trap(TrapCode::user(code as u8).ok_or_else(|| ice("trap code 0"))?);
            }
        }
        Ok(())
    }

    /// Constant payload of a value defined by `iconst`, if any.
    // s268: subtract in the bit domain (never checked source arithmetic).
    // The unsigned range check precedes narrowing to the br_table index,
    // so distant i64 values cannot alias an arm through their low 32 bits.
    fn dense_match(&mut self, table: crate::dense::DenseMatch) -> Result<(), BackendError> {
        let mut value = self.scalar(table.value)?;
        if self.b.func.dfg.value_type(value) != ctypes::I64 {
            value = self.b.ins().sextend(ctypes::I64, value);
        }
        let min = self.b.ins().iconst(ctypes::I64, table.min);
        let index = self.b.ins().isub(value, min);
        let outside = self.b.ins().icmp_imm_s(
            IntCC::UnsignedGreaterThanOrEqual,
            index,
            table.targets.len() as i64,
        );
        let dispatch = self.b.create_block();
        let default = self.blocks[&table.default];
        let empty: [BlockArg; 0] = [];
        self.b
            .ins()
            .brif(outside, default, empty.iter(), dispatch, empty.iter());
        self.b.switch_to_block(dispatch);
        let index = self.b.ins().ireduce(ctypes::I32, index);
        let fallback = self.b.func.dfg.block_call(default, &[]);
        let targets: Vec<_> = table
            .targets
            .into_iter()
            .map(|b| self.b.func.dfg.block_call(self.blocks[&b], &[]))
            .collect();
        let jt = self
            .b
            .create_jump_table(JumpTableData::new(fallback, &targets));
        self.b.ins().br_table(index, jt);
        Ok(())
    }

    fn const_of(&self, v: wolf_wir::ir::Value) -> Option<i64> {
        match self.f.values[v].def {
            wolf_wir::ir::ValueDef::Result(inst, 0) if self.f.insts[inst].op == Opcode::Iconst => {
                match self.f.insts[inst].aux {
                    Aux::Int(n) => Some(n),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Store one field value (scalar or aggregate) at `base + off`.
    fn store_field(
        &mut self,
        base: CValue,
        off: u32,
        fty: TypeId,
        v: wolf_wir::ir::Value,
    ) -> Result<(), BackendError> {
        if is_agg(self.m, fty) {
            let src = self.addr(v)?;
            let size = self.layout_of(fty)?.size;
            let dst = self.b.ins().iadd_imm_s(base, off as i64);
            self.copy_bytes(dst, src, size);
        } else {
            let cv = self.scalar(v)?;
            self.b
                .ins()
                .store(MemFlagsData::trusted(), cv, base, off as i32);
        }
        Ok(())
    }

    /// Load one field (scalar loads a value; aggregate is a view into
    /// the parent's bytes — SSA immutability makes the shared address
    /// sound).
    fn load_field(&mut self, base: CValue, off: u32, fty: TypeId) -> Result<Repr, BackendError> {
        if is_agg(self.m, fty) {
            let p = self.b.ins().iadd_imm_s(base, off as i64);
            Ok(Repr::Addr(p))
        } else {
            let ty = scalar_clif_ty(self.m, fty).ok_or_else(|| ice("field of token type"))?;
            let v = self
                .b
                .ins()
                .load(ty, MemFlagsData::trusted(), base, off as i32);
            Ok(Repr::Scalar(v))
        }
    }

    /// Checked add/sub/mul, signed and unsigned (X3: trap on overflow,
    /// with the identity kept). Narrow widths widen to i64 and
    /// range-check; i64 uses the native overflow-flag ops.
    fn checked_arith(
        &mut self,
        op: Opcode,
        wa: wolf_wir::ir::Value,
        wb: wolf_wir::ir::Value,
        res: wolf_wir::ir::Value,
    ) -> Result<CValue, BackendError> {
        let ty = self.f.value_ty(res);
        let bits = self
            .m
            .types
            .int_bits(ty)
            .ok_or_else(|| ice("checked arith on non-int"))?;
        let a = self.scalar(wa)?;
        let b = self.scalar(wb)?;
        let signed = matches!(op, Opcode::IaddChk | Opcode::IsubChk | Opcode::ImulChk);
        if bits == 64 {
            let (r, o) = match op {
                Opcode::IaddChk => self.b.ins().sadd_overflow(a, b),
                Opcode::IsubChk => self.b.ins().ssub_overflow(a, b),
                Opcode::ImulChk => self.b.ins().smul_overflow(a, b),
                Opcode::UaddChk => self.b.ins().uadd_overflow(a, b),
                Opcode::UsubChk => self.b.ins().usub_overflow(a, b),
                _ => self.b.ins().umul_overflow(a, b),
            };
            self.trap_if(o, TrapKind::Overflow)?;
            return Ok(r);
        }
        // Narrow: widen, compute exactly, range-check, reduce.
        let (xa, xb) = if signed {
            (
                self.b.ins().sextend(ctypes::I64, a),
                self.b.ins().sextend(ctypes::I64, b),
            )
        } else {
            (
                self.b.ins().uextend(ctypes::I64, a),
                self.b.ins().uextend(ctypes::I64, b),
            )
        };
        let r = match op {
            Opcode::IaddChk | Opcode::UaddChk => self.b.ins().iadd(xa, xb),
            Opcode::IsubChk | Opcode::UsubChk => self.b.ins().isub(xa, xb),
            _ => self.b.ins().imul(xa, xb),
        };
        let ovf = if signed {
            let (lo, hi) = self
                .m
                .types
                .int_bounds(ty)
                .ok_or_else(|| ice("bounds of non-int"))?;
            let below = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, r, lo as i64);
            let above = self
                .b
                .ins()
                .icmp_imm_s(IntCC::SignedGreaterThan, r, hi as i64);
            self.b.ins().bor(below, above)
        } else {
            let mask = if bits >= 64 {
                -1i64
            } else {
                (1i64 << bits) - 1
            };
            // Unsigned: sub may go negative, add/mul may exceed the
            // mask — one signed range check covers both since the
            // widened exact result is in (i64::MIN, 2^63).
            let below = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, r, 0);
            let above = self.b.ins().icmp_imm_s(IntCC::SignedGreaterThan, r, mask);
            self.b.ins().bor(below, above)
        };
        self.trap_if(ovf, TrapKind::Overflow)?;
        Ok(self.b.ins().ireduce(clif_int(bits), r))
    }

    /// Checked division/remainder: explicit zero (and MIN/-1 for
    /// signed) checks branch to the trap blocks; the native op runs on
    /// the safe residue.
    fn checked_div(
        &mut self,
        op: Opcode,
        wa: wolf_wir::ir::Value,
        wb: wolf_wir::ir::Value,
        res: wolf_wir::ir::Value,
    ) -> Result<CValue, BackendError> {
        let ty = self.f.value_ty(res);
        let a = self.scalar(wa)?;
        let b = self.scalar(wb)?;
        let zero = self.b.ins().icmp_imm_s(IntCC::Equal, b, 0);
        self.trap_if(zero, TrapKind::DivZero)?;
        match op {
            Opcode::IdivChk | Opcode::IremChk => {
                let (lo, _) = self
                    .m
                    .types
                    .int_bounds(ty)
                    .ok_or_else(|| ice("div on non-int"))?;
                let is_min = self.b.ins().icmp_imm_s(IntCC::Equal, a, lo as i64);
                let is_m1 = self.b.ins().icmp_imm_s(IntCC::Equal, b, -1);
                let ovf = self.b.ins().band(is_min, is_m1);
                self.trap_if(ovf, TrapKind::Overflow)?;
                Ok(if op == Opcode::IdivChk {
                    self.b.ins().sdiv(a, b)
                } else {
                    self.b.ins().srem(a, b)
                })
            }
            _ => Ok(if op == Opcode::UdivChk {
                self.b.ins().udiv(a, b)
            } else {
                self.b.ins().urem(a, b)
            }),
        }
    }

    /// Saturating add/sub/mul (explicit source-level spelling; clamps,
    /// never traps).
    fn sat_arith(
        &mut self,
        op: Opcode,
        wa: wolf_wir::ir::Value,
        wb: wolf_wir::ir::Value,
        res: wolf_wir::ir::Value,
    ) -> Result<CValue, BackendError> {
        let ty = self.f.value_ty(res);
        let bits = self
            .m
            .types
            .int_bits(ty)
            .ok_or_else(|| ice("sat arith on non-int"))?;
        let a = self.scalar(wa)?;
        let b = self.scalar(wb)?;
        let (lo, hi) = self
            .m
            .types
            .int_bounds(ty)
            .ok_or_else(|| ice("bounds of non-int"))?;
        if bits < 64 {
            let xa = self.b.ins().sextend(ctypes::I64, a);
            let xb = self.b.ins().sextend(ctypes::I64, b);
            let r = match op {
                Opcode::IaddSat => self.b.ins().iadd(xa, xb),
                Opcode::IsubSat => self.b.ins().isub(xa, xb),
                _ => self.b.ins().imul(xa, xb),
            };
            let lov = self.b.ins().iconst(ctypes::I64, lo as i64);
            let hiv = self.b.ins().iconst(ctypes::I64, hi as i64);
            let r = self.b.ins().smax(r, lov);
            let r = self.b.ins().smin(r, hiv);
            return Ok(self.b.ins().ireduce(clif_int(bits), r));
        }
        let (r, o) = match op {
            Opcode::IaddSat => self.b.ins().sadd_overflow(a, b),
            Opcode::IsubSat => self.b.ins().ssub_overflow(a, b),
            _ => self.b.ins().smul_overflow(a, b),
        };
        // Overflow direction: for add/sub, a's sign picks the clamp;
        // for mul, the product's sign (sign(a) ^ sign(b)).
        let neg = match op {
            Opcode::ImulSat => {
                let sa = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, a, 0);
                let sb = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, b, 0);
                self.b.ins().bxor(sa, sb)
            }
            _ => self.b.ins().icmp_imm_s(IntCC::SignedLessThan, a, 0),
        };
        let lov = self.b.ins().iconst(ctypes::I64, lo as i64);
        let hiv = self.b.ins().iconst(ctypes::I64, hi as i64);
        let clamp = self.b.ins().select(neg, lov, hiv);
        Ok(self.b.ins().select(o, clamp, r))
    }

    /// Is `callee` one of this program's `export fn`s (kw02)? The
    /// convention is the callee's, so a wolf call into an export
    /// executes the C plan.
    fn callee_is_export(&self, callee: &str) -> bool {
        self.m.funcs.values().any(|f| f.export && f.name == callee)
    }

    /// kw11 (`[conc.mm.atomic.raw.3]`): one atomic intrinsic, in
    /// place. Cranelift's atomic instructions are sequentially
    /// consistent (its IR reference), so this tier implements every
    /// order by the strongest — correct for each, since seq_cst admits
    /// no outcome a weaker order forbids. The flags claim the
    /// alignment `[conc.mm.atomic.raw.4]` requires and not `notrap`.
    fn lower_atomic(
        &mut self,
        a: wolf_wir::AtomicIntrinsic,
        args: &[wolf_wir::ir::Value],
        results: &[wolf_wir::ir::Value],
    ) -> Result<(), BackendError> {
        use cranelift_codegen::ir::AtomicRmwOp;
        use wolf_wir::AtomicOp;
        let ty = match a.width {
            1 => ctypes::I8,
            2 => ctypes::I16,
            4 => ctypes::I32,
            _ => ctypes::I64,
        };
        let flags = MemFlagsData::new().with_aligned();
        let p = self.scalar(args[0])?;
        let mut rest = results;
        let out = match a.op {
            AtomicOp::Load => Some(self.b.ins().atomic_load(ty, flags, p)),
            AtomicOp::Store => {
                let v = self.scalar(args[1])?;
                self.b.ins().atomic_store(flags, v, p);
                None
            }
            AtomicOp::Cas => {
                let e = self.scalar(args[1])?;
                let n = self.scalar(args[2])?;
                Some(self.b.ins().atomic_cas(flags, p, e, n))
            }
            op => {
                let v = self.scalar(args[1])?;
                let rmw = match op {
                    AtomicOp::Swap => AtomicRmwOp::Xchg,
                    AtomicOp::Add => AtomicRmwOp::Add,
                    AtomicOp::Sub => AtomicRmwOp::Sub,
                    AtomicOp::And => AtomicRmwOp::And,
                    AtomicOp::Or => AtomicRmwOp::Or,
                    _ => AtomicRmwOp::Xor,
                };
                Some(self.b.ins().atomic_rmw(ty, flags, rmw, p, v))
            }
        };
        if let Some(r) = out {
            let (&first, tail) = results
                .split_first()
                .ok_or_else(|| ice("an atomic operation without a result"))?;
            self.vals.insert(first, Repr::Scalar(r));
            rest = tail;
        }
        for &tok in rest {
            self.vals.insert(tok, Repr::Token);
        }
        Ok(())
    }

    /// psABI §3.5.7: the banks are independent, but a mixed aggregate
    /// consumes both atomically or neither. The SSE save stride is 16,
    /// independent of the width of the value carried in that slot.
    fn lower_va_arg(
        &mut self,
        args: &[wolf_wir::ir::Value],
        results: &[wolf_wir::ir::Value],
    ) -> Result<(), BackendError> {
        check_variadic_target(self.om)?;
        let ty = self.f.value_ty(results[0]);
        if !matches!(
            self.m.types.get(ty),
            TypeData::I32 | TypeData::I64 | TypeData::F64 | TypeData::Ptr | TypeData::Agg(_)
        ) {
            return Err(nyi(
                "va_arg requires a promoted C scalar or C-compatible aggregate",
            ));
        }
        let layout = self.layout_of(ty)?;
        let list = self.scalar(args[0])?;
        let flags = MemFlagsData::trusted();
        let overflow = self.b.create_block();
        let done = self.b.create_block();
        let address = self.b.append_block_param(done, ctypes::I64);
        if let Some(units) = abi::classify_units(&self.m.types, ty) {
            let gp_slots = units.iter().filter(|u| u.class == RegClass::Int).count() as u32;
            let fp_slots = units.iter().filter(|u| u.class == RegClass::Sse).count() as u32;
            let gp = self.b.ins().load(ctypes::I32, flags, list, 0);
            let fp = self.b.ins().load(ctypes::I32, flags, list, 4);
            let gp_ok = self.b.ins().icmp_imm_s(
                IntCC::UnsignedLessThanOrEqual,
                gp,
                i64::from(abi::variadic::GP_LIMIT - gp_slots * 8),
            );
            let fp_ok = self.b.ins().icmp_imm_s(
                IntCC::UnsignedLessThanOrEqual,
                fp,
                i64::from(abi::variadic::FP_LIMIT - fp_slots * 16),
            );
            let fits = self.b.ins().band(gp_ok, fp_ok);
            let registers = self.b.create_block();
            self.b.ins().brif(fits, registers, &[], overflow, &[]);
            self.b.switch_to_block(registers);
            let save = self.b.ins().load(ctypes::I64, flags, list, 16);
            let gp_wide = self.b.ins().uextend(ctypes::I64, gp);
            let fp_wide = self.b.ins().uextend(ctypes::I64, fp);
            let gp_base = self.b.ins().iadd(save, gp_wide);
            let fp_base = self.b.ins().iadd(save, fp_wide);
            let slot = self.new_slot(Layout {
                size: layout.size.next_multiple_of(8).max(8),
                align: layout.align.max(8),
            });
            let value = self.b.ins().stack_addr(ctypes::I64, slot, 0);
            let (mut ngp, mut nfp) = (0, 0);
            for unit in &units {
                let (base, offset) = match unit.class {
                    RegClass::Int => {
                        let off = ngp * 8;
                        ngp += 1;
                        (gp_base, off)
                    }
                    RegClass::Sse => {
                        let off = nfp * 16;
                        nfp += 1;
                        (fp_base, off)
                    }
                };
                let part = self.b.ins().load(unit_clif_ty(unit), flags, base, offset);
                self.store_unit(value, unit, part);
            }
            if gp_slots != 0 {
                let next = self.b.ins().iadd_imm_s(gp, i64::from(gp_slots * 8));
                self.b.ins().store(flags, next, list, 0);
            }
            if fp_slots != 0 {
                let next = self.b.ins().iadd_imm_s(fp, i64::from(fp_slots * 16));
                self.b.ins().store(flags, next, list, 4);
            }
            self.b.ins().jump(done, &[BlockArg::Value(value)]);
        } else {
            self.b.ins().jump(overflow, &[]);
        }
        self.b.switch_to_block(overflow);
        let raw = self.b.ins().load(ctypes::I64, flags, list, 8);
        let align = layout.align.max(8);
        let rounded = self.b.ins().iadd_imm_s(raw, i64::from(align - 1));
        let mask = self.b.ins().iconst(ctypes::I64, -i64::from(align));
        let value = self.b.ins().band(rounded, mask);
        let next = self
            .b
            .ins()
            .iadd_imm_s(value, i64::from(layout.size.next_multiple_of(8)));
        self.b.ins().store(flags, next, list, 8);
        self.b.ins().jump(done, &[BlockArg::Value(value)]);
        self.b.switch_to_block(done);
        let result = if is_agg(self.m, ty) {
            // Materialize an SSA value, not a mutable view of C's save area.
            let slot = self.new_slot(layout);
            let copy = self.b.ins().stack_addr(ctypes::I64, slot, 0);
            self.copy_bytes(copy, address, layout.size);
            Repr::Addr(copy)
        } else {
            self.load_field(address, 0, ty)?
        };
        self.vals.insert(results[0], result);
        for &token in &results[1..] {
            self.vals.insert(token, Repr::Token);
        }
        Ok(())
    }

    /// Lower one call, executing the callee's ABI plan: tokens erased,
    /// small aggregates in registers, big ones through memory, error
    /// unions per `[abi.err.repr]`. The convention is the callee's:
    /// wolf-native for module functions and runtime shims, C SysV for
    /// the `c.*` import membrane.
    fn lower_call(
        &mut self,
        callee: &str,
        sig: SigId,
        args: &[wolf_wir::ir::Value],
        results: &[wolf_wir::ir::Value],
    ) -> Result<(), BackendError> {
        let cc = self.om.isa().default_call_conv();
        if let Some(fixed) = self.m.sigs[sig].variadic {
            check_variadic_target(self.om)?;
            let symbol = if let Some(entry) = self.funcs.get(callee) {
                entry.symbol.clone()
            } else if self.callee_is_export(callee) {
                callee.to_string()
            } else if let Some(symbol) = abi::c_import_symbol_in(self.m, callee) {
                symbol.to_string()
            } else {
                return Err(nyi(format!("variadic call to non-C function `{callee}`")));
            };
            let si = sig_info(self.m, sig, Conv::C, cc)?;
            let plan = abi::variadic::plan(&self.m.types, &self.m.sigs[sig], fixed as usize)
                .map_err(nyi)?;
            // A distinct private entry for every call shape avoids the module
            // declaration cache's single fixed signature per C symbol.
            let name = format!("_W.va.call.{}", self.variadic_shims.len());
            let fid = self
                .om
                .declare_function(&name, cranelift_module::Linkage::Import, &si.clif)
                .map_err(|e| ice(e.to_string()))?;
            self.variadic_shims.push(VariadicShim {
                fid,
                target: symbol,
                code: abi::variadic::x64::outgoing(plan.vector_registers),
                linkage: wolf_backend::Linkage::Local,
            });
            let fref = self.om.declare_func_in_func(fid, self.b.func);
            return self.emit_call_body(CallTarget::Direct(fref), &si, sig, args, results);
        }
        // Resolve the callee: module function (wolf conv), `c.*`
        // import (C membrane, unmangled libc symbol), or runtime shim.
        let (fref, conv) = if let Some(&(fr, conv)) = self.fref_cache.get(callee) {
            (fr, conv)
        } else if let Some(entry) = self.funcs.get(callee) {
            // kw02: a wolf call into an `export fn` crosses under the
            // C plan its definition was declared with (`[abi.c.export]`).
            let conv = if self.callee_is_export(callee) {
                Conv::C
            } else {
                Conv::Wolf
            };
            let fr = self.om.declare_func_in_func(entry.fid, self.b.func);
            self.fref_cache.insert(callee.to_string(), (fr, conv));
            (fr, conv)
        } else if self.callee_is_export(callee) {
            // kw02: an `export fn` defined in another of this program's
            // objects is imported by its plain symbol under the C plan.
            let si_c = sig_info(self.m, sig, Conv::C, cc)?;
            let fid = match self.imports.get(callee) {
                Some(&fid) => fid,
                None => {
                    let fid = self
                        .om
                        .declare_function(callee, cranelift_module::Linkage::Import, &si_c.clif)
                        .map_err(|e| ice(e.to_string()))?;
                    self.imports.insert(callee.to_string(), fid);
                    fid
                }
            };
            let fr = self.om.declare_func_in_func(fid, self.b.func);
            self.fref_cache.insert(callee.to_string(), (fr, Conv::C));
            (fr, Conv::C)
        } else if let Some(symbol) = abi::c_import_symbol_in(self.m, callee) {
            // The explicit membrane (D19): the WIR name's `c.`
            // namespace IS the seam; the linker symbol is the plain C
            // name, declared with the SysV plan. A function this
            // program defines is never a C import (wolf-lang#620: a
            // module named `c` spells its functions `c.go` too).
            let si_c = sig_info(self.m, sig, Conv::C, cc)?;
            let fid = match self.imports.get(callee) {
                Some(&fid) => fid,
                None => {
                    let fid = self
                        .om
                        .declare_function(symbol, cranelift_module::Linkage::Import, &si_c.clif)
                        .map_err(|e| ice(e.to_string()))?;
                    self.imports.insert(callee.to_string(), fid);
                    fid
                }
            };
            let fr = self.om.declare_func_in_func(fid, self.b.func);
            self.fref_cache.insert(callee.to_string(), (fr, Conv::C));
            (fr, Conv::C)
        } else if let Some((name, ..)) = RT_SYMBOLS.iter().find(|(n, ..)| n == &callee) {
            let fr = self.rt_ref(name)?;
            (fr, Conv::Wolf)
        } else if self.m.funcs.values().any(|f| f.name == callee) {
            // A wolf function OUTSIDE this object's subset (s31
            // per-module objects): import its mangled symbol under the
            // wolf plan. The mangle folds name + rendered sig +
            // convention version, so the import and the defining
            // object's export agree structurally (D7 at link grain).
            let si_w = sig_info(self.m, sig, Conv::Wolf, cc)?;
            let symbol = wolf_backend::mangle(self.m, callee, sig);
            let fid = match self.imports.get(callee) {
                Some(&fid) => fid,
                None => {
                    let fid = self
                        .om
                        .declare_function(&symbol, cranelift_module::Linkage::Import, &si_w.clif)
                        .map_err(|e| ice(e.to_string()))?;
                    self.imports.insert(callee.to_string(), fid);
                    fid
                }
            };
            let fr = self.om.declare_func_in_func(fid, self.b.func);
            self.fref_cache.insert(callee.to_string(), (fr, Conv::Wolf));
            (fr, Conv::Wolf)
        } else {
            return Err(nyi(format!(
                "call to external `@{callee}` (hand-declared externs beyond the \
                 modelled c.* set are c10's header importer)"
            )));
        };
        let si = sig_info(self.m, sig, conv, cc)?;
        self.emit_call_body(CallTarget::Direct(fref), &si, sig, args, results)
    }

    /// `call.ind` (s97): the callee is a VALUE — always wolf-conv (the
    /// C membrane is name-based; no fn value can name a `c.*` import,
    /// the lowerer refuses it upstream) — sharing the direct call's
    /// argument/result marshaling exactly (target 3's parity clause).
    fn lower_call_ind(
        &mut self,
        sig: SigId,
        args: &[wolf_wir::ir::Value],
        results: &[wolf_wir::ir::Value],
    ) -> Result<(), BackendError> {
        let cc = self.om.isa().default_call_conv();
        let si = sig_info(self.m, sig, Conv::Wolf, cc)?;
        let callee = self.scalar(args[0])?;
        let sigref = self.b.import_signature(si.clif.clone());
        self.emit_call_body(
            CallTarget::Indirect(sigref, callee),
            &si,
            sig,
            &args[1..],
            results,
        )
    }

    /// The shared back half of both call forms: marshal arguments per
    /// the ABI plan, emit the call, map declared results and trailing
    /// successor tokens.
    fn emit_call_body(
        &mut self,
        target: CallTarget,
        si: &SigInfo,
        sig: SigId,
        args: &[wolf_wir::ir::Value],
        results: &[wolf_wir::ir::Value],
    ) -> Result<(), BackendError> {
        let mut cargs: Vec<CValue> = Vec::new();
        // Out-slots for memory-class results, in result order. C puts
        // the pointer FIRST in the argument list; wolf trails it.
        let rdata = self.m.sigs[sig].results.clone();
        let mut sret_addrs: Vec<CValue> = Vec::new();
        for (&rty, slot) in rdata.iter().zip(si.rets.iter()) {
            if matches!(slot, RSlot::Sret { .. }) {
                let l = self.layout_of(rty)?;
                let s = self.new_slot(l);
                let a = self.b.ins().stack_addr(ctypes::I64, s, 0);
                sret_addrs.push(a);
                if si.sret_first {
                    cargs.push(a);
                }
            }
        }
        for (&av, slot) in args.iter().zip(si.params.iter()) {
            match slot {
                Slot::Token => {}
                Slot::Direct(_) => cargs.push(self.scalar(av)?),
                Slot::Split(units) => {
                    let units = units.clone();
                    let base = self.addr(av)?;
                    for u in &units {
                        let v = self.load_unit(base, u);
                        cargs.push(v);
                    }
                }
                // C byval struct args take the ADDRESS as the CLIF
                // value; Cranelift performs the stack copy.
                Slot::Ref | Slot::CStruct(_) => cargs.push(self.addr(av)?),
                // Apple-arm64 indirect (AAPCS64 B.4): the CALLEE owns
                // the pointed-at memory, so pass the address of a
                // fresh caller-side copy, never the SSA value's own
                // bytes.
                Slot::CIndirect(size) => {
                    let size = *size;
                    let src = self.addr(av)?;
                    let slot = self.new_slot(Layout { size, align: 8 });
                    let dst = self.b.ins().stack_addr(ctypes::I64, slot, 0);
                    self.copy_bytes(dst, src, size);
                    cargs.push(dst);
                }
            }
        }
        if !si.sret_first {
            cargs.extend(sret_addrs.iter().copied());
        }
        let call = match target {
            CallTarget::Direct(fref) => self.b.ins().call(fref, &cargs),
            CallTarget::Indirect(sigref, callee) => {
                self.b.ins().call_indirect(sigref, callee, &cargs)
            }
        };
        let rvals: Vec<CValue> = self.b.inst_results(call).to_vec();
        // Map declared results, then successor tokens (which trail the
        // declared results in WIR call results).
        let mut rv_i = 0usize;
        let mut sret_i = 0usize;
        let mut wr = results.iter();
        for slot in si.rets.iter() {
            let &rv = wr.next().ok_or_else(|| ice("call result arity"))?;
            match slot {
                RSlot::Token => {
                    self.vals.insert(rv, Repr::Token);
                }
                RSlot::Direct(_) => {
                    self.vals.insert(rv, Repr::Scalar(rvals[rv_i]));
                    rv_i += 1;
                }
                RSlot::Split(units) => {
                    // Materialize the returned units into a slot.
                    let units = units.clone();
                    let addr = self.units_slot_addr(&units);
                    for u in &units {
                        let v = rvals[rv_i];
                        rv_i += 1;
                        self.store_unit(addr, u, v);
                    }
                    self.vals.insert(rv, Repr::Addr(addr));
                }
                RSlot::Sret { disc_in_reg, c, .. } => {
                    if *disc_in_reg && !c {
                        // The register discriminant is advisory at
                        // this tier (the tag bytes are in the slot);
                        // consume its return position.
                        rv_i += 1;
                    }
                    self.vals.insert(rv, Repr::Addr(sret_addrs[sret_i]));
                    sret_i += 1;
                }
            }
        }
        for &rv in wr {
            // Successor tokens.
            self.vals.insert(rv, Repr::Token);
        }
        Ok(())
    }
}

/// The debug shape of a WIR scalar type (s30 variable DIEs); `None`
/// for aggregates, error unions, and tokens (no scalar DIE at v0).
fn debug_ty(m: &WModule, ty: TypeId) -> Option<DebugTy> {
    Some(match m.types.get(ty) {
        TypeData::Bool => DebugTy::Bool,
        TypeData::I8 => DebugTy::I8,
        TypeData::I16 => DebugTy::I16,
        TypeData::I32 => DebugTy::I32,
        TypeData::I64 => DebugTy::I64,
        TypeData::F32 => DebugTy::F32,
        TypeData::F64 => DebugTy::F64,
        TypeData::Ptr => DebugTy::Ptr,
        TypeData::Mem(_)
        | TypeData::Io
        | TypeData::Agg(_)
        | TypeData::Eu { .. }
        | TypeData::Union(_) => return None,
    })
}

fn clif_int(bits: u32) -> cranelift_codegen::ir::Type {
    match bits {
        8 => ctypes::I8,
        16 => ctypes::I16,
        32 => ctypes::I32,
        _ => ctypes::I64,
    }
}

fn trap_code(kind: TrapKind) -> i32 {
    use wolf_rt::native::trap_code as tc;
    match kind {
        TrapKind::Assert => tc::ASSERT,
        TrapKind::Overflow => tc::OVERFLOW,
        TrapKind::DivZero => tc::DIV_ZERO,
        TrapKind::Bounds => tc::BOUNDS,
        TrapKind::StaleHandle => tc::STALE_HANDLE,
    }
}

fn int_cc(cc: IntCc) -> IntCC {
    match cc {
        IntCc::Eq => IntCC::Equal,
        IntCc::Ne => IntCC::NotEqual,
        IntCc::Slt => IntCC::SignedLessThan,
        IntCc::Sle => IntCC::SignedLessThanOrEqual,
        IntCc::Sgt => IntCC::SignedGreaterThan,
        IntCc::Sge => IntCC::SignedGreaterThanOrEqual,
        IntCc::Ult => IntCC::UnsignedLessThan,
        IntCc::Ule => IntCC::UnsignedLessThanOrEqual,
        IntCc::Ugt => IntCC::UnsignedGreaterThan,
        IntCc::Uge => IntCC::UnsignedGreaterThanOrEqual,
    }
}

fn float_cc(cc: FloatCc) -> FloatCC {
    // WIR float compares are ORDERED (NaN compares false) for
    // eq/lt/le/gt/ge; `ne` is IEEE-754 `!=` — the NEGATION of `eq`, so
    // it is UNORDERED (true when either operand is NaN). `nan != nan`
    // must be true natively exactly as in the interpreter (issue #22,
    // wolf-std F-0027: `x != x` is *the* portable NaN test).
    match cc {
        FloatCc::Eq => FloatCC::Equal,
        FloatCc::Ne => FloatCC::NotEqual,
        FloatCc::Lt => FloatCC::LessThan,
        FloatCc::Le => FloatCC::LessThanOrEqual,
        FloatCc::Gt => FloatCC::GreaterThan,
        FloatCc::Ge => FloatCC::GreaterThanOrEqual,
    }
}
