# Changelog

## Unreleased

### The parse family is ruled, and its row is spelled `parse` (s143 — #265 closes)

`str.to_int()` was served by every implementation and ruled by none.
`[mem.str.parse]` and `[mem.str.to_int]` in spec/02 now say what the
four agreeing implementations had been doing: the family is one
method (`to_float` and `to_bool` do not exist — a float parse needs
the float text grammar ruled, which s38 never did, and a bool parse
answers nothing `s == "true"` does not); the surrounding
`[mem.str.ws]` run is ignored, exactly `trim`'s set; the value is an
optional sign and one or more ASCII digits that fit `int`, leading
zeros included; everything else is the row, never a trap — the empty
and blank text, a sign alone, an interior space, `_`, a radix prefix,
a fraction, an exponent, a non-ASCII digit, trailing text, and a
magnitude outside `int` (both machines agree since lupin 0.1.28,
wolf-interp#69; r12 brought those rows into the corpus battery).

The row is **`parse`** now, not `NotAnInt`. s142 copied the
interpreter's spelling, which made it the one CapCase payload-free
mark on the builtin surface, and the language's own pact says
otherwise: payload-free marks are lowercase words and CapCase names a
payload's type — W0603 warns a program that breaks it and lists
`parse` among its examples, the OS families spell their conditions
that way (`not_found`, `utf8`, `closed`), and the json builtins
already answer `parse` for exactly this condition. A spec that
enshrines a spelling the compiler's own lint calls a contradiction
was weighed against one wave's rename and lost. The cost, stated in
the clause: the interpreter's builtin and its two tests, the book's
chapter 1 (one printed block, two lines of prose, exercise 6-9 and its
solution — which also declares a CapCase mark W0603 flags), and here
the three implementations, their tests, the `str` completion's
signature and two witnesses. The compiler moves first: this release
raises `parse`; the mirror (wolf-interp) and the book move in the
same wave, and until the mirror lands `rows/to_int_parse.lu` is the
one corpus witness the two machines part on — `strings/to_int.lu`,
the battery, was rewritten not to spell the mark and stays
byte-identical under lupin 0.1.28. No interface moved.

### Two of the book's families stand on both machines (s143 — #268)

The book measured its samples against both machines for the first
time (wolf-book#9) and found 133 of 478 running on the interpreter
alone, 106 after r12. The two largest families by the compiler's own
refusal are gone.

**String interpolation of a non-primitive value** (10 samples, plus
the `{err}` that held `grammar/else_default.lu` at resolve). `{x}`
renders every value both machines can show the same way, and spec/10
`[type.interp.*]` now says how — the interpreter's rendering adopted
byte for byte: `()`; a tuple `(a, b)`; a struct `Doc { title:
regions, words: 900 }` (a `str` inside a composite is its bytes,
unquoted); a `List` `[1, 2]`; an enum variant qualified,
`Shape.Line(4)`; a caught row as its tag's name with its payload,
`too_short`, `BadDigit(q, 1)`; a `!T` as its ok payload or its row,
so chapter 5's `{popped}` prints without an `else`; an exit reason
`normal(1540)`, `error(Corrupt)`, `killed`, `cancelled`,
`fault(bounds)`. A format spec on a composite refuses, never silently
ignores. What has no promised rendering — a channel, a proc, a scope,
a region, a pointer, a fn, a `dyn`, the shared tier — refuses by
name (`[type.interp.none]`). Both tiers render: the checked machine
type-directs its renderer (a positional value is a tuple or a struct
by its type), the native tier walks the value's shape and reads a
tag's name through a per-module tag table synthesized after every
body has interned its tags (`wolf.tag_name`, one compare chain — a
site cannot switch over tags a later function is the first to
raise). One runtime fact moved with it: a proc's `int` result now
rides the exit reason (`__wolf_rt_task_value`, the entry shim hands it
over), so `{reason}` reads `normal(1540)` where the native tier said
`normal(0)` for every ok body. Witnesses: `strings/interp_values.lu`
(both tiers), `conc/reason_interp.lu` (native; the checked lane
refuses procs), `grammar/else_default.lu` (runs now), each
byte-identical under lupin 0.1.28. Predicted before the gauntlet:
sema's hole check, the checked renderer, the lowerer's print and
value-position paths, no runtime surface beyond the value stash;
measured: that, plus the tag table and the stash.

The samples, measured at this tree against lupin 0.1.28: four of the
ten close (ch05 §5.1's `{last}` block and exercise 5-1; ch14's and
ch15's opening `{reason}` blocks), and the other six now refuse or
reject for a reason that is not interpolation — ch05's `pop` on an
empty list (`none` here, `trap(bounds)` on the interpreter: `pop`'s
empty case is unruled, filed), ch10's and ch14-10's `channel[str]`
(payloads beyond one word), ch12's and ch14's `channel[Doc]` (E1102,
a static rejection), ch15's self-`link()` (E0402) and `?` in a fn
that cannot fail (E0604). The book's runner flips the four at its
next pin bump.

**The iteration protocol (for-trait wiring)** (7 samples). Not the
protocol at all: every one was a worker taking its inbox as a
PARAMETER, and `channel[T]` in a signature position — a parameter, a
field — stayed opaque where the expression `channel[T](n)` typed
`Chan`, so `for d in inbox` refused with the protocol's name. One arm
in signature elaboration types it; `conc/chan_param_for.lu` is the
witness (a plain call, a proc body, a struct field), byte-identical
under lupin 0.1.28. Predicted: one arm and no lowering change;
measured: exactly that. Five of the seven close (exercises 14-6,
14-7, 14-9; ch17's two `worker` blocks — the first prints its bug on
this machine's schedule, as the chapter says it may); ch14's shelver
and exercise 14-10 now stop at `channel[Doc]` (E1102) and
`channel[str]` (payloads beyond one word) — interpolation's leftovers,
not iteration's.

### The ratchet that can tell composition from regression (s143 — #270 closes)

`cargo xtask bench-gates` gated the corpus's IR volume as one number,
the geomean of `IR(midend) / IR(naive)` over every run-phase file the
release tier emits, ratcheted against a ceiling. Nine consecutive
re-records of that ceiling (s86 through s142) were population changes
— a witness entering at 2.3 because the mid-end inlines a twenty-call
helper — and not one was a regression; the gate fired on exactly the
event it was not meant to catch and never, in that history, on the
one it was. The gate is per file now: `bench/ir-volume.json` records
a ratio per corpus path with its two instruction counts, measured on
the linux/x86-64 lane like every gate number; an existing path whose
ratio rises past its record by more than the relative slack (5%) reds
by name with its counts; a path the record has never seen is reported
beside the table and never reds on its own; a recorded path the run
cannot emit is reported as conservatism. `cargo xtask bench-gates
--record` writes the table from the rig. The corpus geomean is still
printed on every run as the number the contract's 0.50 target is
stated against, but it is a reference line, not a verdict — its key
is `corpus_geomean_reference`, and its history in `bench/gates.json`
ends with the entry that says so. The kernel suite keeps its geomean
ceiling: the manifest fixes that population, so a move there is a
lowering change.

## 0.2.8 — 2026-09-09

THE FIRST CHAPTER COMPILES, AND THE SYSCALL GOES FIRST. This release
answers a learner at one end of the language and a server author at the
other.

The book's chapter 1 reaches for `row.to_int()` twice, in the cold open
and again in exercise 1-10. The compiler had refused that call since the
builtin set was drawn, so the first non-trivial string method a learner
met was also the first program that did not build. `str.to_int() -> int
! {NotAnInt}` is in the set on both tiers now, spelled the way the
reference interpreter has always spelled it, and text it cannot read is
the `NotAnInt` row rather than a trap. When a method really is outside
the set, the refusal says which method: ``this `str` method, `to_int`,
is outside the builtin set``, where it used to give a byte span and
leave the reader counting.

At the other end, a program that reads a socket the kernel has already
made ready now makes the read and waits for nothing. Every parking call
in the net family used to go to the reactor thread first (a waiter cell,
a lock handshake, a `kevent` to arm it, a `kevent` to wake it, a
condvar, two context switches) and only then make the syscall, on a
socket the program's own `net_wait` had just reported ready. lobo, the
web server written in wolf, paid that round-trip three times per
request. The same lobo source was built against the old compiler and
against this one and measured on a single macOS arm64 box in one session
six minutes apart: 53.3 µs per request before and 21.2 after, 18,766
req/s to 47,162, and the share of the serving thread parked in a condvar
fell from 40.7% to 1.1%. The `wolf-reactor` thread never started at all
in the second run, because the reactor is lazy and a serving loop that
never parks never asks for one.

Three smaller changes come with that one. `TCP_NODELAY` is on for every
TCP stream the runtime hands out, accepted or dialed, so the common
shape (answer with a head, then a body, two writes, the second small) no
longer stalls 40 ms behind the peer's delayed ACK; `net_nodelay(fd, on)`
sets it either way, and `net_writev` sends the head and the body as one
gathered write. `fs_fstat(fd)` answers `[kind, size, modified_ms]` from
one metadata read on an open handle, where a static-file server
previously paid four separate path stats for the same facts. And a
write's budget now covers its whole drain, so a `net_deadline` armed on
a stream bounds a large `net_write` to a peer that stopped reading;
before this release that drain rode a blocking syscall and ran past the
budget.

Every call that existed at 0.2.7 keeps its signature and its error rows,
so a program written against it compiles and behaves the same way here.

### The first chapter compiles (s142 — #263 closes)

`str.to_int() -> int ! {NotAnInt}` joins the builtin set on both
tiers. The book's chapter 1 reaches for it twice (`row.to_int() else
0` in the cold open and in exercise 1-10), the reference interpreter
has served it all along, and the compiler had refused it since the
set was drawn at s37 — so the first non-trivial string call a
learner meets was the first program that did not build. The row is
the one lupin 0.1.27 answers, spelled as it spells it. What the parse
accepts: the surrounding `[mem.str.ws]` run is ignored (ASCII and the
non-ASCII separators alike, so `s.to_int()` and `s.trim().to_int()`
are one value), then an optionally signed (`+`/`-`) run of ASCII
digits that fits `int`, leading zeros included. Everything else is
`NotAnInt`, never a trap — the empty and the blank text, a word, an
interior space, `_`, a fraction, a radix prefix, a non-ASCII digit, a
lone sign, trailing letters. A magnitude outside `int` is the row on
both wolf tiers: there is no `int` the text names, and checked
arithmetic never answers with a quiet wrap. The reference interpreter
at this pin parses that one input to a wider integer and prints it as
an `int`; filed as wolffe-lang/wolf-interp#69, and the corpus witness
(`strings/to_int.lu`, row for row with lupin) leaves that input to the
crate tests. `to_float`/`to_bool` are NOT added: neither the spec nor
lupin defines them, and the family is mirrored, not invented. The
spec is silent on the row — filed as #265; the clause will say what
this entry says.

And the refusal names the method. `wolf build: cannot compile this yet
— this `str` method (outside the builtin set) @155..167` made the
maintainer count bytes to learn that the span was `row.to_int()`; it
now reads ``this `str` method, `to_int`, is outside the builtin set``,
on the stderr line and in the observation record's
`x-unsupported-construct`, for every method outside the set.

### One stat on the handle (s142 — #261 closes)

`fs_fstat(fd: int) -> List[int] ! {not_found, denied, io}` —
`[kind, size, modified_ms]` from ONE metadata read on the open handle
(`[os.fs.fstat]`). lobo ws23 found a static-file request paying four
path stats (`fs_is_dir`, `fs_is_file`, `fs_size`, `fs_modified_ms`,
~0.5 µs each on macOS arm64) where nginx pays one `open` and one
`fstat`; the router's reorder took one away and the last two could not
move because nothing answered off the handle the request was about to
read. `kind` is 0 for a regular file, 1 for a directory, 2 for
anything else; `size` and `modified_ms` are `fs_size`'s and
`fs_modified_ms`'s words in their units. The rows are the path stat's;
a closed or forged handle is `io`. Every tier-1 host, every tier — the
checked machine reads the same handle metadata the runtime does. The
one host difference is `fs_open`'s and is stated in the clause rather
than papered: unix opens a directory read-only (`kind` 1 is
reachable); windows refuses the open first (`denied`), so there a
server still classifies directories by path. Witness
`corpus/fs/fstat.lu`; the directory case is the crate tests'
(`#[cfg(unix)]`). The `O_NONBLOCK` open mode #261 mentions as a
nice-to-have is not in this entry.

### The syscall goes first (s141 — #257 closes)

A ready socket is answered by the syscall alone. Every parking call
in the net family (`net_accept`, `net_read`/`net_read_bytes`,
`net_write`/`net_write_bytes`, and the new `net_writev`) now tries the
syscall before it waits, and waits only when the kernel answers that
it would block. Until now each of them parked on the `wolf-reactor`
thread first (a waiter cell, a lock handshake, a `kevent` to arm, a
`kevent` to wake, a condvar, two context switches) and only then ran
the syscall, on sockets the program's own `net_wait` had just
reported ready. lobo ws22 profiled it with `sample(1)` on macOS arm64
(one serving process, keepalive): 63 µs per request against nginx's
19 on the same box, and about 30 of the 63 was that round-trip, taken
three times per request, 37.8% of the serving thread parked in a
condvar, 8% in its own `kevent`, the syscalls themselves 12%. Now a
read on a ready socket touches no lock handshake, no condvar and no
`kevent`; a park happens on `WouldBlock` only, and the retry after
the wake goes against the budget computed when the call began, never
a fresh one (`[os.net.accept]`'s rule, now every call's).

Nothing a program can observe in the rows moves; the cost does.
Predicted first, from ws22's arithmetic: about 30 of 63 µs. Measured
the way the finding was made: lobo (trunk `023ec64`, its source
untouched) built once at the v0.2.6 pin and once at this branch
(`aec4d9f`, a dev-stamped `.wolf-bin` staged the way lobo's CI stages
one), `tools/lobo-parity` (five interleaved pairs, `ab -t 5 -c 32`
over four generators, both shapes, both cells) and the same
`sample(1)` for ten seconds at one millisecond on the N=1 serving
process under `ab -k -t 20 -c 32`, one session, same box, six minutes
apart. nomad-1, Apple M5 Pro, 18 cpus, macOS 26.4.1, 2026-09-09
01:15Z–01:21Z:

| | before (wolf 0.2.6, pin 398e5f5) | after (this branch) |
|---|---|---|
| N=1 keepalive, lobo req/s (median), µs per request | 18,766, 53.3 µs | 47,162, 21.2 µs |
| N=1 keepalive, nginx ÷ lobo, median [min, max] | 3.799x [3.612, 4.075] | 1.579x [0.943, 1.792] |
| N=1 close, nginx ÷ lobo | 2.273x [2.165, 2.687] | 1.116x [0.988, 1.324] |
| N=18 keepalive, nginx ÷ lobo | 2.446x [2.347, 2.536] | 1.597x [1.581, 1.602] |
| N=18 close, nginx ÷ lobo | 1.118x [1.072, 1.166] | 1.055x [1.045, 1.101] |
| main thread in `__psynch_cvwait` (of its samples) | 3,396 of 8,352, 40.7% | 95 of 8,450, 1.1% |
| main thread in its own `kevent` | 610, 7.3% | 0 |
| threads the process ran | main, `wolf-reactor`, `wolf-signal` | main, `wolf-signal` |

Thirty-two microseconds per request on the N=1 keepalive cell, and
the `wolf-reactor` thread never started: the reactor is lazy, and a
serving loop that never parks never asks for it. What remains at 21 µs
is the syscalls (`open` 33% of the thread, `sendto` 17%, `recvfrom`
5%) and lobo's own three stats, which are lobo's (wolffe-lang/lobo#3,
ws23). Both sets are marked REFUSED by the tool's quiet-rig rule,
load(1m) 5.05 and 5.10 before each set with two sibling lanes on the
box (the bar's rule is 3.0; the after set also saw nginx spread 1.86x
on the N=1 keepalive shape), so they are not an entry in lobo's
ledger; they are the same box, the same hour, the same load, and the
ratio between them is the number this entry claims. The bar itself
(1.10x) is not met on the keepalive shape and this entry does not say
it is; a quiet set and the linux runner's number are lobo's to take
at the next pin that carries this change (v0.2.7 shipped while this
branch was still red on the runner, and does not).

One consequence is stated because a program can build on it: a
write's budget covers the whole drain. A body larger than the send
buffer leaves in chunks with a park between them, each park against
the call's budget, so a `net_deadline` armed on a stream now bounds a
large `net_write` to a peer that stopped reading; before, the drain
rode a blocking syscall past any budget. The row is `net_write`'s `io`
(its `timeout` coarsened, as the call declares), and the bytes the
kernel took before the budget fired have been sent.

Every socket the runtime holds lives non-blocking on the three reactor
hosts (linux, macOS, windows), a listener since 0.2.6 and a stream
since now, the flag set on acquisition rather than trusted to a
kernel's inheritance rule; a tier-2 host with no reactor keeps the
blocking syscall. The checked machine answers the same rows through
its own blocking sockets and their timeouts; the reference interpreter
polls first and waits on not-yet already, so the differential is the
proof. Spec: `[os.net.io]` (new), one sentence in `[os.net.accept]`
(the accepted stream is under the same posture, its rows unchanged).
Witness: `corpus/net/syscall_first.lu`, a read on a socket `net_wait`
reported answers, and a budgeted large write to a peer that never
reads returns at the budget with the row, not a park. The runtime's
tests count this thread's parks: an accept with a connection queued,
a write into an empty buffer and a read of bytes `poll(2)` reported
park zero times.

### `TCP_NODELAY` by default, and a gathered write (s141 — #254 closes)

Nagle's algorithm is off on every TCP stream the runtime hands out,
accepted or dialed, and `net_nodelay(fd, on) -> () ! {io}` sets it
either way. lobo measured the reason on the CI runner (linux x86-64):
a server that answers with a head and then a body, two writes, the
second small, stalled behind the peer's delayed ACK, 40 ms per
request, 780 req/s against nginx's 84,493 on the same runner, a 108x
gap for a program that did nothing wrong. The posture Go, nginx and
node take is the one that makes a naive two-write server correct
without knowing this entry exists. lobo has carried `tcp_nodelay` as
`planned` in its directive table for twenty waves because nothing in
the language could honor it; now both directions can be. It is one
option, named, with a stated default and its rows, not a generic
`setsockopt`, and none is planned; the clause says what else that
shape admits (a keepalive switch) and what it does not (linger,
buffer sizes).

`net_writev(fd, parts: List[List[byte]]) -> () ! {closed, io}` sends
every part in order as one write, `writev(2)` on unix and `WSASend` on
windows, gathered by the kernel and never copied by the runtime, with
`net_write`'s rows exactly and the same posture: the syscall first, a
park on would-block against the budget computed at entry, the drain
resumed from the byte the kernel stopped at in whichever part it
stopped in. Empty parts send nothing. A head and a body now leave in
one syscall, and where two writes were two segments, one. The macOS
numbers above were taken with lobo still writing twice, so nothing in
them is this entry's; the linux number is, and it is lobo's to take.

Both are served on every tier-1 host and every tier: native, release,
and the checked machine, whose streams enter its table under the same
default and whose gather is the same std vectored write driven to
completion. The reference interpreter's mirror is filed with the
clause text as wolffe-lang/wolf-interp#67. Spec: `[os.net.nodelay]`,
`[os.net.writev]` (new). Witnesses: `corpus/net/nodelay.lu`,
`corpus/net/writev_gather.lu`.

## 0.2.7 — 2026-09-08

THE PAIRING IS CHECKED WHERE IT ROTS. `wolf --version` prints the lupin
release this compiler is differentially tested against, and that line has
one job: to be true. It stopped being true when the interpreter published
0.1.27 and `crates/wolf_driver/PAIRING` still said 0.1.26. The gate that
exists to catch exactly that did catch it, on every machine in the house
at once and on none of the six CI jobs, because the comparison needs a
lupin binary and no runner had one. Two lanes tripped over it
independently, in work that had nothing to do with the pairing. This
release re-measures the pairing against the interpreter as released, and
gives the linux CI job the pinned lupin release archive so the same
comparison runs on the runner, so the sentence `wolf --version` prints
about the reference interpreter is checked somewhere that is watching.

The tag carries two more pieces of work that a reader meets sooner than
the pairing. `wolf --help` answers a stranger now, with per-verb help, a
man page and shell completions in the archive, and three other things a
newcomer hit in their first five minutes are fixed beside it. And the
seven `[sched.*]` spec anchors are published, so a program may cite them.
Both are below.

### The one gate CI could not run (r10 — #253 closes)

The pairing test compares this build against the lupin release `PAIRING`
names, and per D57 it compares both halves of that claim: the version
always, and the declared conformance pin whenever the sibling is a release
build. It fires only where a sibling lupin binary exists. Runners had
none, so the single gate that compares the two implementations was the
single gate CI never ran, and rot in it arrived as a laptop surprise in
whatever sprint happened to be next.

A sibling checkout on the runner would be a campaign: a second repository,
its own toolchain pin, a build, and a cache to keep that build from being
paid every run. The pinned release archive is none of those things. It is
public, it is already built, it is two megabytes, and per D57 it is a
release build that prints its bare version, so both halves compare on the
runner exactly as they do beside a sibling checkout at the tag. The whole
of it is one step and two environment variables in a job that already
exists, and it cost one second of runner time on the run that proved it.

The step reads the version out of `PAIRING` rather than repeating it, so
it tracks the stamp and cannot go stale on its own; a stamp naming a
release that does not exist reds by name. The digest is never typed: it is
read off the release asset GitHub reports and checked against the bytes
that arrived. The fetch runs before the gauntlet with `LUPIN` set for the
job, so `cargo xtask ci`'s own test step is what compares, and the runner
runs the same gauntlet a laptop does rather than a second copy of one
test. `WOLF_PAIRING_REQUIRE_SIBLING` closes the way this could have been
worthless: an absent sibling is honest on a bare box and dishonest on a
runner that just downloaded one, so the test now decides which it is
looking at, and a failed download reds as a failed download instead of
arriving as a green skip.

### The pairing

The pairing moves to lupin 0.1.27 (pin `6ade878`), stamped against the
release as published rather than against its successor on the
interpreter's trunk: a release's pin is part of what that release is
(D57), and the release that exists declares `6ade878`, wolf-lang's own
v0.2.5 tag. The interpreter's trunk has since re-vendored to v0.2.6's
`398e5f5`, which is advance notice for the next stamp and not rot in this
one, exactly as the dev-sibling skip says out loud.

Re-measured bare, no `LUPIN=` override, against a clean checkout of the
`v0.1.27` tag built by itself, so D57's second half applied and version
and pin were both compared and both matched. The ritual over 508 files:
checked 269 agreements / 124 completeness notes / 2 soundness / 108
unsupported / 8 hard, native 290 / 124 / 0 / 90 / 5. Against v0.2.6's
measurement over the same 508 files (266 / 124 / 2 / 111 / 8 and 287 /
124 / 0 / 93 / 5) every row is flat but one, and that one is the same
move twice: agreements +3 and unsupported −3 on both lanes, with run-rung
coverage carrying the same fact (the interpreter executed 360 entries at
the previous stamp and executes 363 now). It is the sibling growing, not
this compiler. The three constructs the interpreter landed in its mirror
at 0.1.27 turn `unsupported` at resolve into `exit(0)` at run on its side,
so `corpus/net/reuse_port.lu`, `corpus/net/wait_readiness.lu` and
`corpus/os/cpus.lu` leave the conservatism ledger and enter agreements.

Hard divergence is flat at 8 and 5 and decomposes with nothing left over,
over the same two terms as v0.2.6: six (five) `Diag` rows are #167's
warning-channel asymmetry, the interpreter emitting no warnings at all,
and the two soundness findings are #168's float-cast twins, where wolfc's
own checked lane exits 0 and the native lanes trap. Every surviving row is
a filed issue older than this release. No class opened at this pin and
none was deferred. The interpreter's `[os.net.accept]` evaluation, landed
after the tag, moves no row here: `corpus/net/accept_race.lu` needs an
inherited listener the interpreter is never handed, so it stays
unsupported on both lanes as it was at v0.2.6.

One note for anyone whose gauntlet reds at this stamp with a pin
complaint. The suffix a lupin build prints is written by its build script,
and a build script's output is cached: a tree built once at the tag and
then moved off it can keep printing the bare version while compiling a
newer pin, which reads to this gate as a release build declaring the wrong
pin. That is the sibling's stamp being stale, not `PAIRING`. Rebuilding
the sibling with its build script forced to re-run (`touch build.rs`)
restores the `+dev` suffix and the row goes quiet.

### The stranger's five minutes (s140 — #249, #250, #251, #252 close)

`wolf --help` was one line of twenty-three pipe-separated verbs, printed
to stderr, exiting 2. Asking the compiler what it does was an error, and
the answer gave no verb's arguments. It exits 0 on stdout now, so it
pipes into a pager, and it says what wolf is, gives a first program that
can be pasted and run, points at the documentation, and states the D32
rule that a directory is a module before that rule bites. Every verb the
overview lists answers `--help` itself, both `wolf build --help` and
`wolf help build`. A wrong flag prints the same usage line `--help`
shows, because both read one table, so the two cannot drift apart.
`wolf run prog.lu --help` still belongs to the program: everything after
the entry file is the program's argv.

`cargo xtask dist` stages a man page and shell completions into the
archive the formula and the PKGBUILD unpack (`wolf.1`, `wolf.bash`,
`_wolf`, `wolf.fish`). The binary that just built emits them from the
same table the help reads, so a packager has something to install and it
describes the verbs that exist.

The compile-failure footer names the code the reader was shown (#249).
It said `wolf --explain E0201` whatever error had been reported, so a
package that failed with E0302 sent its author to a different error's
entry, on the second command a newcomer types.

A `use std.…` that misses says that no standard library is configured,
that the standard library is a separate release (wolf-std), and that
`--std-root <dir>` or `WOLF_STD` points wolf at a checkout (#251).
Reaching that miss is itself the evidence that no root is set. With a
root configured the note stays silent, and a test asserts that silence,
because the sentence would be false.

The README's links are absolute (#252). `dist` stages that file and the
packages install it to `share/doc/wolf/README.md`, where nine
repo-relative links resolved to nothing. A gauntlet check reads the
shipped file, and a second check asserts the README still links
somewhere, so an empty scan cannot pass the first one.

### The schedule is named (s139 — #246 closes)

`spec/07-schedule-points.md` is normative for the `sched` namespace, and
that namespace is registered under `[conf.anchor.ns]`. Its seven anchors
(`sched.engine`, `sched.ev1`, `sched.flags`, `sched.point.hook`,
`sched.point.set`, `sched.seed`, `sched.stable`) are published in
`spec/anchors.json`, taking it from 424 to 431. The change is additive:
nothing was renumbered or dropped, because none of the seven had ever
been published for `[conf.anchor.stable]` to pin.

Before this the document declared those anchors and no registry carried
them, so `[conf.tag.valid]` rejected every citation of them while the
scheduler implemented the clauses: `wolf_rt::task::det` carries
`[sched.seed]`'s packed seed and `[sched.stable]`'s append rule,
`::task::hooks` the kind table, `::reactor` and `::net` the
`io.arrive`/`timer.fire` appends. `corpus/test/conc_schedules_test.lu`
cites `sched.stable` in a `conforms:` tag, the first citation the
admission makes legal.

The cause was four hand-copied namespace lists that all omitted document
07. They are one `NS_OWNERS` table now, and the admission gate reads both
directions: a registered namespace must publish anchors, and a spec
document may not declare a namespace nobody registered. The second guard
scans `spec/` on disk, because the defect was a document that nothing in
the tooling listed.

## 0.2.6 — 2026-09-06

THE ACCEPT IS FAIR. A hand that loses an accept race now costs you a
budget instead of a process. Start several hands on one listening
socket (the prefork shape 0.2.5 made writable, whether they share the
address with `reuse_port` or inherit one listener from a parent) and
each arriving connection wakes more than one of them while exactly one
takes it. Until now the losers then called a *blocking* `accept(2)`
with their deadline already spent, and parked in the kernel until the
next connection arrived: microseconds on a busy server, and on a quiet
one, never. Alive at 0.0% CPU, answering no control verb, reaped by
their master as dead. Now a loser comes back inside the budget its
`net_deadline` armed when the call began, never a fresh one, so the
most a lost race can cost a serving loop is that budget, and N
processes may share one listener without one of them going silent.
Nothing in `net_accept`'s signature or its rows moved, so a program
written against 0.2.5 runs unchanged, and the shape 0.2.5 invited you
to write now survives an idle Sunday.

### A lost race is not an answer, because it carries none (s138 — #242 closes)

A listener lives non-blocking under the runtime now. The take after
a wake finds the connection or finds nothing, and a hand that finds
nothing waits again against the budget its `net_deadline` already armed.
With a budget armed, `net_accept` returns within it; without one it
parks in the runtime's wait, where kill teardown reaches it, and not in
the kernel's, where nothing does.

Nothing new appears in the rows. From the program's side a lost race is
indistinguishable from the connection never having arrived, the same
thing a single hand sees when a peer aborts between the wake and the
take. So it is not a new row (a `net_accept(l)?` with no deadline
armed would then fail on a stranger's reset, and a `timeout` before any
deadline would lie about a clock), not an empty handle, and never a
trap. A loop that multiplexes a shared listener with `net_wait` and must
keep serving its other sockets arms a short budget on the listener; that
budget is the most a lost race can cost it. The accepted stream is
blocking whatever the listener's mode (BSD kernels hand the flag down,
linux does not, and the runtime makes it one posture), so reads and
writes are as before.

Measured on macOS arm64, three hands free-for-all on one inherited
listener, `ab -n 6000 -c 32`, one connection per request: about 30,000
req/s before and after (29.7–30.8k parking, 29.7–33.1k fixed; one hand
serves 23–28k, six serve 25–30k, the loopback client being the ceiling).
Under load the fix costs nothing and buys nothing, because the next
connection unparks every loser microseconds later. It buys the quiet
server, and it deletes a workaround downstream: the ten-millisecond
accept turn lobo shipped to avoid the park cost it 11,622 req/s against
17,347, and comes out now.

Spec: `[os.net.accept]` (new), with a sentence each in `[os.net.wait]`
(readiness is not exclusivity) and `[os.proc.inherit]`. Witness:
`corpus/net/accept_race.lu`, two hands on one inherited listener, one
connection, both hands return; before this fix it reports
`loser_returned false` in twelve seconds instead of hanging. Windows
and the checked machine refuse the inherit set, so the witness is
vacuous there.

### The blast-radius bound holds (s138 — #243 closes)

A keyword where a parameter name goes is one report, not two.
`fn f(true, x: int)` said "`true` is a reserved keyword, so it cannot
name a parameter" and then, on the same token, "expected `:` and a type
after the parameter name". The second line contradicted the first and
told you nothing the first had not; now the type is asked for only when
a `:` says you meant a parameter there (`fn f(true: int)` is still one
report, the name), and a real name with no type keeps its report
(`fn f(x)` still asks for one).

That is all of #243. The nightly's blast-radius sweep (every corpus
file, three hundred single-token mutations each, at most five cascade
diagnostics for a mutation that re-keys structure) had been red since
the s137 corpus adds: turning the `=` of `let a =
net_listen_with("127.0.0.1:0", true, 16) else |e| match e {` into `fn`
drew six. Reduced to the smallest program that draws six and read one
by one, five of them are one per enclosing tier (the binding, the run
of non-parameters, the parameter, the phantom header's end, the arm
list read as its body) and the sixth was the parameter reported twice.
The recovery is fixed; the bound and its rationale do not move. The
counter-example is pinned deterministically beside #20's and #109's.

### The letters

`[conf.anchor.ns]` admits the namespaces it publishes (#239). The
clause that registers anchor namespaces named seven of them while the
spec published anchors in eleven, and had done since `diag` (s67), `ct`
(s112), `type` (s113) and `os` (s114) each went normative without an
append. At this release that was 71 published anchors outside the
clause's own letter, every one of them a clause `[conf.tag.valid]`
made a CI failure to cite. Inside this repository the gap was invisible,
because the extractor is the permissive side and admitted all eleven; it
only ever announced itself downstream, in a rig that mirrors the letter
and therefore *rejected* tags that ought to be legal: wolf-std's F-0099,
where six witnesses for `[os.net.unix]` could not name the clause they
conform to. The clause caught up, and not one anchor moved. Moving them
was never the option: `[conf.anchor.stable]` forbids renumbering a
published anchor, and the citations that would have had to move number
3,273 across nine repositories against the one paragraph an append
costs. The append is additive on #120's precedent, with the reason
written into the clause instead of a commit message, and a new
`[conf.anchor.ns.admit]` says how the next namespace is admitted: in one
change, clause and tooling together, on every implementation track. Three
tests in `cargo xtask ci` now hold the two halves equal, so the fifth
recurrence fails a gauntlet instead of a downstream rig. Anchors 423 →
424.

The pairing holds at lupin 0.1.26 (pin `982f857`), unchanged from
v0.2.5, because the interpreter published no release between the two
tags, and the first pairing in this series that needed no re-stamp at
all. Re-run at this head against the v0.1.26 release build (bare, no
`LUPIN=` override; D57's second half applies, so version and pin were
both compared and both matched). The ritual over 508 files: checked
266 agreements / 124 completeness notes / 2 soundness / 111
unsupported / 8 hard, native 287 / 124 / 0 / 93 / 5. Hard
divergence is flat against v0.2.5's 8 and 5, and decomposes with nothing
left over: six (five) `Diag` rows are #167's warning-channel asymmetry,
lupin emitting no warnings at all, and the two soundness findings are
#168's float-cast twins, where wolfc's own checked lane exits 0 and the
native lanes trap. Every surviving row is a filed issue older than this
release, and no class opened at this pin. The one corpus file this
release adds, `corpus/net/accept_race.lu`, is unsupported on both lanes
(the interpreter is handed no listener to inherit), which is where the
507 → 508 and the two `unsupported` counts moved.

## 0.2.5 — 2026-09-04

THE SERVER HAS CORES. A loop that waits costs about 37x less than a
loop that looks. Hold one connection open and idle for five seconds,
and v0.2.4 gave a single-threaded server no way to spend less than
50.2 M cycles and 1,378 involuntary context switches doing nothing
at all. With no call that waits on more than one socket, such a server
had to time-slice: a short deadline on the listener, another on every
open connection, around and around, waking up to learn that nothing had
happened. `net_wait(fds, deadline_ms)` asks the kernel about the whole
set at once and sleeps until a socket actually speaks. The same five
idle seconds now cost 1.3 M cycles and 5 switches, both figures net
of a program that only sleeps, which is the floor either way. That is
the difference between a server that burns a core to sit still and one
that costs nothing to sit still, and if you are writing a serving loop
in wolf it is the one call to reach for first.

The other half of a server is the other cores. Several processes can
hold one listening address now, with `net_listen_with(addr, reuse_port,
backlog)`, or a parent can open the listener and hand it down, with
`os_spawn_with(exe, args, inherit)` passing real descriptors and
`net_adopt_listener(fd)` picking one up on the other side. `os_cpus()`
says how many hands to start: the number of cores this process may
actually be scheduled on, which inside a container is the quota, not the
hardware. A prefork server is a program you can write in wolf today.

### A listener that hands can share (s137 — #234 and #235 close)

`net_listen_with` is `net_listen` with the two options a server needs.
`reuse_port` sets `SO_REUSEPORT` before the bind, the only place it
works, so several listeners can hold one address at once, in one
process or across many, and the old hand can leave without the port ever
going free. `backlog` is the `listen(2)` queue hint (`<= 0` asks for the
default). The handle it answers is the ordinary listener every other net
call already serves, so nothing else in the surface grew for it. A held
address answers `exists` now, instead of a bare `io`: the "already in
use" answer a server has to tell apart from everything else.

What the kernel then does with a group is the host's, and the clause
states it. All three answers were measured on the runner by the
runtime's own tests instead of quoted from a manual, and two of them
overturned the prediction:

- linux distributes accepts across the group: the prefork shape you
  expect, N processes bound to one port and N of them getting work.
- macOS hands every SYN to the newest bound socket, and falls to a
  survivor when that one closes. So `reuse_port` there buys a handover
  with no dropped connections, not a fan-out. For many hands on macOS,
  inherit the listener: one queue, every child accepting from it, the
  kernel doing the distributing.
- windows refuses `reuse_port`, and refuses an inherit set.
  `SO_REUSEADDR` there means "any process may take this port", so
  aliasing to it would hand you a hijack where you asked for a shared
  queue; and a `SOCKET` is not a small number a child can be handed by
  position.

`os_spawn_with` hands a child real descriptors, and the child receives
them as 3, 4, … in the order given. That numbering is the contract, so a
parent never learns a descriptor number and a child never has to be
told one: "the listener is 3" is true by position. The adopter asks the
kernel what the number is instead of trusting it (a stream socket, with
no peer, listening where the kernel keeps that flag), so a pipe, a
connected stream or a stranger's number is `io`, never a trap. It owns
the descriptor from then on (`net_close` closes it, a second adopt of a
live number is refused) but it owns nothing outside it: an adopted unix
listener does not unlink the socket file its parent bound. One spawn
hands at most 64. The checked machine refuses both with the construct
named, because a child of the program being interpreted would be a
child of the compiler.

Spec: `[os.net.listen.opts]`, `[os.proc.inherit]`. Witnesses:
`corpus/net/reuse_port.lu`, `corpus/net/inherit_listener.lu`. Per-host
detail in [`docs/platforms.md`](docs/platforms.md).

### The wait behind that number (s137 — #127 closes)

`net_wait(fds, deadline_ms)` answers which of a set of handles can be
read without blocking (a listener whose `net_accept` will not block, a
stream whose `net_read` will answer bytes or `closed`) as the subset,
in the order you gave. Every other blocking call in the family waits on
one socket; this is the one that waits on many, and it is what a program
that spawns no tasks has instead of a scheduler.

Measured on an idle loop holding one connection for five seconds (macOS
arm64, the shape lobo's ws16 bench runs):

| | loop passes | involuntary context switches | cycles |
|---|---|---|---|
| deadline time-slicing (25 ms + 12 ms) | 125 | 1,385 | 57.1 M |
| one `net_wait` on the whole set | 1 | 12 | 8.2 M |
| a program that only sleeps (the floor) | — | 7 | 6.9 M |

Net of the floor: 50.2 M cycles of idle work against 1.3 M, and 1,378
involuntary context switches against 5. The loop wakes when a socket
speaks, not when a timer says to look.

An empty answer is the deadline expiring with nothing ready, which is an
answer and not a failure. Readiness is level-triggered, so asking
consumes nothing and two waits answer the same until you actually read
or accept; a stream whose peer has gone is ready, and the read that
follows reports `closed`. `io` is a forged handle, or a wait nothing
could ever end. The call neither parks a task nor disturbs the
reactor's registrations, so it mixes freely with `spawn`.

Alone in this release it names no host: `poll(2)` on linux, macOS and
freebsd, `WSAPoll` on windows, and the checked machine mirrors it call
for call. Spec: `[os.net.wait]`. Witness:
`corpus/net/wait_readiness.lu`.

### `os_cpus()` (s137 — #233 closes)

A program can ask how big the machine is. `os_cpus()` answers the
count of cores this process may actually be scheduled on, always `>= 1`
when it answers.

The count is of *schedulable* cores, not *installed* ones. On linux the
number honours a cgroup cpu quota and a cpu affinity mask, so a
container given two cpus of quota on a sixty-four-core host answers 2,
where counting `processor` rows in `/proc/cpuinfo`, which is what a
program had to do before this existed, answers 64 and starts sixty-two
workers that will never get a core between them. macOS reads `hw.ncpu`,
windows `GetSystemInfo`; every tier reads the same source, so `wolf run`
and `wolf run --checked` agree on the number on one host.

A host that cannot answer gives the `io` row, and not a 1: a program
resolving `workers auto` can say it did not learn the number instead of
quietly running one worker and looking healthy. Spec: `[os.cpus]`.
Witness: `corpus/os/cpus.lu`.

### Four fixes

- A diagnostic on a long line killed the compiler (#238). When one
  diagnostic pointed twice at the same source line and the two places
  were far apart, the human renderer windowed the line around the first
  underline and then sized the second one `hi - lo` with `hi` left of
  `lo`, a `usize` underflow, so `str::repeat` asked for near-`usize::MAX`
  bytes and the process aborted. In `conform-run` that is worse than an
  ugly message: the record is written after the report, so the run
  exited 101 with an empty stdout, and every consumer that greps for
  `error[` read a clean file. wolf-std's sc35 lost a refusing row past
  two full-tree scans that way. Both ends of an underline clamp into the
  window now; a row past the cut marks the cut and keeps its label.
- `channel[bool]` compiled to a crash. A `bool` payload crossing a
  channel's one-word wire lowered to `zext.i64`, which the WIR verifier
  rejected as "integer conversion needs integer types": an internal
  compiler error on any native build. `zext` from `bool` is the bool→int
  bridge now and verifies; `sext` from `bool` still does not, because
  sign-extending a truth value means nothing.
- The runtime's own test pipes could leak into a child. On hosts
  without `pipe2` (macOS), the reactor's test pipe was not close-on-exec,
  so a descriptor nobody handed over could turn up in a spawned child,
  which matters rather more in a release where `os_spawn_with` hands
  descriptors over on purpose. It matches the posture every other
  descriptor the runtime opens already had.
- `net_wait` builds on a tier-2 host. Its readiness call reached
  through `crate::reactor`, which is gated to the three hosts with a
  poller written for them, so a target that merely `cargo check`s the
  workspace lost the module. It lives in `crate::poll` now,
  `cfg(any(unix, windows))`: the reactor owns the edge-consuming set the
  task layer parks on, and this is the level-triggered question a
  spawn-free loop asks about a borrowed set.

### The letters

The release published itself, again (#226). v0.2.4 was the first tag
where a fully-green four-host `dist` matrix moved its own release out of
draft, with the final `publish` job counting the archives on the page
before it touched the flag. If you are reading this on a published
release page that no one edited by hand, that is the mechanism holding
for a second tag.

The corpus cannot hold a long line, which is why #238 lived so long.
The renderer bug above needs a source line wider than the renderer's own
100-column window. `cargo xtask corpus` conform-runs every entry through
the human reporter, so the walk would have caught it on the first pass,
if any corpus file could carry the shape. None can, and none ever will:
`cargo xtask fmt-lu` holds every corpus file canonical under `wolf fmt`,
and the formatter reflows every over-width line it can break. Measured
while looking for a home for the witness: an `if`, a `match`, a call, an
operator chain, a nested type and a long string literal, six shapes, all
six reflowed. What survives formatting is the unbreakable single token,
which is the long f-string the ledger witnesses print, and that carries
one annotation, so the window centres on it and nothing lands past the
cut. Two of this repository's gates are in tension here, and the
resolution is that a witness needing an unformattable source writes its
own fixture: #238's lives in
`crates/wolf_driver/tests/long_line_record.rs`, which drives the binary
on both run-reaching lanes. Anyone reaching for `corpus/` to pin a
rendering shape should read this paragraph first.

The pairing moved to lupin 0.1.26 (pin `982f857`), and this time the
differential has no dark corner. v0.2.4 shipped paired with lupin 0.1.24
and named two classes of file the interpreter could not yet read: nine
that mint a `byte`, and one byte order mark whose single refusal named
itself in twenty more rows of `corpus/grammar/`, because lupin loads a
file's directory siblings as modules. Both retired at the interpreter's
next two releases, as that entry said they would. The ritual over
507 files at this pin: checked 266 agreements / 124 completeness notes
/ 2 soundness / 8 hard, native 287 / 124 / 0 / 5. Hard divergence is
down from 38 and 33, and the counts decompose with nothing left over. The six
(five) `Diag` rows are #167's warning-channel asymmetry, lupin emitting no
warnings at all; the two soundness findings are #168's float-cast twins,
where wolfc's own checked lane exits 0 and the native lanes trap. Every
surviving row is a filed issue older than this release, and no class
opened here.

## 0.2.4 — 2026-09-03

THE BYTE SHIPS. A 64 KiB read charges what it holds. wolf has a
byte-width scalar now (`byte`, an 8-bit unsigned octet, `0..=255`, one
byte of storage) and every builtin that produces or takes raw octets
speaks `List[byte]`. So `fs.read_bytes` of a 64 KiB file charges its
region 65,584 bytes natively and 65,536 on the checked machine,
where v0.2.3 charged 1,048,560 for the same octets: eight times the
width and twice again for the growth history a push-grown list carries.
Sixteen times the payload, down to one. Read a file, walk a string's
bytes, echo a socket, and the number in the ledger is now the number of
octets you actually have.

This is a breaking change, and it is one line of fix per site. The
elements of `str.bytes()`, and of `fs_read_bytes`, `fs_read_chunk`,
`net_read_bytes` and `str_from_utf8`'s input, are `byte`, not `int`.
A `byte` never adopts a numeric literal and never quietly widens: `b ==
10` is E0401, so is a literal `match` arm, so is `let n: int = b`, and
so is binding a reader's result as `List[int]`. Every note names the
spelling. Arithmetic and comparisons against integers want `as int`
(`b as int == 10`, `(b as int) + 1`); a value going back into a byte
list wants `as byte` (`out.push(n as byte)`, which truncates to the low
eight bits and never traps). A width-bearing scalar infers nothing
behind your back, and the compiler counts the sites for you: 33 in 11
corpus files here, 87 in 13 of wolf-book's, 198 in 45 of wolf-std's,
217 in 30 of lobo's. Comparisons against int literals are the majority
everywhere.

wolf speaks unix-domain sockets too. `net_listen_unix(path)` /
`net_connect_unix(path)` on linux and macOS, in the TCP pair's own
shape: the fd is an ordinary net stream, so `accept`, `read`/`write`,
the byte pair, `net_deadline` and `net_close` all serve it call for
call. Windows answers `unsupported` instead of pretending.
[`docs/platforms.md`](docs/platforms.md) is the per-host ledger.

It pairs with lupin 0.1.24 at pin `3befc3e` (D57: the pin is part of
this release's identity).

### The producers speak bytes (s136 — #231 closes)

`byte` arrived in the language at s135 and nothing produced one. All
eight octet builtins (`str.bytes()`, `str_from_utf8`, `fs_read_bytes`,
`fs_write_bytes`, `fs_read_chunk`, `fs_write_chunk`, `net_read_bytes`,
`net_write_bytes`) were declared over `List[int]`, which made the type
worse than useless downstream: wolf-std measured a reader substituted to
`List[byte]` charging 17x/18x its payload where the `List[int]`
reader it replaced charged 16x, because the substitute had to convert
elementwise against a builtin and `[mem.region.account.1]` keeps the
list it converted from charged for the region's life. The sc34 sprint
refused the substitution with that table instead of shipping the
regression, and this release is the answer.

The eight speak `List[byte]` on every tier. The native runtime mints a
byte read as one 1-byte-element buffer at exact capacity, so a producer
that knows its length has no growth history to pay. That is why the
64 KiB read costs 65,584 (one list header plus the payload) and not the
131,120 a push-grown `List[byte]` would, and the checked machine's byte
slots charge the payload exactly.
`corpus/memory/byte_producers_ledger.lu` pins the relations on every
tier (a read holds its payload; at most the payload plus a header; the
same octets as `List[int]` charge at least seven times more);
`strings/bytes_roundtrip.lu` is the round trip and `net/echo_bytes.lu`
the echo. The refusal for a `byte` used as an `int` is E0401 with the
spelling in the note (`typecheck/byte_elem_arith_fail.lu`). The
`invalid` row stays declared on the two writes: the vocabulary is
stable, and a byte element is always in range, so typed code can no
longer reach it. `os_random` keeps its `List[int]`, because
`[os.random]` says so and it is not a byte surface.

### The phantom 16x (s136 — #232 closes)

The checked machine charged 1,048,576 ledger bytes for `for b in
s.bytes()` over a 64 KiB `str`, a walk that allocates nothing and that
the native tier and lupin both charge 0 for, because it evaluated
the consumed view as the materializing fallback. The consumed positions
(`for b in s.bytes()`, `s.bytes()[i]`, `s.bytes().len` and the query
family) are the receiver's own storage on that tier too now, charged to
no region. `let bs = s.bytes()` still materializes and still charges;
that is the difference the syntax was always making.
`corpus/memory/consumed_walk_charges_nothing.lu` prints the relation on
every tier.

### Unix-domain sockets (s136 — #227 closes)

`[os.net.unix]`, the spec's first socket clause. `AF_UNIX` stream
sockets in the TCP pair's shape, and the error rows tell a host apart
from a path: `unsupported` is the host, and never a bare `io`;
`exists` is a stale socket file at bind (the operator's to remove; the
runtime never clobbers a path it did not bind), `not_found` a missing
directory or file, `denied`, and `refused` a file nobody listens on.
The runtime created the socket file, so `net_close` of the listener
unlinks it. Windows answers `unsupported`: `AF_UNIX` has existed there
since 10 1803 and the test suite measures the kernel's answer on the
runner, but `std::net` has no unix-domain surface on that host and the
runtime carries no winsock binding beyond `WSAPoll`, so
`docs/platforms.md` names the rung instead of the runtime claiming it.
`corpus/net/unix_echo.lu` is the three-lane witness, and it prints the
same stdout on every host either way: an echo where the family is
served, a named refusal where it is not. lobo's control endpoint gains
its second transport at ws16.

### The string-layout codes (s136 — D74, #230 closes)

One rule per code, which is what the catalog and the spec had stopped
agreeing about. E0103 is the delimiter rule: a `"""` shares its
line with text, opening or closing; delimiters stand alone. E0104 is
the margin rule only (a content line left of the closing delimiter's
column); the closing-delimiter case used to answer E0104 too, and the
spec sentence and the catalog disagreed about which meaning it had.
E0105 is the tab/space rule, unchanged. E0102 owns the bare `{`
in a plain string: `"hello {world"` is a string whose interpolation
never closes before its line ends, and wolfc answered E0109
("unterminated generalized literal", because the `world"` inside the
open interpolation happened to spell one), which is the wrong family;
lupin was right. And a byte order mark at the very start of a file is
stripped: tolerated, never a diagnostic, and the formatter keeps it
in place; anywhere else it is E0107. Six corpus witnesses under
`grammar/` pin the codes and spans for both machines.

### One path spelling (#222 closes)

`wolf add` and `wolf publish` printed their status paths with the host's
separator (`app\wolf.pkg` on windows) while every diagnostic path in the
same binary is slash-normalized (`--> app/main.lu:3:5`). Every path a
package verb prints (`add`, `rm`, `init`, `vendor`, `publish`, and
their error lines) goes through the diagnostics' own spelling now:
forward slashes on every host. Found by wolf-book's samples lane the
week it was first lit.

### The deadline and the closed peer (#224 closes)

`wolf conform-run --checked` killed a connected client at `net_deadline`
after the peer closed. The cause is not handle lifetime: lobo's serve
sequence hands the parsed head to `serve_request` and never drains the
socket, so the server's close is a close over unread receive data, an
RST close, and on macOS `setsockopt(SO_RCVTIMEO)` answers EINVAL on a
reset socket (linux keeps answering 0) while the reply the server wrote
is still readable. The checked machine implements deadlines with that
setsockopt pair and reported the EINVAL as an `io` row. The deadline now
arms on a reset socket (the native reactor's timer wheel never asked the
kernel and always did), the buffered reply is read, and the reset is the
ordinary `closed` row after it. `net/peer_close_after_serve.lu` pins it
on every tier; the checked twin lives in `crates/wolf_mem/tests/net.rs`.

### The letters

The release publishes itself (#226). This page has been created as a
draft since v0.2.0, which is right, since no half-uploaded release
should be public. But the un-draft was a hand step, and it is the step
that left v0.2.0 and v0.2.1 unpublished for a day (#200) and v0.2.3
sitting behind a complete, green matrix. `release.yml` gains a final
`publish` job that `needs:` the whole four-host `dist` matrix: it runs
only when every archive uploaded, it lists the assets into its own log
before it moves the flag, and it refuses (leaving the draft alone) if
fewer than four archives are on the page. A fully-green matrix
publishes itself; a red one stays a draft.

The pairing moved to lupin 0.1.24 (pin `3befc3e`), and this is the one
release where the differential is behind. The reference interpreter
released `byte` in the mirror at is35, but the type landed
at s135 and its producers at s136, both after that tag, so lupin 0.1.24
refuses `as byte` at resolve and nine corpus files go dark on it:
`fs/bytes_dirs`, `memory/byte_list_ledger`, `net/byte_roundtrip`,
`net/echo_bytes`, `net/line_reader_bytes`, `strings/byte_view_lend`,
`strings/from_utf8_border`, `typecheck/byte_casts`,
`typecheck/byte_shapes`. D74's byte order mark costs one more:
`corpus/grammar/bom_at_start.lu` opens with `ef bb bf`, wolfc strips it
and lupin 0.1.24 refuses it, and because the interpreter loads a file's
directory siblings as modules, that single refusal names itself in
twenty other rows of `corpus/grammar/`. The ritual run over 503 files:
checked 238 agreements / 2 soundness / 38 hard, native 261 / 0 / 33,
and the hard counts decompose with nothing left over: 6 and 5 are
#167's warning-channel asymmetry, 2 are #168's float-cast twins, 21 and
19 are that one BOM, 9 and 9 are the byte flip set. Two named classes,
both retiring at the interpreter's next release.

## 0.2.3 — 2026-09-02

THE ARCHIVE RETURNS. On Windows, `spawn` and scopes, `proc`, channels
and `select`, `sync`/`when`, `os.signal` and `net` deadlines now compile
and run. v0.2.2 was the first archive that built and ran your program
on that host, and it refused twenty-one corpus rows by construct name:
"windows-native serves no `spawn`/scopes (the task layer) at the s60a
bring-up". That table is retired. The windows native lane measures at
macOS parity now, 261/278/0/295/0 (checked/native/release/union/
all-three), with zero rows refused by construct name, and a stack
overflow inside a task reports in wolf's own voice,
`wolf-rt: stack overflow in task '<name>'`, exit 134, where v0.2.2
died as `0xC00000FD` with no words at all. Two things this host still
does not serve, and says so: `wolf build --release` (the LLVM tier is
s60c's), and external `reload`/`upgrade` signal delivery, which has no
Windows analog. A program's own reload path works, and wws-shaped
programs use a control channel for the rest.

linux aarch64 has its archive back. v0.2.2's release page
carried three archives, not four: the new learner-path smoke gated the
upload on a native tier that host does not serve, so the arm archive was
built and then thrown away (#213). The smoke reads the driver's
exit-code contract now, treating an exit-2 environment refusal as an
unserved host and not a broken archive, printing the refusal a learner
would see and then proving `wolf test corpus/hello.lu` runs from the
unpacked archive on the checked tier, which is how that host has served
learners since v0.2.1. Four archives at this tag. And the release page
carries this paragraph instead of a bare asset list, which is the third
letter below.

Underneath (s60b): workers are Win32 threads on the kernel's own
reserve-and-guard stacks, `CreateThread` with
`STACK_SIZE_PARAM_IS_A_RESERVATION` at `WOLF_TASK_STACK`, 8 MiB by
default: `VirtualAlloc(MEM_RESERVE)` plus the `PAGE_GUARD` page ntdll
walks down on first touch, done by the kernel instead of by hand. The
span is not ours to pool (Windows offers no thread on memory the runtime
mapped), so a worker keeps its stack for life, and idle trim is named
for s60c. `os.signal` rides `SetConsoleCtrlHandler`: Ctrl+C and the
console closing are `terminate`, Ctrl+Break is `quit`, the
`[os.signal.platform]` table, with `os_signal_raise` an in-process
loopback, because no self-targeted console event exists that would not
also reach every other process on the console. The reactor behind the
s35 interface is `WSAPoll`: `net` deadlines fire the `timeout` row,
accept/read/write park with the pool compensating, kill teardown reaches
them. Measured on the runner: 24 tasks parked on 200 ms read deadlines
resolved 24/24 `timeout` in 207 ms wall, so the deadlines fire at the
deadline and not at the poll's cost. IOCP (completion ports, the
async-fs and many-socket rung) is s60c's, behind the same seam.
[`docs/platforms.md`](docs/platforms.md) is the per-host ledger.

It pairs with lupin 0.1.23 at pin `8cda3aa` (D57: the pin is part of
this release's identity).

### The server annotates (s134)

`wolf lsp` serves `textDocument/signatureHelp`,
`textDocument/semanticTokens/full` and `/range`, and
`textDocument/inlayHint`, the three rungs s133's closeout named as
the binding table read three more ways. Nothing is a textual search.
Signature help reads the checker's call record for the innermost
call whose argument list holds the cursor (`TypedBody::calls`, keyed
by the call expression's span): the declared parameters as
`name: type` with a declared `mut`/`take` spelled, the receiver
omitted because the parentheses never spell it, the active parameter
counted by the commas before the cursor, the return type when the
callee is a declared item, and its `///` comment (the one doc model
hover and `wolf doc` read; markdown when the client lists it, plain
text otherwise); triggers on `(`, re-triggers on `,`. Semantic
tokens classify every identifier through what it bound to
(`Resolution::refs`, then `TypedBody::member_refs`): `parameter` when
the binder sits in a parameter list, `variable` otherwise (`readonly`
unless the binder is `var`'s), `function` / `type` / `variable` by an
item's kind, `namespace` for modules and std paths, `property` /
`enumMember` / `function` for fields, variants and methods, `keyword`
from the token kind, `type` for builtins and `Self`; a binder's own
token carries `declaration`; a name the compiler never bound gets no
token. The legend is closed and fixed: eight types in one order, two
modifiers. Inlay hints are the inferred type of an unascribed
`let`/`var` binder and the parameter name before a positional
argument that is not already that name, and only at calls the checker
resolved to a declaration, so a fn-typed value and a prelude name
offer none; each class switches off through
`initializationOptions.inlayHints.{types, parameterNames}` and the
client's own toggle decides whether hints show at all. Positions
honor the negotiated encoding at every span (an astral character on
the line before a token moves its UTF-16 column, not its byte). No
delta tokens: a full answer is cheap here and a delta is a promise
about identity across edits this server has no reason to make; it
answers `-32601` like every other absence. `wolf_query`'s
contract moves to v5 (additive). Eighteen transcripts were recorded
in wolf-lsp against this build (one script per rung per maintained
client profile, fackr, facsimile, nvim, vscode, helix and emacs, the
answers differing by the profile's own declarations), the forty-seven
existing ones re-recorded with the initialize answer as their only
diff, and the unknown-method probe re-targeted at what is still
absent. Latency, s57's table before and after on the same machine:
`diagnostics-after-edit` p95 110.6 → 110.8 ms (p50 107.7 → 106.6),
hover p95 0.2 → 0.1 ms, cold first diagnostics p95 4.1 → 4.4 ms. Every
class is inside its budget and the number near perception is unmoved.

### The proc leaves its module (s134 — #219 closes)

A `spawn proc` in a non-entry module now builds on the release
tier under every partition. lobo ws13 measured the gap while
adopting the region cap: its budget helper spawns a proc from a leaf
module, and `wolf build --release` answered “cannot compile this yet —
func.addr of `@work.run.task0.entry` outside this object's subset”
while `wolf run` executed the same program: #136's proc twin, one
partition over. s117's `refs=` edge keeps a spawner and its entry shim
in one cluster; the per-module partition (`WOLF_MIDEND=0`, the
measurement mode lobo's gauntlet runs in while #146 is open) never
consulted it, because the shim is synthetic, has no source file, and
rode the root module's object while the spawner sat in its own. The
debug tier had imported such a symbol across objects since #116; the
LLVM tier refused. The LLVM emitter now takes an out-of-subset
referee's address through the same mangled-symbol declaration a
cross-object call uses, a link-time constant in every object, resolved
by the linker. Only a name no module function carries is a refusal now.
Witnesses:
`corpus/conc/proc_cross_module` (ws13's thirty-line reproducer, three
lanes plus lupin, `normal=0 breach=2`) and a driver test that pins the
per-module partition itself, because a thirty-line program is one
cluster under the whole-program phase and the refusal cannot fire
there. lobo's `ws13-cap` branch builds `--release` at this commit.

A refusal names itself in the record. `wolf conform-run --json`
answered `{"verdict":"unsupported","diagnostics":[]}` on every checked
proc spawn: named on stderr, and nothing in the record a rig reads
over a pipe. The record now carries `x-unsupported-construct` and,
when the refusal has one, `x-unsupported-span` (`[proto.record.ext]`
extension keys, so they take no part in comparison and the
counterparty need not emit them) on every `unsupported` verdict at
every rung: typecheck, mem, wir, the checked machine, and the native
and release lanes. `diagnostics` stays empty, because an `unsupported`
verdict
carries no partial diagnostics and a refusal is not a fault in the
program, so it has no E-code. `proc_cap_fault_join.lu`'s checked
verdict is unchanged, `unsupported` at `mem`, now reading
`"x-unsupported-construct": "structured concurrency in checked
execution (C1 deferred)"` with the span of the spawn, because the
checked machine (`conform-run --checked`, the s23 UB machine) runs no
structured concurrency at all: `spawn`, scopes, `select` and `when`
are refused at the expression. A proc is refused where every spawn is;
running one there is the C1 sprint, not a fix. The other `--checked`,
meaning `wolf run --checked` and `wolf build --checked`, is the native
build under the checked profile (the quarantine allocator and the
checked-tier runtime hooks), which is why ws13 saw the same file run
there and refuse under `conform-run`; the two flags name two machines,
and the record now says which one declined.

### The span is the offending token (s134 — D71, #220 closes)

A parse refusal about a token now spans that token. is34's first
full three-lane diff-run found DIV-2026-020: on eight grammar
witnesses wolfc's E0201 was a zero-width point at the offending
token's start while lupin spanned the token, the same byte at a
different width, invisible to every walk that compares codes and
visible to every editor, which highlights nothing at a zero-width
range. D71 ruled the width: "expected `}`, found identifier `y`"
points at `y`, every byte of it. The parser's one primitive for a
refusal about the current token (`here()`) now answers the token's own
span, so E0201 and its siblings (E0203's "expected a struct name",
E0206's "expected a type", E0207's "expected a pattern", the missing
`=`/initializer reports) all moved together; a refusal at end of file
still lands on the zero-width `Eof` marker, and a suggestion's edit
keeps its own zero-width anchor (an insertion point is zero-width; the
primary span and the fix's span were always different things). Seven of
the eight rows are now byte-identical to lupin's: `[550,551)` the `y`,
`[534,535)`, `[415,416)`, `[581,582)`, `[669,671)` the `..`,
`[332,333)`, `[896,897)`. The eighth, `let_group_bare_tuple.lu`, was
never a width question: wolfc reads the D63 let-group and refuses at
the end of the initializer list ("this value has no name", now
`[374,375)`) while lupin refuses at the first comma (`[364,365)`), so
it stays a locus divergence for its own triage, as #220 said it
would.

Blast radius, measured before the change: wolf-lang, 37 snapshot files
(35 in `wolf_parse`, the LSP one-truth test's `[28,28]` → `[28,29]`,
and the eight `check: fail(E0201)` corpus pins unchanged, since the
walk compares codes and the directive grammar pins no spans); wolf-book, 5
diagnostic snapshots carrying 7 E0201 renderings whose carets widen
(a read-only count; the book's lane re-records at its pin bump);
wolf-lsp, 0 transcripts (none carries an E0201; the two E0202s are at
the opener and the E0203 in the two smoke transcripts already spanned
its token). `cargo xtask differ` gains a span-width class: two
rejections with the same code at the same start byte that differ only
in width now classify as `SpanWidth`, where until now they were a
`Diag` divergence spelled like a wrong locus, which is why is34 could
only carry them as a waiver (`differ::DIV_2026_020_FILES` in
wolf-interp; it retires at the pin bump that carries this, the
interpreter lane's, as #177's did).

### The letters

One archive per target, not a history (#212). `cargo xtask dist`
runs in every gauntlet, and the archive, the staged tree and the
unpacked smoke tree it writes are all named for the version, so a
version bump renamed them and left the previous set behind forever. r05
measured the cost: five runs' worth plus the fuzz target dir took the
release rig to 0 bytes free mid-release, which deadlocks a tool
harness that spools every output to a file first. `dist` now prunes
every prior artifact for its own target before it writes anything, and
names each removal, so a CI log can be audited. The second consequence
was worse than disk and closes with it: the release workflow uploads
`target/dist/*.tar.gz` by glob, so an archive left over from an older
version would have ridden a tag it did not belong to.

The release body fills itself (#214). `GET
/repos/wolffe-lang/wolf-lang/releases/tags/v0.2.2` answered with a
`body` of length 0, and so did v0.2.1: the workflow created the
release with `--title` and nothing else, so the paragraph written for a
newcomer lived only in this file, and a learner arriving from a download
link never read it. `cargo xtask release-notes <TAG>` cuts the entry for
the tag with fenced blocks transparent (the 0.2.2 entry quotes a
compiler refusal inside a fence, and a naive cut would have ended the
body two paragraphs in), and the release job passes it to `gh release
create --notes-file` and then to `gh release edit`, so the body is right
whether the workflow opened the release or someone else did. A tag with
no entry here fails the job.

The multiline, raw and generalized literals have productions (#215).
`literal` named `MULTILINE_STRING`, `RAW_STRING` and
`GENERALIZED_STRING` on one line and defined none of them; `STR_TEXT`
and `CHAR_TEXT` were cited by the productions above them and defined
nowhere. A reader working from `spec/grammar.ebnf` alone derived nothing
at all for three of the six literal forms, the same class as #198, one
literal over. It bit le05, which wired an `invalid_escape` node for
tree-sitter-wolf and had to decide whether it belongs inside a `"""`
string: the productions could not answer, so the answer had to be read
off a contrast between three prose bullets, two of which say "no
escapes" outright and the third of which says nothing about them. The
lexer was measured and the productions written from it:
`MULTI_PART ::= MULTI_TEXT | STR_ESC | '{{' | '}}' | INTERP`, the same
alternatives `STR_PART` has, because one routine scans both bodies. So
escapes and interpolation work inside a multiline, and a reader can now
derive that from the grammar. `RAW_TEXT` and `GEN_TEXT` derive
scalars and nothing else, which is where "no escapes, no interpolation"
is read from instead of the sentence beside them. Two corpus witnesses
make it measured: the escapes running inside a multiline, and the E0101
refusal of an unknown escape there, the first corpus entry anywhere to
pin one, which is how it turned up #225, a code collision 484
differential entries had never shown: lupin refuses a bad escape under
the code the catalogue spends on a multiline's opening line, in plain
strings and multilines alike.

The pairing moved to lupin 0.1.23. The sibling released while s60b
and s134 ran, which is why both waves carried `LUPIN=` overrides. The
ritual differ run over 484 corpus files found nothing new: checked 257
agreements / 2 soundness / 8 hard, native 278 / 0 / 5, every hard row a
standing named one (#167's warning-channel asymmetry, #168's float-cast
twins). One class is gone: not a single `SpanWidth` row, because
s134's D71 work made those seven E0201 spans byte-identical and this is
the pin bump at which the interpreter retires its waiver.

## 0.2.2 — 2026-09-02

THE LEARNERS' RELEASE. On Windows, this is the first archive that
compiles and runs your program. Unpack it, keep `wolf.exe` and
`wolf_rt.lib` together, and `wolf run hello.lu` produces a real
`hello.exe` and executes it: the native tier, on the host, not an
interpreter. One thing has to be installed beside it, Visual Studio
Build Tools with "Desktop development with C++". The import libraries
every Windows link needs (`kernel32.lib`, `ws2_32.lib`, the UCRT,
`msvcrt.lib`) ship with the Windows SDK and the MSVC toolset, not with
Windows, and this is the same requirement Rust's own `windows-msvc`
toolchain carries; bundling them so no install is needed at all is
s47's, and the refusal that asks for them says so. With them present
wolf finds a linker by itself, trying `WOLF_LINKER`, then `lld-link`,
then rustup's bundled `rust-lld` (a learner with a Rust toolchain
already has one), then MSVC `link.exe`, and `wolf build --verbose`
names the choice.

What the bring-up does not serve, it refuses before the link, in these
words:

```text
wolf build: cannot compile this yet — windows-native serves no
<construct> at the s60a bring-up (the runtime's task layer, io
reactor, and signal delivery are s60b's — the IOCP road);
`<symbol>` would not link (pipeline is honest through `wir`; the
conservatism ledger, not a bug in your program)
```

Twenty-one rows of the corpus take that refusal: the task layer
(`spawn` and scopes, `proc`, channels and `select`, `sync`/`when`,
`region_transfer`), `os.signal`, and `net` deadlines. `wolf build
--release` refuses in the same voice. Everything else serves: `print`,
strings, lists, json, `fs`, `os` (env, cwd, exe, exit, child
processes), `time`, `random`, regions and the region ledger, `net` in
its documented blocking posture, and the C membrane for scalars and
pointers. A trapping program prints `wolf-trap: <kind>` with its
site line and exits 134, the same number as every other native
host. Nothing is a silent stub and nothing is a link error.
[`docs/platforms.md`](docs/platforms.md) is the per-host ledger and
names s60b and s60c as the road.

Underneath (s60a): cranelift emits COFF objects under the MSVC x64
convention, the driver links them against `wolf_rt.lib` (shipped in
every windows archive since v0.2.1) and the import libraries a Rust
staticlib needs, and the C runtime's console entry calls wolf's `main`
shim. Five linker rungs were proven, and one of them turned up a real
`link.exe` bug with wolf's DWARF SECREL relocations. The 134 is not
signal arithmetic here: a trap is a call into the runtime ending in
`ExitProcess(134)` (D70), which is why no vectored handler is needed,
since no sited trap is a fault at any tier. `[abi.c.targets]` gains the
win64 bring-up contract: scalars and pointers direct, aggregates by
value refused by shape until the campaign's `cl.exe` differential.
`cargo xtask lane-coverage` measures windows on its own floor line,
259/255/0/274/0 (checked/native/release/union/all-three), checked at
full parity, native the macOS count minus the 21 refused rows, release
dark because that host's floor says so. `cargo xtask dist` now
unpacks its own archive and builds and runs `corpus/hello.lu` from it
on every host: the learner's path, mechanized, in standing CI.

It pairs with **lupin 0.1.22** at pin `2bfbe5e` (D57: the pin is part
of this release's identity).

### The server navigates (s133 — #208 closes)

`wolf lsp` serves `textDocument/definition`, `textDocument/references`
and `textDocument/rename` (with `prepareRename`), the three rungs s122
named and nobody had climbed, so F12 / Shift+F12 / F2 on a `.lu` file
did nothing in every editor. All three answer from a binding table,
never a textual search: the resolver keeps the decision it already
makes for every name (`Resolution::refs`, uses and binders alike, a
binder a ref to itself, an import's bound name a ref to what it
imports), and the checker keeps the type-dependent half
(`TypedBody::member_refs`: fields through `.`, literal and pattern
fields, variants in value/call/pattern position, methods and associated
fns). The two halves share one key, the declaration's name token span
with its `FileId`, so a cross-file answer is a lookup over the package
graph, not a scan. A name the compiler never bound answers `null`
rather than a guess, and the lexical half keeps answering when typing
stopped.

The wire shapes follow the client's declarations, read once at
`initialize`: `LocationLink[]` with `originSelectionRange` when it
declares `linkSupport`, `Location[]` otherwise; a rename's
`WorkspaceEdit` as `documentChanges` when it declares them, the
`changes` map otherwise. References come back in (file, offset) order,
the declaration only when `includeDeclaration` asks, and the negotiated
position encoding holds at every span (the utf-16 astral case is
pinned). Rename refuses with `RequestFailed` (-32803), naming the
token and the reason and never leaving a partial edit, on keywords
(`self` and `Self` included), builtin types, prelude names, std and
`import c` symbols, modules, and a new name that is not a single identifier;
`prepareRename` refuses with the same reasons before the box opens. The
D59 `//!` member marker carries no identifier, so no rename ever
touches one. `wolf_query`'s contract moves to v4 (additive). Six client
profiles were transcribed against this build. Latency, measured before
and after on the same machine: `diagnostics-after-edit` p95 105.7 →
105.7 ms, so the one number near perception is unchanged, and the three
new requests answer at ≈0.03 ms p95 in-process. Known residue: the
reachable set is the package around the entry (the v0 single-entry
model); a workspace-root model is s57's.

### The region answers, and holds (s131, s132, D68 — #187 closes)

Region accounting became readable and then became a contract. Three
tiers gained `region_bytes(r)`, a named region's byte ledger and the
count `wolf_rt` has kept since s76, now surfaced, and
`live_region_bytes()`, the process-wide live total, with
`[mem.region.account]` pinning what every tier guarantees (zero at
creation, monotone within the lifetime, stable between allocations,
wholesale disappearance at free) and leaving the units as per-tier
measured facts.

Then the cap. A region takes a creation-time byte budget (`region
r(cap: n) { … }`, or `region(cap: n)` on the value form) and a charge
that would take its ledger past the budget is the deterministic trap
`alloc-contract` at the allocating site: the existing kind, no new trap
vocabulary, because a byte budget is an allocation contract. At-cap
exactly is not a breach; the next byte is; `cap: 0` is legal and
everything breaches it. Budgets are denominated in ledger units, not
payload bytes. The clause now says so, after #203 measured a
64 KiB io chunk at ~1 MiB of ledger, so portable programs derive
budgets from measured `region_bytes` readings, which is how the
witnesses pin the boundary on all three tiers at once.

D68's point is the fault half: a breach inside a proc no longer
kills the process. The trap is contained at the proc boundary, so the
proc dies by the killed-proc sequence and no `defer` below the boundary
runs, and `[conc.proc.exit]`'s closed reason set gains the mapping,
`fault(kind)` read at the join as `is_fault()` and
`is_alloc_contract()`. Teardown is free-then-deliver, measured and then
pinned: at the join, `live_region_bytes()` has already returned to its
pre-spawn reading, so the supervisor that answers 503 on this reason
was handed the memory back first. The trapping worker parks, and its
thread and stack high-water are the measured per-breach cost.

### The comma insists everywhere (s131, s132 — D67, D69)

The separating comma is now required wherever the productions always
said it was, family by family and each with its blast radius measured
before the tightening. D67 took the pattern family: `Point { x .. }`,
`Point { x y }` and `(a b)` refuse at E0201 with a machine-applicable
"add the comma" fix (`wolf fix --apply` produces the canonical
spelling), and `..` follows a separator like one more member. D69 took
the rest of the unlicensed laxity: struct literal fields (`Point { x: 7
y: 2 }`, and the newline-separated spelling lupin always refused),
closure parameters (`fn(a b)`), and inline-C capture lists (`unsafe c
[a b]`).

Blast radius across the world, measured: zero working code. The
wolf-lang corpus and fixtures (532 files), wolf-book (1,129), wolf-std
(887), wolf-web (1,976) and lobo (1,395) already write the comma; the
sole flagged file anywhere is a fuzz-minimized broken-input formatter
fixture, whose idempotence test still holds. The multi-line literal's
trailing layout is untouched (a terminator run before `}` is the
production's own) and the separator report latches once per list and
never into a reported wreck. Only lupin and the spec's letter were ever
this strict; the compiler now agrees with both.

Two more measurements rode along: `defer` runs at scope exit, not as
the frames return (D66/#193, now with a corpus witness pinning the
loop-turn interleaving on all three lanes), and two or-pattern
divergences got witnesses. An or-pattern over product alternatives
refuses on every wolfc lane, one inside a product refuses natively
while the checked executor runs it, and lupin runs both. These are
permissive-direction divergences that were invisible until a file put
them in the differ's ledger (#196).

### The letters

A bare entry name means `.` (#206). `wolf conform-run hello.lu`
answered "the package root has no wolf source files" where
`./hello.lu` ran the program: `Path::parent()` on a bare relative name
is the empty path, not `None`, and the anchoring that fixed it lived in
one CLI parser, which is why `build`, `run` and `fmt` worked
and `conform-run`, `test`, `interface` and `doc` did not. It lives in
the loader now, root and entry anchored together so a headered entry
stays its own module's entry. One consequence is worth stating, because
it is visible in the machine record: `conform-run` reads its argument
through the same anchoring before interning it, so the record's `file`
field carries the anchored spelling and `wolf conform-run hello.lu` and
`wolf conform-run ./hello.lu` now produce byte-identical records
instead of two `FileId`s for one file. Identical programs, identical
records, whatever the command line typed. On Windows before this
release that one missing `./` stood between a learner and their first
program, because `conform-run --checked` was the only way to run one.

`STR_PART` derives escapes (#198). v0.2.1 bounded `\u{…}` at one to
six hex digits and said in prose that the bound "binds in string
literals too" while the production derived no escape at all, so a
reader working from `spec/grammar.ebnf` got the bound for `'…'` and
nothing whatsoever for `"…"`. `STR_ESC` now carries the escape set,
`UNI_ESC` sits beside it, and `CHAR_ESC ::= STR_ESC | '\' "'"`, so
"the char set is the string set plus `\'`" is read off the productions
instead of asserted next to them. Two corpus witnesses make the string
half measured: the seven-digit refusal (a shape rule, not a value one,
since `0x0000041` is `A` and it is refused before anything asks) and
its in-bounds twin at one, four and six digits.

A trap runs no defers, at the root too (#209). `[conf.trap.exit]`
ruled the proc path and was silent about the root, so nothing pinned
whether a root-domain trap flushed its pending defers. The consistent
reading is written now, that a trap is not an error value and runs no
`defer` or `errdefer` anywhere and that at the root death is immediate,
with `faults/trap_skips_root_defers.lu` as the witness. It records a
measured divergence for the interpreter's next sprint: every wolfc lane
abandons the pending root defer, lupin 0.1.22 runs it. The differ could
never have found this on its own, because on a trapping program the
interpreter's record carries no stdout, so the two machines are
verdict-identical whatever they print.

Two nondeterministic verdicts retired. The net refusal probes
dialed a just-released ephemeral port and bet that nothing took it in
between; under `cargo test`'s full parallelism that bet lost, and it
reddened a trunk gauntlet while passing 3/3 in isolation (#205). They
dial a port from outside the host's ephemeral range now, one nobody's
`bind(0)` can be handed, so one dial is the whole story. And the
bare-entry suite's linking rung builds the runtime staticlib on demand
and skips where a host cannot link, with the reason printed, instead of
reading an absent toolchain as a regression.

## 0.2.1 — 2026-09-01

THE LETTER AND THE ARCHIVE. A patch release: no new features, and
nothing runs here that did not run at v0.2.0. It carries a tag the
Windows archive can finally reach, the pattern work that landed
between the tags, and four places where the written language and the
built one had drifted apart, each one measured on the tools before a
word of it was rewritten. It pairs with lupin 0.1.20 at pin
`b80d239` (D57: the pin is part of this release's identity).

### Patterns take the product domain (s129, s130)

Struct patterns went through the whole pipe (`[gram.pat.struct]`,
#179): `Point { x, y: p, .. }` takes a struct apart by field name,
fields move field-wise so an unnamed field stays live, a pattern
without `..` must name every field (E0814, the same lean E0408 takes
at construction), and the formatter has a canonical form for them.
The lent-view slice gap closed alongside it (#184): `b[lo..hi]` over
a lent byte view resolves per `[mem.list.slice]` instead of meeting a
misattributed c06 refusal.

Match arms then took the full product domain (s130, retiring that c06
arm): tuple and struct patterns, `@`-bindings, literals at product
depth, and products nested through enum/row payloads (`Pair(a, 0)`,
`Dot(Point { x, y: 0 })`) compile on both native tiers and execute on
the checked lane, with exhaustiveness and the redundant-arm warning,
which already reasoned over products, finally carrying running
witnesses. Two shapes stay refused on the native pipe: an
enum/row test inside a product, and a `str` literal inside one.

An arm still takes the whole scrutinee when it binds a non-`Copy`
piece. The field-wise partial-move story remains a `let`-binder rule;
the boundary is pinned by an `E1001` witness and the diagnostic's
`copy` suggestion is the sanctioned idiom. Two checked-lane fixes rode
along (arm guards evaluate their condition instead of their wrapper
node, and a qualified constructor's dotted tag (`Pairs.Pair`) matches
a bare-name arm the way the native tiers compare tag ids) and one
release-tier fix: a type-blind peephole could fold a bool `bxor x, x`
to an integer constant and ICE the verifier, so boolean results now
fold to `bconst`.

### Four letters, each measured first

`defer` runs at scope exit, not as the frames return (D66, #193).
Every implementation already did: a `defer` in a loop body fires at
the end of each turn, and `[mem.shared.drop.1]` had implied it all
along, since a drop that runs "at scope exit, LIFO with defer" cannot
be LIFO with something frame-timed. `[mem.model.order]`'s frame
wording is amended and a corpus witness now pins the interleaving on
all three lanes.

`\u{…}` takes one to six hex digits (#189). The clause said so in
prose while its production said `HEX_DIGIT+`, and nothing said which
was normative. The lexer bounds it at six, so the prose was the
letter and the production amended. The bound is on the escape's
shape, not on the value it names: leading zeros count, and
`'\u{0000041}'` is refused before anything asks that it spells `A`.

Two region diagnostics stopped lying (#192). `W1001` claimed a
region "never allocates — delete the region" on blocks whose callees
allocate through it; the fact it read was the in-frame site list, and
D12 charges a callee's allocations to its caller's ambient, so the
advice was measured at +82 MB on the reporting program. It now
requires no call in the region's extent either. And `E1010` refused a
region block whose tail was a unit-typed raising call: an error union
is never `Copy`, so every raising call got a phantom ambient
allocation, which in tail position read as the block's value
outliving the region. That judgement now reads through the error row
the way the region-return judgement beside it already did: a row tag
carrying a real payload still allocates and still gets its site.

### The archive

v0.2.0's release workflow produced three of four tier-1 archives and
failed on Windows: `cargo xtask dist` hunted the unix staticlib name
on MSVC, the first tag since s59 added the guard. Fixed forward at
`10c2bf8`: the staticlib is named for its target, and the Windows
archive stages `wolf_rt.lib` against the day s60 teaches the driver to
link there. This tag proves it (#183).

## 0.2.0 — 2026-08-30

WOLFGANG TELLS THE TRUTH. v0.1.0 was an identity claim that shipped
the debug tier and named the rest not built. Nineteen days later the
rest is largely built, and this release's own contribution is saying
so precisely: `wolf --version` now names the build's commit and
never claims a release it is not (D57: a build made at the `v0.2.0`
tag prints the bare version; every other build answers
`0.2.0+dev.<commit>`), the observation record's `impl_version` carries
the same identity, and the release pairs with lupin 0.1.17 as the
reference interpreter at pin `addcd7f`, a pin that is part of this
release's identity (D57 again). Everything below landed on trunk
between the two tags, told by campaign.

### The release tier, and the M2 declaration

Campaign c09 made `wolf build --release` real: the backend emits
textual LLVM IR handed to the system clang, with no llvm-sys, no
inkwell and zero new dependencies (D33), plus `--emit=llvm-ir`, dominator-scoped
GVN, check elimination, clustering with byte-identical output at 1, 4
and 8 threads, and PGO's instrumented loop (`.wprof`, `wolf profile
show|merge`, branch weights). The campaign also found and killed a
real miscompile: a false `!noalias` pair from region-freshened
inlining let LLVM delete a load; the fix made the role the aliasing
unit, at measured zero cost. The perf campaigns that followed (c23
range facts, c24 the six mechanisms, c29 the hot header, plus the a2,
a5, b3, d2 and fmt lanes) drove the thirteen-kernel T1 suite from
0.476x to over 1.0, and on 2026-08-24, after three consecutive
nightly holds under `cargo xtask bench ritual` (geomean 1.099, 1.066,
1.080 vs naive `clang -O3`, two documented exceptions renounced by X3
and D25), M2 was declared (#89). The ritual treats a refuted gate as a
result rather than a failure; its ledger is in the repo.

### The platform matrix

The compiler came home (s59, c13): the macOS/aarch64 native tier
lights, with the Apple-arm64 C ABI (`wolf-abi-2`, differentially green
against Apple clang), Mach-O emission with dSYM and lldb parity, the
kqueue reactor, deterministic links, and all 21 linux-only test
headers flipped to runtime skips that distinguish environment from
breakage. The s127 sprint brought the release tier along: triple and
datalayout
is host-derived from clang's own emission (never hand-composed), and
macOS holds three-tier parity at full linux floors. A nine-commit
Windows sweep then hardened what the s59 flip exposed: environment
refusals became exit 2 (so Windows stopped reading every native suite
as a program refusal), fixtures learned RFC 8089 file URLs, and a
real product bug fell: `wolf add`'s git fetch now pins line-ending
config, so an `autocrlf` host no longer manufactures a false E1506.
CI runs six jobs across linux x86-64, macOS aarch64 and windows
x86-64; linux/aarch64 waits on a runner (#166).

### The toolchain

`wolf_pkg` (c11) is D33's covenant as running code: declarative
manifests with no expressions anywhere, MVS resolution, blake3
content identity, capability audit where a dependency acquiring
`exec` fails CI before the ledger moves, the transparency log's
Merkle half, `wolf publish`, `wolf vendor`, `replace`/`exclude`, and
spec §8. The C importer (c10, s46) landed as a versioned interface
artifact produced by an out-of-process libclang worker: the compiler
binary links no C frontend, and the importer names what it refuses.
`wolf doc` and script mode arrived with s53, whose rider created the
PAIRING file this release stamps.

### The claims become native

What v0.1.0 could only interpret or check now compiles. Concurrency
(c19): scope, spawn, channels and procs lower to machine code with
the defer law pinned on the release tier and a proc's argument record
copied at spawn (D14, D15, D16, D30). The io surface (c20): sixteen
fs builtins, sorted `read_dir`, and no false promise of atomic rename.
The crossing (c26) closed the checked-native split for builtins
entirely: net (with `error: timeout` firing for the first time on
any lane), JSON without an invented DOM surface, and process control
with zombie discipline. Generics (c21) monomorphize on one worklist with
D8 dedup; dispatch (c22) landed trait methods, `call.ind`, and the
two-word trait object ABI, completing the story from D28 to D47; the
value tier (c25) made regions and closures values, with capturing
closures as an (entry, env) pair through `call.ind`. The consumer
findings campaign (c17) turned wolf-std's friction into fixes,
including the byte-view lend analysis and the first code retirement
(E1015 → W1004).

### c27 — small debts

Five reopenings of the same discipline: front-end debts paid without
inventing syntax. Raw-literal decoding, `main`'s legal shapes (E0414),
scoped bottom for literal `assert(false)`, nested error rows
flattening (D51), declared-row-first widening (D52), the numeric
literal chapter `[type.numlit]` with int-adopts-float and trapping
float→int casts (D54), value-preserving `wrapping[T] as int` (D56),
and `List[mod.Type]` reading an imported type in bracket position.

### c28 — the constant-time tier

`#[consttime]` is a verified contract (D53): secret-tainted data
cannot decide a branch, index memory, pick a call target, or feed a
variable-time instruction. Each is refused fail-closed at the WIR rung
on every lane in every build mode, E1601–E1607, with spec/09 written
before the pass and assembly witnesses whose bodies contain zero
conditional branches.

### c29 — the hot header

Loop-carried header promotion, licensed only by existing proofs:
`_Wmain` on the b3 kernel gave back 8.1% of its instructions, ten
other kernels stayed byte-identical, and the campaign's real product
was a correction: the number it chased was an ops count misread as
an instruction count, and the instrument outvoted the narrative.

### c30 — signals

The program hears the signal: reception via a self-pipe trampoline,
`os_signal_listen/wait/raise` on every lane, and `os_random` over
getrandom/getentropy/BCryptGenRandom with no fallback and no seeding,
where failure traps. The `[os.signal]` and `[os.random]`
clauses wrote the platform matrix down, which pre-authorized
the macOS crossings.

### c31 — clustering

The shim travels with its spawner: summary v3 records `func.addr` as
a reachability edge, so a spawn shim or closure entry can never be
split away from the body that references it.

### c32 — codegen debts

The loop and the layout: the versioning pass routes live-outs against
the current CFG, token linearity refined to the edge target, and List
element stride rounds up to the element's alignment, witnessed on
session-shaped and mixed-width layouts.

### c33 — strings

`chars()` landed on every lane, and then the scalar got its type:
`char` is a Unicode scalar value (D58), four bytes, scalar-value
order, no arithmetic, `char as int` total and `int as char` trapping
on the surrogate gap and out of range, with `chars()` re-typed
`List[char]` and every caller migrated.

### c34 — the server

`wolf lsp` serves completion (keywords, scope names, typed members)
from a query that answers incomplete and broken buffers, measured
under the keystroke budget.

### c35 — the crashes

The compiler does not panic: str-match constant-folding builds no
orphan blocks, GVN scopes per continuation, and an integer literal
that cannot fit its type is E0415 at the front end, not a verifier
ICE. The one suspected oracle hole (#152) was ruled both-correct:
X3 has no hole at list-element provenance, only a width.

### c36 — the module explains itself

D59: directory = module by default, membership is the default and
standalone is a normative opt-out, E0301 explains both of its
situations, a silently unparseable sibling is E0202, and E0302 is
reachable from `wolf build`.

### c37 — the trap names its site

A trap's second stderr line says where (`at file:line:col`) while the
first line stays the byte-identical parsed ABI; per-site cold blocks
cost ~0.2% of .text; `[conf.trap.report]` and `[conf.trap.render]`
rule the shape, and D60 rules that the kind, not the exit status, is
the contract. The release cache learned that site coordinates are
part of a cluster's key.

### c38 — the origin

D61: the index chooses its origin, with `#![index(0|1)]` as a lexical
marker, 1-mode coupling inclusive ranges, the shift landing as one
checked subtraction on both executing lanes, and zero cost when the
marker is absent: 422 files, zero verdict flips.

### c39 — the book writes

The walls the language showed its first real user came down in one
sprint: comma-grouped binders (D63), tuple destructuring with
element-wise moves live on three lanes, `str + str` as
interpolation-append (D62) with mixes still refused, and list
slices as fresh-List copies with `for` over a slice. The acceptance
was the human's own scratch programs running unmodified on all three
tiers, byte-identical to lupin.

### The spec, the corpus, and this release's own fix

The conformance machinery hardened alongside: lane coverage became a
gated ratchet, `cargo xtask peel` reads the fail-fast ledger, and the
anchor registry now holds 403 anchors, including `gram.lex.ident`,
which the spec-extract bracket scanner had silently dropped twice by
pairing a bare `[` literal in prose with the next anchor's `]`
(F-0100, #170/#177; fixed this release, with the s126 shebang prose
as the pinned regression witness). At the tag the lane-coverage
floors stand at checked 235 / native 257 / release 257 / union 271 /
all-three 221.

v0.1.0's notes said the release tier, macros and the registry were
not built. The release tier is built and declared against its gate;
the registry protocol is built with its hosted half still waiting
(X7); macros remain the CTFE tier's future. The corpus ledger, not
this document, stays the authority on completeness.
