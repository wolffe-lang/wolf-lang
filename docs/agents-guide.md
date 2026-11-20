# An agent's guide to writing good wolf

This guide is for a model (or a person) about to write wolf. Read it before
the first line. It is current at **wolf 0.2.40** (`wolf 0.2.40 (wolfgang, pin
b333b9d)`), **lupin 0.1.62** and the standard library bundled in the 0.2.40
archive (wolf-std `be1d10fa`). No model has seen much wolf in training, so
what you think you remember about it is probably a guess; this file is what
the released toolchain actually does.

- The spec wins. [`spec/`](../spec/) is normative; read it at the tag you
  build with. When this file and the spec disagree, the spec is right and
  this file has a bug.
- Every `wolf` fence below is a whole program, compiled and run with the
  release archives named above, under both machines: the compiler (`wolf
  build`, then the binary) and the reference interpreter (`lupin run`). A
  fence whose header names a `lupin=` verdict is one where the two machines
  answer differently, and the header says how.
- The check is [`agents-guide-check.sh`](agents-guide-check.sh) beside this
  file. Each release runs it with that release's archives before the tag,
  and moves the version line above. At 0.2.40 it ran on macOS arm64.

```sh
WOLF=path/to/wolf LUPIN=path/to/lupin docs/agents-guide-check.sh docs/agents-guide.md
```

A fence header uses `expect=` for the compiler's verdict (default `exit:0`),
`lupin=` for the interpreter's (default: the same), `args=` and `stdin=` for
the program's input, `warn=` for a warning the build must print, and `nofmt`
for a fence that must not parse. A verdict is `exit:N`, `trap:KIND`,
`fail:CODE` or `unsupported` ("cannot compile this yet"). An `output` fence
after a program is its exact stdout.

## The ten rules

1. One program, one directory. A directory is a module, so a second `.lu`
   file with its own `main` beside yours is E0302, not a second program.
2. Run `wolf fmt`. Its layout is the only layout, and CI everywhere runs
   `wolf fmt --check`.
3. Spell mutation at the call site: `f(mut x)`, `f(take x)`,
   `(mut xs).push(v)`.
4. Absence and failure are rows, never sentinels: `-> T ! {tag}`, handled by
   `?`, `else` or a `match`. Traps are for broken contracts, not bad data.
5. Arithmetic is checked everywhere. Intended wraparound is `wrapping[T]`.
6. A `str` never changes and is `Copy`. Building one allocates; slicing,
   `trim`, `split`, `words`, `lines` and `bytes()` in a consumed position do
   not.
7. Regions are the allocator. Straight-line code needs no region syntax, but
   a loop that runs long, or reads its input a chunk at a time, puts one
   `region` around its body: outside a named region nothing is freed until
   the enclosing region ends.
8. If you state an output, you ran it. A claim about what an implementation
   answers is a test or it is a rumour, and that includes what the compiler
   will say about your program.
9. Measure the binary, not the version string. `wolf --version` line 1 names
   its pin. `wolf run --checked` builds natively; the checked machine is
   `wolf conform-run FILE --json --checked`.
10. File what the language cannot do; do not paper over it. Name the gap and
    its issue number where the reader meets it.

In any hot output loop, collect bytes in a `List[byte]` and write them with
`fs_write_chunk` rather than calling `print` per line: `print` is one
`write(2)` per call (wolf-lang#408), and at a million lines that was 15 times
slower than a buffered writer on macOS and 8 times on Linux when it was
measured at 0.2.14 (see [Writing output](#writing-output)).

## What agents get wrong unless told

Each row is a failure seen in real agent-written wolf, with the sentence that
prevents it. They are written to be pasted into a brief.

| What goes wrong | Write this instead |
|---|---|
| Trusting an old note that says wolf cannot do something | Before writing around a limitation, check the CHANGELOG at your pin. If the limitation is gone, use the feature. |
| C-shaped counter loops (`var i = 0; while i < n { …; i += 1 }`) | `for i in 0..n` when the bounds are fixed, `for x in xs` for elements. Keep `while` only when the cursor moves by more than one, the bound changes, or the cursor outlives the loop. |
| Hand-written early returns, or `cond && action()` as a statement | `if c then stmt` for a one-statement guard, `?` to propagate, `else return v` to bail out. There is no `&&` statement form. |
| Binding `s.bytes()` inside a loop | Index `s.bytes()[i]` inline or iterate `for b in s.bytes()`; `let b = s.bytes()` copies every byte. |
| No region in a streaming or per-turn loop | Wrap the body of every loop that allocates per turn in `region NAME { }`; build what must outlive it outside, or copy it out with `copy region NAME { }`. |
| Building output with `+=` on a `str`, or `print` per line | Push bytes into one `List[byte]` and write it with `fs_write_chunk` per 64 KiB. |
| A catch-all `else \|_\|` | Handle every tag by name; read which tags the call raises first. |
| A fallible result left unhandled | W0601 is denied everywhere. When the compiler refuses something at an odd place, look for an unhandled fallible call nearby first. |
| `var k = 0` as an accumulator | `var k: int = 0`. An unannotated literal is `i32` and traps past 2^31. |
| Returning or storing a parameter it was only lent | A parameter without a mode is `read`; returning or storing it is E1002. `copy` it, or declare it `take`. |
| C operators and layout | Bitwise not is `!x` (`~x` is E0107). Braces open on the header line. A literal brace in a string is `{{` or `}}`. |
| A keyword as a field or function name | There are no raw identifiers; see [Names](#names) for the 50 keywords. |
| A function with a `mut` parameter used as a value | Not yet compilable (wolf-lang#917). A table of handlers is an id per row and one `match`. |
| Deep recursion over input | Parse and walk with explicit stacks. Native code traps `stack-overflow`; the checked machine and lupin stop far sooner. |
| `if let x = e; cond { }` | Specified for the next release and not in 0.2.40. Do not write it until a release carries it. |
| A checked-machine claim from `wolf run --checked`, or from a `wolf` on `PATH` | The checked machine is `wolf conform-run FILE --json --checked`, run with the pinned binary. Quote the command. |
| Reading exit 2 or 4 from `wolf build` as a skip | 2 is a rejected program, 4 is "cannot compile this yet". Neither is a skip. |
| An expected answer computed by code you also wrote | Expected answers come from the reference tool or the spec, never from a second program with the same misunderstanding. |

## The shape of the language

Bindings are `let` (immutable, fields included) and `var`. A statement ends
at a newline when the line's last token could end an expression, so an
expression continued onto the next line puts its operator at the end of the
line (`wolf fmt` joins it back when it fits). Generics use `[]`.

```wolf
fn main() {
    let name = "wolf"
    var count = 0
    count += 2
    let total = count + 40
    print("{name}: {total}")
}
```

```output
wolf: 42
```

`let` means the value, not only the name: assigning to a `let` or to a field
of one is E0410.

```wolf expect=fail:E0410
struct Row {
    cents: int,
}

fn main() {
    let r = Row { cents: 1 }
    r.cents = 5
    print("{r.cents}")
}
```

Every string literal interpolates: `{x}` is a hole, `{{` and `}}` are
literal braces, and `$` escapes nothing, so a literal `${` is `${{`. `+` and
`+=` join two `str`s, or a `str` and a `char`; `str + int` is E0409
(interpolate the number instead). `r"..."` is raw: no escapes, no holes.
`len` is bytes. There is no `s[i]`, and slices take byte offsets that must
land on code-point boundaries.

```wolf
fn main() {
    let host = "example.org"
    let n = 3
    print("{host} has {n} items, {{braces}} and $HOME${{PATH}}")
    var s = "hello" + ", " + "world"
    s += '!'
    print(s)
    print(r"C:\raw\path")
    print("{'é'} is {'é' as int}, {"é".len} bytes")
}
```

```output
example.org has 3 items, {braces} and $HOME${PATH}
hello, world!
C:\raw\path
é is 233, 2 bytes
```

Reach for the builtin `str` surface first: `s[a..b]` (traps `bounds` on a
bad offset), `s.get(a..b)` (the `none` row instead), `^n` from the end,
`find`, `rfind`, `contains`, `starts_with`, `ends_with`, `count`, the
zero-copy `split`, `words`, `lines`, `trim*`, `strip_prefix` and
`strip_suffix`, `chars()`, `bytes()`, and the one parser,
`to_int() -> int ! {parse}`. `words` and `trim` use Unicode `White_Space`;
`lines` splits on LF and drops one CR before it; `split` keeps empty fields.

```wolf
fn main() {
    let s = "héllo, wolf"
    print(s[0..1])
    print(s[^4..])
    print(s.get(1..2) else "<not a boundary>")
    print("{s.find("wolf") else -1}")
    for w in "  two   words ".words() {
        print("[{w}]")
    }
    print("{"a,,b".split(",").len} fields")
    print("{"42".to_int() else 0} {"4x2".to_int() else -1}")
}
```

```output
h
wolf
<not a boundary>
8
[two]
[words]
3 fields
42 -1
```

The data types are structs, enums with payloads, tuples, `List[T]`,
`Map[K, V]` (keys `str`, `int`, `char` or `bool`) and `Pool[T]`, and `{x}`
renders any of them. Patterns destructure where a value is born:
`let (q, r) = …`, `let Point { x, y } = p`, comma groups
`var lo = 0, hi = 10`, range arms `1..=9 =>`. The two-way expression is
`if c then a else b`.

```wolf
struct Point {
    x: int,
    y: int,
}

fn classify(n: int) -> str {
    match n {
        0 => "zero",
        1..=9 => "digit",
        _ => "big",
    }
}

fn main() {
    let p = Point { x: 3, y: 4 }
    let Point { x, y } = p
    let (q, r) = (17 / 5, 17 % 5)
    var lo = 0, hi = 10
    lo += 1
    hi -= 1
    let side = if x > y then "x" else "y"
    print("{x} {y} {q} {r} {lo} {hi} {side} {classify(7)} {p}")
}
```

```output
3 4 3 2 1 9 y digit Point { x: 3, y: 4 }
```

List literals, `range[T]`, error-set aliases and bare variant patterns run
on both machines (lupin has accepted them since 0.1.37):

```wolf
error ParseErrors = {parse, empty}

enum Color {
    Red,
    Green,
    Blue(int),
}

fn width(r: range[int]) -> int {
    r.end - r.start
}

fn first_number(xs: List[str]) -> int ! ParseErrors {
    if xs.len == 0 {
        return empty
    }
    xs[0].to_int()?
}

fn name(c: Color) -> str {
    match c {
        Color.Red => "red",
        Color.Green => "green",
        Color.Blue(n) => "blue {n}",
    }
}

fn main() {
    let cents = [340, 275, 100]
    let nothing: List[str] = []
    print("{cents[1]} {width(2..=7)} {nothing.len} {name(Color.Blue(9))}")
    print("{first_number(["12", "x"]) else -1} {first_number([]) else -1}")
}
```

```output
275 6 0 blue 9
12 -1
```

`[]` needs its element type from context; a bare `let xs = []` is E0419.
`range[T]`'s `end` is always exclusive (`(2..=7).end` is 8). The alias's own
name may be CapCase, as `ParseErrors` is: the case rule (W0603) is about the
tags, and that fence builds with warnings denied.

A pattern takes its scrutinee's type, so a literal that type cannot hold is
refused (E0415) where lupin, checking less, runs the program and traps:

```wolf expect=fail:E0415 lupin=trap:overflow
fn main() {
    let b: u8 = 7
    let s = match b {
        300 => "big",
        _ => "small",
    }
    print(s)
}
```

An integer literal pattern carries no sign: `-1 =>` is E0207. Match a
negative value with a guard, `n if n == -1 =>`.

```wolf expect=fail:E0207 nofmt
fn sign(n: int) -> str {
    match n {
        -1 => "minus one",
        _ => "other",
    }
}

fn main() {
    print(sign(-1))
}
```

A type that contains itself by value has no finite size (E0404); recurse
through a `List`, a `Map` or a `handle` into a `Pool`.

```wolf expect=fail:E0404
struct Node {
    value: int,
    next: Node,
}

fn main() {
    print("never")
}
```

## Names

There are exactly 50 reserved keywords and no raw identifiers:

`as` `asm` `assume` `borrow` `break` `comptime` `const` `continue` `copy`
`defer` `distinct` `dyn` `else` `enum` `errdefer` `export` `extern` `false`
`fn` `for` `freeze` `handle` `if` `impl` `import` `in` `let` `loop` `match`
`move` `mut` `proc` `pub` `region` `return` `scope` `select` `shared`
`spawn` `struct` `take` `trait` `true` `type` `unsafe` `use` `var` `weak`
`when` `while`

They bite as field and function names (`type`, `handle`, `copy`, `move`,
`in`, `proc`, `region`, `scope`, `take`, `use`, `impl` all look like good
names). The error is a parse error that does not mention the keyword, so
recognise it by its shape: "this `{` is never closed" at the struct.

```wolf expect=fail:E0202 lupin=fail:E0008 nofmt
struct Token {
    type: int,
}

fn main() {
    let t = Token { type: 1 }
    print("{t.type}")
}
```

`then`, `error`, `from`, `timeout`, `self`, `rc`, `pool` and `cap` are
contextual and stay usable as names. A module's identity is its last path
segment, so `std.x.json` and `std.json` cannot both be imported by one
program.

A declaration wins its name over the prelude, the names every file can use
without an import, and W0304 warns, which is fatal under `--deny-warnings`.
The prelude grows with almost every release, so a name that was free when
you wrote it can start failing the build when you move the pin:

```wolf expect=fail:W0304 lupin=exit:0
fn min(a: int, b: int) -> int {
    if a < b then a else b
}

fn main() {
    print("{min(3, 4)}")
}
```

`wolf prelude` prints the list your binary has (244 names at 0.2.40, and
`--json` for a program to read). These 128 joined the prelude between 0.2.14
and 0.2.40; none left. If code you are reading defines one of them itself, it
was written for an older pin.

<!-- prelude-since:begin -->
- Types and intrinsics: `Proc` `Scope` `VaList` `align_of` `offset_of`
  `fence` `never` `va_arg` `va_copy` `va_end` `va_start`
- Bytes and arguments: `bytes_count` `bytes_find` `env_arg0`
  `env_args_bytes` `env_unset`
- Files: `fs_access` `fs_adopt` `fs_copy_chunk` `fs_create_dir_at`
  `fs_create_dir_mode` `fs_fstat_record` `fs_link` `fs_link_at` `fs_lock`
  `fs_lstat` `fs_lstat_at` `fs_mkfifo` `fs_open_at` `fs_open_perm`
  `fs_read_at` `fs_read_dir_entries` `fs_read_dir_entries_at`
  `fs_read_link` `fs_realpath` `fs_remove_at` `fs_remove_dir_at` `fs_seek`
  `fs_set_mode` `fs_set_mode_fd` `fs_set_nonblock` `fs_set_owner`
  `fs_set_owner_fd` `fs_set_times` `fs_stat` `fs_stat_at` `fs_statfs`
  `fs_symlink` `fs_symlink_at` `fs_sync` `fs_sync_dir` `fs_tell`
  `fs_truncate` `fs_unlock` `fs_write_some`
- Sockets: `net_peer_cred` `net_write_some`
- Processes and identity: `os_arch` `os_chdir` `os_chroot` `os_clock_ns`
  `os_core_limit` `os_core_off` `os_egid` `os_error` `os_error_text`
  `os_euid` `os_exec` `os_fork` `os_getpriority` `os_gid` `os_groups`
  `os_isatty` `os_name` `os_pgid` `os_pid` `os_pipe` `os_ppid`
  `os_proc_pid` `os_set_core_limit` `os_set_ids` `os_set_pgid`
  `os_setpriority` `os_setsid` `os_sid` `os_spawn_fds` `os_spawn_job`
  `os_spawn_session` `os_uid` `os_umask` `os_wait_poll` `os_wait_poll_core`
  `os_wait_status` `os_wait_status_core`
- Waiting: `os_ready_close` `os_ready_open` `os_ready_set` `os_ready_wait`
  `os_wait_ready`
- Signals: `os_exit_signal` `os_signal_action_one` `os_signal_default`
  `os_signal_default_one` `os_signal_ignore` `os_signal_ignore_one`
  `os_signal_ignored` `os_signal_last` `os_signal_number` `os_signal_poll`
  `os_signal_realtime` `os_signal_send` `os_signal_send_all`
  `os_signal_send_group` `os_signal_text` `os_signal_wait_fd`
  `os_signal_wait_fds`
- Terminals: `os_term_attr` `os_term_chars` `os_term_foreground`
  `os_term_mode` `os_term_open_pty` `os_term_pty_name`
  `os_term_restore_at_exit` `os_term_set_attr` `os_term_set_foreground`
  `os_term_set_mode` `os_term_set_size` `os_term_size`
<!-- prelude-since:end -->

Do not name a function `print`, `read_line`, `assert` or `min` either; they
have always been there.

## Loops, queries and guards

Use a range when the bounds are fixed for the walk: `for x in xs` when you
need the elements, `for i in 0..xs.len` when you need indices. A range
stores its endpoints and evaluates them once, before the walk; it does not
build a list of indices. `0..n` excludes `n` and `0..=n` includes it.

```wolf
fn index(xs: List[int], needle: int) -> int {
    for i in 0..xs.len {
        if xs[i] == needle then return i
    }
    -1
}

fn main() {
    var xs = [2, 4, 6]
    for i in 0..xs.len {
        xs[i] += 1
    }
    var total = 0
    for i in 0..6 {
        if i == 1 then continue
        total += i
    }
    print("{index(xs, 7)} {total} {index([], 1)}")
}
```

```output
2 14 -1
```

The range advances even when `continue` skips the rest of the body, and an
empty range has no turns. Keep `while` when the cursor changes by more than
one, when the next turn depends on a bound that changes, or when the cursor
must survive the loop: a byte scanner, a retry loop and
`while out.len < target` need not become ranges. Do not mechanically replace
`while i < xs.len` when the body grows `xs`, because the range captured the
original length. Iterating elements lends the collection, so to update
elements in place, walk an index range.

The bundled standard library makes the common queries methods on `List`.
They are eager: `filter(...).map(...)` builds an intermediate list and then
the result, not a lazy pipeline.

```wolf
fn main() {
    let xs = (0..6).collect()
    let ys = xs.filter(fn(x: int) { x % 2 == 0 }).map(fn(x: int) { x * 10 })
    let total = ys.fold(0, fn(a: int, x: int) { a + x })
    let down = ys.sorted_by(fn(a: int, b: int) { a > b })
    let up = down.sorted()
    var work = copy down
    (mut work).sort_by(fn(a: int, b: int) { a < b })
    let indexed = ys.enumerate()
    let paired = ys.zip([7, 8])
    print("{total} {ys.sum()} {down[0]} {up[0]} {indexed[1].1} {paired.len} {paired[1].1}")
}
```

```output
60 60 40 0 20 2 8
```

| Method | Result and cost |
|---|---|
| `map(f)` | a new list of transformed elements |
| `filter(keep)` | a new list of the kept elements, in order |
| `fold(seed, step)` | a left-to-right reduction; no list of its own |
| `sum()` | checked addition over a `List[int]` |
| `(mut xs).sort_by(less)` | a stable sort in place |
| `sorted_by(less)`, `sorted()` | a sorted copy; `less(a, b)` returns `bool`, not an ordering |
| `enumerate()` | a new list of `(index, value)` pairs from zero |
| `zip(ys)` | a new list of pairs, as long as the shorter input |
| `(a..b).collect()` | a materialised list; a plain range loop does not need it |

`any`, `all`, `count_where`, `index_where`, `find_where`, `min_by` and
`max_by` are methods too; the last four answer the `none` row when there is
no result. A function value literal has no return type annotation
(`fn(a: int, b: int) { a > b }`). `par(f)` is the parallel combinator: it
creates tasks, keeps result order, and is not a free replacement for a
serial `map`. The receiver is read, not consumed, and keeping heap-valued
elements copies them, so a chain over nested lists costs far more than one
over integers. Use a loop when you need one pass, side effects, `?` on each
element, or control over copying.

Write `if condition then statement` for a one-statement guard, with a call,
`return`, `break` or `continue`. Use braces for declarations, assignments and
anything longer. Do not write `condition && action()`: the logical operators
take and give booleans and are not statements.

```wolf
fn positive(n: int) -> int ! {negative} {
    if n < 0 then return negative
    n
}

fn twice(n: int) -> int ! {negative} {
    positive(n)? * 2
}

fn or_zero(n: int) -> int {
    let value = twice(n) else return 0
    value
}

fn main() {
    print("{or_zero(-1)} {or_zero(3)} {twice(-1) else 0}")
}
```

```output
0 6 0
```

`?` propagates the failure out of the current function, whose row must admit
it. `else return 0` handles the failure here and returns an ordinary value
from this function. `else 0` substitutes a value and carries on. Use the
row directly rather than unpacking it into a boolean and testing that.

## Modes

A parameter is `read` (no keyword, the default: immutable for the call, and
the caller keeps it), `mut` (exclusive, in and out) or `take` (consumed). The
call site spells the mode, and a method with a `mut self` receiver is called
as `(mut x).method()`. A `for` over a place holds a read claim on it for the
whole loop, so writing to it inside is E1013.

```wolf
struct Counter {
    hits: int,
}

impl Counter {
    fn reset(mut self) {
        self.hits = 0
    }
}

fn bump(mut c: Counter) {
    c.hits += 1
}

fn total(xs: List[int]) -> int {
    var sum = 0
    for x in xs {
        sum += x
    }
    sum
}

fn consume(take xs: List[int]) -> int {
    xs.len
}

fn main() {
    var c = Counter { hits: 0 }
    bump(mut c)
    bump(mut c)
    let before = c.hits
    (mut c).reset()
    var xs = List[int]()
    (mut xs).push(4)
    (mut xs).push(5)
    let t = total(xs)
    let n = consume(take xs)
    print("{before} {c.hits} {t} {n}")
}
```

```output
2 0 9 2
```

A value used after `take` is E1001 under the compiler; lupin reaches the same
fault when it runs, as `trap(use-after-move)`. A handle type whose closer is
`close(take h)` gets use-after-close as that same error.

```wolf expect=fail:E1001 lupin=trap:use-after-move
fn consume(take xs: List[int]) -> int {
    xs.len
}

fn main() {
    var xs = List[int]()
    (mut xs).push(1)
    let n = consume(take xs)
    print("{n} {xs.len}")
}
```

Leaving the mode off a `mut` argument is E1007, with a fix-it:

```wolf expect=fail:E1007
fn bump(mut n: int) {
    n += 1
}

fn main() {
    var n = 1
    bump(n)
    print("{n}")
}
```

A `read` parameter is lent, never given. Returning it, sending it, storing
it into module state or a `mut` parameter, or carrying it into a `take` is
E1002; lending it `mut`, taking it or writing through it is E1014. Only
values that can reach shared storage are refused (a `List`, a `Map`, a
`Pool`, or a type parameter). Write `copy`, or make the parameter `take`.
lupin runs this program, so a lupin run is not evidence that a move is
legal:

```wolf expect=fail:E1002 lupin=exit:0
fn same(xs: List[int]) -> List[int] {
    xs
}

fn main() {
    let a = [1, 2, 3]
    let b = same(a)
    print("{b.len}")
}
```

`copy` is deep on every tier: a copied `List`, `Map`, struct or tuple shares
nothing with the original. Binding a struct element of a list moves it out,
so write `let x = copy xs[i]` when you mean to read it.

## Memory: let the regions work

Every function allocates in its caller's current region, which is why
ordinary wolf has no region syntax. `region name { }` makes one that dies,
all at once, at `}`. That is the tool for per-turn scratch, and it reclaims
`str` bytes too.

```wolf
fn main() {
    var kept = 0
    for i in 0..1000 {
        region scratch {
            let line = "item {i}: {i * i}"
            kept += line.len
        }
    }
    print("{kept}")
}
```

```output
15427
```

Outside a named region the ambient region frees nothing until it ends. A
long-running loop, a server's request loop, an editor's key loop and a
streaming copy each need one region per turn, or memory grows with the
input: wolf-lang#416 records a `cat`-shaped loop at 340 MB copying 256 MiB,
and 2.4 MB with a region around the body. `break`, `continue` and `return`
leave a region block normally.

A value allocated in a region may not outlive it: returning it, sending it
or storing it outside is E1010. That includes every built `str` (an
interpolation with a hole, `+`, `upper`, `lower`, `repeat`, `replace`,
`str_from_utf8`).

```wolf expect=fail:E1010 lupin=trap:region-fault
fn label(n: int) -> str {
    region scratch {
        let s = "n={n}"
        s
    }
}

fn main() {
    print(label(3))
}
```

The fix is to build the value in the caller's region (move the `let` out of
the block), or to let the block's value leave with `copy region`:

```wolf
fn label(n: int) -> str {
    copy region scratch {
        let parts = "n={n}"
        parts
    }
}

fn main() {
    print(label(3))
}
```

```output
n=3
```

A call may keep what it is handed, so passing a region's value to a call
that also takes a `mut` argument declared outside the block is E1010, and
`push` is such a call. lupin runs it. Write `push(copy label)`, or build the
value outside the region:

```wolf expect=fail:E1010 lupin=exit:0
fn main() {
    var kept = List[str]()
    for i in 0..3 {
        region turn {
            let label = "item {i}"
            (mut kept).push(label)
        }
    }
    print("{kept.len}")
}
```

For a buffer reused every turn, keep the list outside the loop and `reset`
it; `reserve(n)` grows it to exactly `n` once. Never `reserve(len + k)` per
append: that reallocates and copies every time.

```wolf
fn main() {
    var buf = List[byte]()
    (mut buf).reserve(4096)
    var turns = 0
    for i in 0..3 {
        (mut buf).reset()
        for b in "turn {i};".bytes() {
            (mut buf).push(b)
        }
        turns += buf.len
    }
    print("{turns} {buf.len}")
}
```

```output
21 7
```

A `spawn proc` body owns its own ambient region, so a `str` built in a proc
and sent on a channel is E1010 too: build it in the spawner and pass it in.
`let r = region()` is the first-class form, `in r { }` places allocations in
it, `freeze r` makes the whole graph deeply immutable and shareable, and a
region may carry a byte budget whose breach traps `alloc-contract` (contained
at a proc boundary as `fault(alloc-contract)`):

```wolf expect=trap:alloc-contract
fn main() {
    region small(cap: 64) {
        var xs = List[int]()
        for i in 0..1000 {
            (mut xs).push(i)
        }
        print("{xs.len}")
    }
}
```

`region_bytes(r)` reads a region's ledger. Its units differ per tier, so
compare relations, never numbers. `shared T` is reference counted and must
be acyclic (E1006; use `weak` for back-edges). A `handle T` into a `Pool[T]`
traps `stale-handle` on a freed slot. A `List` slice `xs[a..b]` is a fresh
copy; a `str` slice is a view.

The stack is the other memory. A native main thread that overflows it traps
`stack-overflow` (exit 134) instead of dying of a bare signal, and the
checked machine and lupin give up far sooner (both answer `unsupported`).
Do not recurse on the depth of your input; keep an explicit stack.

```wolf expect=trap:stack-overflow lupin=unsupported
fn depth(n: int) -> int {
    if n == 0 then return 0
    1 + depth(n - 1)
}

fn main() {
    print("{depth(100000000)}")
}
```

## Errors: rows, not exceptions

`-> T ! {tag, Tag(Payload)}` declares a row: payload-free marks are
lowercase, a tag carrying data is CapCase (W0603). `-> !T` infers a private
row; a `pub` item states its row, and spelling a multi-tag row inline on a
`pub` item draws W0602 (name the set with `error Name = {…}`). `?`
propagates when your row covers the callee's. There is no `From`, no
unwinding and no exception: an error return is ordinary control flow.

```wolf expect=exit:1
fn parse_port(text: str) -> int ! {parse, out_of_range} {
    let n = text.to_int()?
    if n < 1 || n > 65535 {
        return out_of_range
    }
    n
}

fn describe(text: str) -> str {
    let port = parse_port(text) else |e| match e {
        parse => { return "not a number" },
        out_of_range => { return "out of range" },
    }
    "port {port}"
}

fn main() -> int ! {parse, out_of_range} {
    defer print("defer runs on every exit")
    print(describe("8080"))
    print(describe("http"))
    print(describe("70000"))
    print("{parse_port("x") else 0}")
    let p = parse_port("0")?
    print("unreachable {p}")
    0
}
```

```output
port 8080
not a number
out of range
0
defer runs on every exit
error: out_of_range
```

What that program shows:

- `v else fallback` substitutes; `v else |e| match e { … }` discriminates,
  and an arm may `return` a value or re-raise a tag.
- A row that leaves `main` prints `error: <tag>` on stdout and exits 1, on
  both machines. That is the quickest witness of which tag a function
  raised, and exactly wrong for a command-line tool: handle rows before
  `main` ends.
- `defer` runs at scope exit (per turn inside a loop); `errdefer` only on
  error paths. A trap runs no `defer`.
- `"{e}"` renders the tag's name, the fastest way to see what a call raised.

Re-raising a tag from a handler needs a declared row. In `fn main() -> !int`
the same arm is refused as "cannot compile this yet" (lupin runs it):

```wolf expect=unsupported lupin=exit:1
fn main() -> !int {
    let n = "x".to_int() else |e| match e {
        parse => { return parse },
    }
    print("{n}")
    0
}
```

and with the row declared it builds:

```wolf expect=exit:1
fn main() -> int ! {parse} {
    let n = "x".to_int() else |e| match e {
        parse => { return parse },
    }
    print("{n}")
    0
}
```

```output
error: parse
```

The recoverable reads answer `none`: `xs.pop()`, `xs.get(i)`, `first`,
`last`, `s.get(a..b)` and `m[k]` on a `Map`. `m[k] = v` inserts; `m[k] += v`
is E0417 (write `m[k] = (m[k] else 0) + v`).

```wolf
fn main() {
    var stack = List[int]()
    (mut stack).push(7)
    let top = (mut stack).pop() else 0
    let again = (mut stack).pop() else -1
    var tally = Map[str, int]()
    for w in "a b a".words() {
        tally[w] = (tally[w] else 0) + 1
    }
    print("{top} {again} {tally["a"] else 0} {tally["zzz"] else 0} {stack.get(5) else -9}")
}
```

```output
7 -1 2 0 -9
```

Three diagnostics guard rows. Discarding a fallible result is W0601, a
warning every serious build denies. An operator on a `!T` is E0409 on either
side (`n? + 1` and `(n else 0) + 1` are the fixes). And `x else 0 + y` means
`x else (0 + y)` (W0318): parenthesise.

```wolf expect=fail:W0601 lupin=exit:0
fn main() {
    var xs = List[int]()
    (mut xs).pop()
    print("{xs.len}")
}
```

```wolf expect=fail:E0409
fn five() -> int ! {parse} {
    "5".to_int()?
}

fn main() {
    print("{five() + 1}")
}
```

lupin's check is narrower: it refuses an operator on a direct call to a
fallible function, but runs an operator on a name bound to a row and prints
`6`. An interpreter run is not evidence that an operator on a row is legal.

```wolf expect=fail:E0409 lupin=exit:0
fn main() {
    let n = "5".to_int()
    print("{n + 1}")
}
```

The warnings that matter in practice, all in [warnings.md](warnings.md):
W0601 (a discarded fallible result), W0602 (an anonymous multi-tag row on a
`pub` item), W0603 (a tag's case), W0304 (shadowing a prelude name), W0313 (a
`pub` item without `///`), W0318 (`else` precedence) and W0801 (a bare name
in a pattern that binds a fresh name where a test was meant, so the arm
matches everything).

## Arithmetic

Arithmetic is checked in every profile: overflow, division by zero and an
out-of-range cast trap. `/` truncates toward zero and `%` takes the
dividend's sign. Intended wraparound is the `wrapping[T]` type. `byte` is an
unsigned octet, not an integer: its operators widen to `int`, `n as byte`
keeps the low eight bits (the one narrowing cast that never traps), and a
literal never adopts it (`65 as byte`). Float `%` is `fmod`; negate a float
with unary minus, never `0.0 - x` (W0402).

An integer literal with nothing to take a type from is `i32`. `var k = 0`
followed only by literal arithmetic stays `i32` and traps past 2^31 on both
machines:

```wolf expect=trap:overflow
fn main() {
    var k = 0
    k += 1500000000
    k += 1500000000
    print("{k}")
}
```

An `int` operand later on makes the compiler type `k` as `int`, but lupin
keeps it `i32` and traps (wolf-interp#173), so annotate every accumulator
that can grow, `var k: int = 0`, and both machines agree:

```wolf lupin=trap:overflow
fn big() -> int {
    3000000000
}

fn main() {
    var k = 0
    k += big()
    k += 1
    print("{k}")
}
```

```output
3000000001
```

```wolf
fn main() {
    var k: int = 0
    for _ in 0..3 {
        k += 1500000000
    }
    let h: wrapping[u64] = 18446744073709551615
    let wrapped = h + 1
    print("{k} {(wrapped & 255) as int}")
    print("{300 as byte} {-1 as byte} {3.7 as int} {-7 / 2} {-7 % 2} {7.5 % 2.0}")
}
```

```output
4500000000 0
44 255 3 -3 -1 1.5
```

Leaving the wrapping domain is a checked conversion: `h as int` traps when
the value does not fit `int`. Mask or reduce first.

```wolf expect=trap:overflow
fn main() {
    let h: wrapping[u64] = 18446744073709551615
    print("{h as int}")
}
```

Bitwise not is `!` on an integer, at the operand's own width, and `e & !m`
clears the bits of `m`. C's `~x` does not parse (E0107), and the compiler
offers `!x`:

```wolf
fn main() {
    let x = 5
    let flags: u8 = 15
    let mask: u8 = 5
    print("{!x} {!flags} {flags & !mask}")
}
```

```output
-6 240 10
```

```wolf expect=fail:E0107 nofmt
fn main() {
    let x = 5
    print("{~x}")
}
```

## Traits and comptime

Traits are nominal, and generic bodies typecheck against their bounds alone:
an operator on a bare `T` is E0501, and the fix is the bound. An operator on
a user type dispatches through `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg`,
`Eq` and `Ord` by name; nothing is synthesised. `type X = distinct Y` is the
adapter with its own impl set.

```wolf
trait Add {
    fn add(self, other: Self) -> Self
}

struct Money {
    cents: int,
}

impl Add for Money {
    fn add(self, other: Self) -> Self {
        Money { cents: self.cents + other.cents }
    }
}

fn total[T: Add](xs: List[T], zero: T) -> T {
    var acc = copy zero
    for x in xs {
        acc = acc + x
    }
    acc
}

fn main() {
    var wallet = List[Money]()
    (mut wallet).push(Money { cents: 250 })
    (mut wallet).push(Money { cents: 199 })
    let sum = total(wallet, Money { cents: 0 })
    print("{sum.cents}")
}
```

```output
449
```

Functions are values, capturing or not, and a capturing closure copies its
captures when it is made:

```wolf
fn apply(f: fn(int) -> int, n: int) -> int {
    f(n)
}

fn main() {
    let offset = 10
    let shift = fn(x: int) x + offset
    print("{apply(shift, 5)} {apply(fn(x: int) x * 2, 5)}")
}
```

```output
15 10
```

A function type carries no modes, so a function with a `mut` or `take`
parameter cannot be a value yet (wolf-lang#917; lupin runs it). A table of
handlers is an id per row and one `match`:

```wolf expect=unsupported lupin=exit:0
fn bump(mut n: int) {
    n += 1
}

fn main() {
    let f = bump
    var n = 1
    f(mut n)
    print("{n}")
}
```

Module-level `const`, `let` and `var` compile. Their initialisers are
evaluated at compile time; a `const` has no storage, a `let` is read-only
data. They hold integers, `byte`, `bool`, floats and `str` (in a `const` or
a `let`):

```wolf expect=unsupported
const AREA: int = 12 * 12

let GREETING: str = "hello"

fn main() {
    print("{AREA} {GREETING}")
}
```

```output
144 hello
```

A module `var` is shared by every task, so it is read and written only inside
`unsafe { }`, with a `# Safety:` comment saying what keeps it race-free
(E1301):

```wolf expect=fail:E1301
var calls: int = 0

fn bump() -> int {
    calls += 1
    calls
}

fn main() {
    print("{bump()}")
}
```

Module state of any other type, a `List` among them, cannot compile yet. A
zero-argument function is the way to a constant that is not static data:

```wolf expect=unsupported lupin=unsupported
let PRIMES: List[int] = [2, 3, 5]

fn main() {
    print("{PRIMES.len}")
}
```

```wolf
fn primes() -> List[int] {
    [2, 3, 5]
}

fn main() {
    print("{primes().len}")
}
```

```output
3
```

Comptime is the compiler's engine (lupin refuses every `comptime fn`) and a
sandbox with no ambient anything: the clock, the environment, the filesystem
and the network are refused by name (E0701). There are no macros.

```wolf expect=fail:E0701 lupin=unsupported
comptime fn home() -> str {
    env_get("HOME") else ""
}

fn main() {
    print(home())
}
```

## Concurrency

`scope s { s.spawn(fn() { … }) }` joins every task at `}`; a task that fails
cancels its siblings and re-raises at the scope's exit. Report results
through channels: a task that writes a captured `var` is E1101 on both
machines (under `--deny-warnings` the compiler reports its companion warning
W1101 first). `select` picks a ready arm, `when (a, b) { }` acquires a whole
set at once (a nested `when` is E1103), and `spawn proc f(args)` starts a
supervised failure domain whose exit reason is a value. Schedules are seeded
and replayable (`wolf test --schedules=N`, `--replay=`); a program whose
output depends on the seed is wrong.

```wolf
fn main() -> !int {
    let ch = channel[int](4)
    scope s {
        s.spawn(fn() {
            for i in 1..=3 {
                ch.send(i * i)?
            }
        })
    }
    var sum = 0
    for _ in 0..3 {
        sum += ch.recv()?
    }
    print("{sum}")
    0
}
```

```output
14
```

```wolf expect=fail:E1101 warn=W1101
fn main() {
    var total = 0
    scope s {
        s.spawn(fn() {
            total += 1
        })
    }
    print("{total}")
}
```

`ch.send(v)` is `() ! {closed, cancelled}`: a send whose row nobody reads is
the W0601 discard; `?` hands it to the task's scope.

Most large wolf programs (an editor, a terminal multiplexer, a server) do
not use tasks for I/O. They wait in one place for whatever is ready:

- `os_wait_ready(entries, set, deadline_ms)` waits on fs handles and net
  handles together (bit 61 marks a net handle, bit 62 asks about writing),
  with a set of signal meanings and a deadline, and answers every ready
  entry with its facts (1 read, 2 write, 4 hangup, 8 error) plus the one
  signal meaning taken. It costs the length of the list.
- `os_ready_open`, `os_ready_set`, `os_ready_wait` and `os_ready_close` are a
  readiness set the kernel holds (epoll on Linux, kqueue on macOS); a wait
  costs what is ready, not what is held. `std.poll` wraps it.
- `net_write_some` and `fs_write_some` write what the peer will take now and
  say how much; `fs_set_nonblock` makes a handle non-blocking.
- `os_term_open_pty`, `os_spawn_session` and `os_setsid` give a program a
  pseudo-terminal and a child in a session of its own.

lupin declines every wait that would reach the host, and says so. `os_fork`
answers `busy` while any task is live.

## Writing a command-line program

| Need | Spelling | Notes |
|---|---|---|
| arguments | `env_args() -> List[str]` | excludes argv[0]; `-` passes through as a word; a non-UTF-8 argument traps `argument-utf8` |
| arguments as bytes | `env_args_bytes() -> List[List[byte]]`, `env_arg0() -> str` | every argument with argv[0] first; the name as invoked |
| exit status | `main -> int`, or `os_exit(n)` | only the low byte survives; `os_exit` runs no `defer` |
| stdout and stderr text | `print(s)` (adds `\n`), `print_raw(s)`, `eprint`, `eprint_raw` | one `write` per call, unbuffered; write errors are dropped (wolf-lang#408) |
| stdin lines | `read_line() -> str ! {eof, io, utf8}` | strips `\n` and `\r\n`; a last line with no newline is still a line |
| standard streams as bytes | `fs_read_chunk(0, n) -> List[byte] ! {eof, io}`, `fs_write_chunk(1, bytes) -> () ! {invalid, io}` | handles 0, 1 and 2 are the descriptors the process started with; end of input is the `eof` row |
| files | `fs_open`, `fs_open_mode`, `fs_open_perm`, `fs_read_chunk`, `fs_write_chunk`, `fs_copy_chunk`, `fs_fstat`, `fs_close` | the first handle an open answers is 3; `std.fs` wraps them with named rows |
| why a call failed | `os_error() -> int`, `os_error_text(code) -> str` | the host's number for the last failure, and its text |
| environment | `env_get(name) -> str ! {missing, utf8}` | |

A `cat`-shaped skeleton. Operands default to `-`, each failure prints the
GNU shape `prog: operand: reason` on stderr and sets status 1 without
stopping, bytes go through untouched, each read turn has its own region, and
no row escapes `main`:

```wolf args=-,does-not-exist stdin=one\ntwo\n expect=exit:1 lupin=exit:0
fn copy_fd(fd: int) -> () ! {io} {
    var stop = false
    while !stop {
        region turn {
            let chunk = fs_read_chunk(fd, 65536) else |e| match e {
                eof => {
                    stop = true
                    List[byte]()
                },
                io => { return io },
            }
            fs_write_chunk(1, chunk) else |e| match e {
                invalid => { return io },
                io => { return io },
            }
        }
    }
}

fn cat_one(path: str) -> () ! {not_found, denied, io} {
    if path == "-" {
        copy_fd(0)?
        return
    }
    let fd = fs_open(path)?
    defer fs_close(fd) else |_| {}
    copy_fd(fd)?
}

fn main() -> int {
    var paths = env_args()
    if paths.len == 0 then (mut paths).push("-")
    var status = 0
    for p in paths {
        cat_one(p) else |e| match e {
            not_found => {
                eprint("mycat: {p}: No such file or directory")
                status = 1
            },
            denied => {
                eprint("mycat: {p}: Permission denied")
                status = 1
            },
            io => {
                eprint("mycat: {p}: I/O error")
                status = 1
            },
        }
    }
    status
}
```

```output
one
two
```

Notes on that program:

- The message is yours to write. The rows are the toolchain's vocabulary
  (`not_found`, `denied`, `exists`, `io`, …), not `strerror` text; map each
  to the wording of the tool you match, and keep `io` as the catch-all.
  argv[0] is not in `env_args()`; `env_arg0()` is the name as invoked and
  `os_exe()` the full path.
- A read of handle 0 acts on the descriptor itself: it starts where the
  shared offset stands and moves it, and a pipe, a socket or a terminal is
  read as what it is. A write to 1 or 2 lands in order with `print` and
  `eprint` around it. A write to a closed descriptor is `io`.
- A streaming read allocates a fresh `List[byte]` per call, which is why the
  loop body is a region. `std.fs.read_into` reads into a buffer the caller
  holds and resets.
- Usage errors exit 1 in the coreutils family, not 2: `basename`, `dirname`,
  `yes` and `echo` all exit 1 on a usage error in GNU coreutils 9.11. Exit 2
  is the convention elsewhere (`grep`, `diff`), so take the status from the
  utility you are matching. Return it from `main -> int`; reach for
  `os_exit` only to stop deep inside a call, knowing that no `defer` runs.
- Handle `-` yourself; nothing in the language treats it specially.
- `lupin run` passes no arguments, so the interpreter runs this program as
  if it had none (it reads stdin and exits 0). Keep the logic a tool needs
  (option parsing, splitting, formatting) in functions over `str` and
  `List[byte]` that both machines can test, and keep the process edge thin.

The kernel can move the bytes itself: `fs_copy_chunk(src, dst, max)` copies
up to `max` bytes between two handles (on Linux it tries `copy_file_range`,
`sendfile` and `splice`) and answers how many, with `eof` at the end.

```wolf stdin=copied\040by\040the\040kernel\n
fn main() -> int ! {io} {
    var total = 0
    loop {
        let n = fs_copy_chunk(0, 1, 65536) else |e| match e {
            eof => { break },
            io => { return io },
        }
        total += n
    }
    eprint("{total} bytes")
    0
}
```

```output
copied by the kernel
```

`bytes_find(xs, b, from)` and `bytes_count(xs, b)` search a byte list with
vector instructions, much faster than a `for b in chunk` loop over a large
buffer:

```wolf
fn main() {
    let text = "one\ntwo\nthree\n"
    let lines = bytes_count(text.bytes(), 10 as byte)
    let first = bytes_find(text.bytes(), 10 as byte, 0) else -1
    let missing = bytes_find(text.bytes(), 33 as byte, 0) else -1
    print("{lines} {first} {missing}")
}
```

```output
3 3 -1
```

`os_error()` is the host's number for the last failed call and
`os_error_text(code)` its text, which is what a tool needs to print GNU's
exact message:

```wolf
fn main() {
    let fd = fs_open("does-not-exist") else |e| {
        let code = os_error()
        print("{e} {code} {os_error_text(code)}")
        return
    }
    print("opened {fd}")
}
```

```output
not_found 2 No such file or directory
```

By default a write to a pipe whose reader has gone kills a native wolf
program with `SIGPIPE`, silently, as it kills `cat`: `prog | head -1` leaves
status 141. A program that would rather see the failure ignores
`BROKEN_PIPE` (bit 2048, `std.signal.broken_pipe()`), and the write then
answers `io`. `INTERRUPT`, `SUSPEND`, `CHILD` and every other catchable POSIX
signal have meanings too (`os_signal_listen`, `os_signal_wait`,
`os_signal_ignore_one`, `os_signal_text`).

```wolf lupin=unsupported
fn main() -> int ! {io, unsupported} {
    os_signal_ignore(2048)?
    let (r, w) = os_pipe()?
    fs_close(r)?
    fs_write_chunk(w, "x".bytes()) else |e| match e {
        invalid => { print("invalid") },
        io => { print("io: nobody reads the pipe") },
    }
    0
}
```

```output
io: nobody reads the pipe
```

`read_line` is the text path. A catch-all `else` would end the loop on `eof`
but also swallow `io` and `utf8`, so discriminate. lupin declines
`read_line`:

```wolf stdin=b\r\na\n\nc lupin=unsupported
fn main() -> int ! {io, utf8} {
    var n = 0
    loop {
        let line = read_line() else |e| match e {
            eof => { break },
            io => { return io },
            utf8 => { return utf8 },
        }
        n += 1
        print("{n}: [{line}]")
    }
    0
}
```

```output
1: [b]
2: [a]
3: []
4: [c]
```

`read_line` cannot tell you whether the input ended with a newline, drops
the CR of a CRLF, and refuses non-UTF-8 input. A byte-exact tool (`cat`,
`wc -c`, `tr`, `od`) reads handle 0 in chunks instead.

### Writing output

`print` and `print_raw` are one `write(2)` each, so a tool that emits many
small pieces must buffer. One million short lines at wolf 0.2.14, default
tier, median of the runs (not re-measured since; the lesson has not moved,
wolf-lang#408):

| strategy | macOS `/dev/null` | macOS file | Linux `/dev/null` | Linux file |
|---|---|---|---|---|
| `print` per line | 0.61 s | 1.70 s | 0.14 s | 0.64 s |
| `print_raw` per line | 0.43 s | 1.74 s | 0.14 s | 0.65 s |
| `str` batch built with `+=`, flushed per 1,000 lines | 1.28 s | 1.83 s | 1.59 s | 1.59 s |
| `List[byte]` buffer, `fs_write_chunk` per 64 KiB | 0.08 s | 0.11 s | 0.08 s | 0.08 s |

All four produced identical bytes. Building a `str` by appending is
quadratic (see [Writing fast wolf](#writing-fast-wolf)). The buffered writer,
as a reusable piece:

```wolf
struct Out {
    fd: int,
    buf: List[byte],
}

fn flush(mut o: Out) -> () ! {io} {
    fs_write_chunk(o.fd, o.buf) else |e| match e {
        invalid => { return io },
        io => { return io },
    }
    (mut o.buf).reset()
}

fn emit(mut o: Out, s: str) -> () ! {io} {
    for b in s.bytes() {
        (mut o.buf).push(b)
    }
    if o.buf.len >= 65536 then flush(mut o)?
}

fn main() -> int ! {io} {
    var o = Out { fd: 1, buf: List[byte]() }
    for i in 0..3 {
        emit(mut o, "line {i}\n")?
    }
    flush(mut o)?
    0
}
```

```output
line 0
line 1
line 2
```

### What is still missing

| Gap | Status |
|---|---|
| streaming reads without a region per turn: the ambient region keeps every chunk | wolf-lang#416, open; use a region per turn or `std.fs.read_into` |
| a function with a `mut` or `take` parameter as a value | wolf-lang#917, open |
| integers past 64 bits | wolf-std#100, open (`std.bigint` or `u128`) |
| `print` reporting a failed write | wolf-lang#408, open; write through `fs_write_chunk(1, …)` to see `io` |
| `if let x = e; cond { }` and `while var x = e; cond { }` | specified for the next release, not in 0.2.40 |

Permissions, process ids, `chdir`, `isatty`, symbolic and hard links,
owners, file times, the directory-relative `*_at` calls, `statfs`,
priorities, pipes, terminals and every POSIX signal are all in the prelude
now (see [Names](#names)); the spec's OS chapter, `spec/11-os.md`, has the
signature table.

A native program whose runtime cannot set up what it reaches (its signal
hub or readiness set) refuses to start with one line on stderr and exit 71.

## Writing fast wolf

Which tier to ship: `wolf build` is the fast default backend; `wolf build
--release` goes through LLVM with the mid-end on. Measured at 0.2.14 on macOS
arm64 over a 256 MiB `List[byte]` (median of three, the buffer's
construction subtracted, not re-measured since):

| kernel | default | `--release` |
|---|---|---|
| lines and words (a `wc` loop) | ~0.37 s | ~0.11 s |
| checked `int` sum | ~0.18 s | ~0.03 s |
| `wrapping[u64]` sum | ~0.12 s | ~0.03 s |

Ship `--release`. At `--release` the checked `int` sum and the
`wrapping[u64]` sum were both ~0.03 s, inside the timer's resolution, so do
not reach for `wrapping` for speed; use it when wraparound is the meaning
(hashes, checksums, PRNGs), and remember its exit cast traps.

Regions for per-turn scratch: 20,000 turns each building a ~40-piece `str`
by interpolation measured 240 MB of peak memory with no region and 2.7 MB
with the body inside `region pass { }` (0.2.14, macOS arm64). A long-running
or large-input tool that builds strings per record needs the block.

Do not build strings by appending. An interpolation copies its result, and
`s = "{s}{piece}"` or `s += piece` copies the whole accumulator every time, so
a loop of them is quadratic (wolf-lang#298 counts ~110 arena allocations and
6.4 KB per request in a web server from this). The witness, in relations so
it holds on every tier:

```wolf
fn main() {
    var concat_ratio = 0
    var bytes_ratio = 0
    region a {
        var s = ""
        for i in 0..1000 {
            s += "piece {i};"
        }
        concat_ratio = region_bytes(a) / s.len
    }
    region b {
        var buf = List[byte]()
        for i in 0..1000 {
            for x in "piece {i};".bytes() {
                (mut buf).push(x)
            }
        }
        bytes_ratio = region_bytes(b) / buf.len
    }
    print("{concat_ratio > 10 * bytes_ratio}")
}
```

```output
true
```

So push bytes into one `List[byte]` and write it once, write pieces straight
to the output buffer instead of joining them, and memoise a string you
rebuild per request. `std.strbuf` is backed by a `List[byte]` in the bundled
std, so appending to it is amortised O(n):

```wolf
use std.strbuf

fn main() {
    var b = strbuf.new()
    for i in 0..3 {
        strbuf.push_str(mut b, "piece {i};")
    }
    print(strbuf.to_str(b))
}
```

```output
piece 0;piece 1;piece 2;
```

`lower()` and `upper()` always copy, even when nothing changes.

The byte tier for binary data: `List[byte]` strides by one byte and
`List[int]` by eight, and every byte-producing builtin (`bytes()`,
`fs_read_*`, `net_read_bytes`, `str_from_utf8`'s argument) speaks
`List[byte]`. `s.bytes()` is a zero-copy view wherever it is consumed on the
spot (iterated, indexed, `len`, or passed to a function that only reads it);
a `let bs = s.bytes()` materialises a copy, and doing that per turn of a
byte loop has run a machine out of memory. Walk bytes, not `chars()` (which
materialises a `List[char]`), unless you need scalars.

A `Map` finds its keys through a hashed index and keeps insertion order
(0.2.38: 100,000 string-key inserts and lookups went from 16.0 s to 0.020 s
on Linux). Replacing a key's value keeps its place:

```wolf
fn main() {
    var m = Map[str, int]()
    m["pear"] = 3
    m["apple"] = 1
    m["fig"] = 2
    m["apple"] = 10
    for (k, v) in m.pairs() {
        print("{k}={v}")
    }
}
```

```output
pear=3
apple=10
fig=2
```

The algorithmic lessons are the usual ones, and they have each cost a wolf
program minutes: parse an index file once per command, not once per read;
compare ids or stat data before reading and hashing bodies; and look for a
check that compares every record against every other, the usual hidden
quadratic.

Profiling: two instruments measure different things, so say which one a
number came from.

- Share of time: `sample <pid> 10 1` on macOS, `perf record -F 997 -g -p
  <pid>` on Linux, on one process under load. Run-to-run noise was about
  0.3 points of share in an 8 s window for a server, so a smaller delta is
  not a finding.
- Counts: `strace -c -f` for syscalls per unit of work; callgrind for
  instructions; for memory, `region_bytes` around the code in question or
  growth in resident size over a run divided by the work. A leaf table
  under-prices allocation: one server's string runtime looked like ~13% of a
  request from leaves and measured 30-55% by removing it.
- Time a whole binary with `/usr/bin/time`, state the load, repeat three
  times, and take the first run as warm-up. A peak resident size measured
  through `exec` includes the parent's, so spawn the measured binary from a
  small parent and record an empty run's floor beside each row.

## Tools

| Verb | What it does |
|---|---|
| `wolf run file.lu [args]` | build with the default backend and run; the exit code is the program's |
| `wolf build file.lu -o out [--release]` | a native executable; on Linux the build links with `ld.lld` when one is on `PATH` and names it in a note |
| `wolf fmt [--check]` | the one canonical layout; sorts `use` lines |
| `wolf fix [--apply]` | apply the compiler's machine-applicable suggestions |
| `wolf test [--json] [--nocapture] [--native\|--release] [--schedules=N] [--replay=…]` | run `*_test.lu` files: each top-level `fn test_*` is a test, in a fresh checked machine by default; a test file with a `main` and no `test_*` runs as one test |
| `wolf doc [--json]` | the package's documentation |
| `wolf prelude [--json]` | every name a program uses without an import |
| `wolf interface` | every module's public interface |
| `wolf add`, `wolf rm`, `wolf update` | dependencies in `wolf.pkg` |
| `wolf lsp` | the Language Server Protocol over stdio |
| `wolf --explain E0401` | the full explanation of any diagnostic code |
| `wolf conform-run file.lu --json --checked` | the checked machine: every allocation modelled, every undefined-behaviour row detected, an honest `unsupported` past its budgets |
| `lupin run file.lu`, `lupin run --json` | the reference interpreter; `-e 'expr'`, `repl` |
| `--std-root <dir>` (`WOLF_STD`, `LUPIN_STD`) | resolve `use std.x` against a std checkout instead of the archive's own |

Exit and trap conventions, measured at 0.2.40. `wolf build` exits 0 when it
built, 2 when it rejected the program, and 4 when it refused a construct it
cannot compile yet ("cannot compile this yet …, the conservatism ledger, not
a bug in your program"): that is a gap, not your error, and worth a
four-line reproducer. Neither 2 nor 4 is a skip. A native trap prints
`wolf-trap: <kind>` on stderr and exits 134. lupin exits 0 for the program's
own exit, 2 for a static rejection, 3 for a trap and 4 for `unsupported`.
Always check a pipeline's status explicitly (`$pipestatus`,
`${PIPESTATUS[@]}`); a pipe into `tail` returns `tail`'s.

A curve25519 signature that once crashed the `--release` mid-end builds and
verifies at 0.2.40 with the mid-end on.

## Habits that keep paying

- Bisect a refusal before designing around it: one four-line program per
  shape, each in its own directory, varying the data as well as the syntax
  (a checked-machine `unsupported` can depend on which path executed).
- Write the prediction down before the run, then report hits and misses.
- Re-read the release notes before re-probing a finding: it may have healed
  in the release you already pinned.
- Edit in small pieces, confined to the function at fault, and end with a
  build and a test run that you repair if they are red. A whole-file rewrite
  that ends with the build broken is the most common way an agent's session
  is lost.
- When a probe narrows a fault to one function, edit it and rerun the one
  failing case before reading further.
- The only acceptable endings are every row green, or a pasted failing case
  with the exact rule you dispute. A summary of what passed is not an
  ending.
- Every example in a README is pasted from the binary, and licence text is
  copied from the official file, byte for byte.

## Not in 0.2.40

- The binding header `if let x = e; cond { }` and `while var x = e; cond
  { }` (one binder, a literal `;`, braces only) is specified
  (`[gram.expr.if.head]`) and planned for the next release. The 0.2.40
  compiler does not accept it.
- `wolf fmt` keeping a one-statement `if c then …` on one line merged after
  0.2.40 (wolf-lang#1125). At 0.2.40, run `wolf fmt` and keep what it gives.
