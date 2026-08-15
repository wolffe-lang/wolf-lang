//! s37 — the builtin `str` surface under checked execution (D24/D25):
//! byte-offset slicing with its two defined faults (OOB, split code
//! point), `^n` end-relative offsets, the recoverable boundary
//! primitive `get` (wolf-lang#17), the Python method set, the
//! materialized views, `[mem.str.order]` byte-lexicographic ordering,
//! and the `[[fill]align][width]` format-spec subset with honest
//! refusals beyond it (wolf-lang#10 — a spec is never silently
//! ignored).
//!
//! Every program is statically clean through `mem` before it
//! executes — the harness asserts the ladder, so a trap or a miss
//! here is a genuine dynamic verdict.

use wolf_mem::ubcheck::{self, Budget, Verdict};
use wolf_sema::{AliasTable, MemoryLoader, resolve_package_with, typecheck_package_with};

/// Run a single-file program: assert the static ladder is clean, then
/// execute under the checked machine. Returns the full outcome
/// (verdict + stdout).
fn run(src: &str) -> ubcheck::RunOutcome {
    let mut ml = MemoryLoader::new("strb");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    assert!(
        res.diagnostics
            .iter()
            .all(|d| d.severity != wolf_diag::Severity::Error),
        "input resolves clean: {:?}",
        res.diagnostics
    );
    let tc = typecheck_package_with(&res.package, true);
    assert!(
        tc.not_yet.is_empty(),
        "input typechecks fully: {:?}",
        tc.not_yet
    );
    assert!(
        !tc.has_errors(),
        "input typechecks clean: {:?}",
        tc.diagnostics
    );
    let mem = wolf_mem::check_package(&res.package, &tc);
    assert!(
        mem.not_yet.is_empty(),
        "input stays inside the mem surface: {:?}",
        mem.not_yet
    );
    assert!(
        mem.diagnostics
            .iter()
            .all(|d| d.severity != wolf_diag::Severity::Error),
        "input is statically accepted: {:?}",
        mem.diagnostics
    );
    ubcheck::run_checked(&res.package, &tc, Budget::default())
        .expect("the program is within the executable surface")
}

fn assert_exit(src: &str, code: u8) {
    match run(src).verdict {
        Verdict::Exit(n) => assert_eq!(n, code, "exit code"),
        other => panic!("expected exit({code}), got {other:?}"),
    }
}

fn assert_trap(src: &str, kind: &str) {
    match run(src).verdict {
        Verdict::Trap(t) => assert_eq!(t.kind, kind, "trap kind"),
        other => panic!("expected trap({kind}), got {other:?}"),
    }
}

fn assert_stdout(src: &str, expected: &str) {
    let out = run(src);
    match out.verdict {
        Verdict::Exit(0) => {}
        other => panic!("expected exit(0), got {other:?}"),
    }
    assert_eq!(out.stdout, expected, "stdout");
}

/// The refusal path: the program must be statically clean but refuse
/// under checked execution with the given construct text.
fn assert_refuses(src: &str, needle: &str) {
    let mut ml = MemoryLoader::new("strb");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    let tc = typecheck_package_with(&res.package, true);
    assert!(
        tc.not_yet.is_empty() && !tc.has_errors(),
        "input typechecks clean: {:?} {:?}",
        tc.not_yet,
        tc.diagnostics
    );
    match ubcheck::run_checked(&res.package, &tc, Budget::default()) {
        Err(nyc) => assert!(
            nyc.construct.contains(needle),
            "refusal names the construct: got `{}`",
            nyc.construct
        ),
        Ok(out) => panic!("expected a refusal, got {:?}", out.verdict),
    }
}

// ----------------------------------------------------- len + slicing --

#[test]
fn len_is_bytes() {
    // `"é".len == 2` — the honest documentation case (D24).
    assert_exit(
        "fn main() -> !int {\n\
         let a = \"wolf\"\n\
         let b = \"é\"\n\
         if a.len == 4 && b.len == 2 && \"\".len == 0 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

#[test]
fn slicing_shares_bytes() {
    assert_exit(
        "fn main() -> !int {\n\
         let s = \"the wolf runs\"\n\
         let head = s[..8]\n\
         let tail = s[9..]\n\
         let mid = s[4..8]\n\
         if head == \"the wolf\" && tail == \"runs\" && mid == \"wolf\" { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

#[test]
fn slicing_from_end() {
    // `^n` counts bytes from the end (D25): `s[..^1]` drops the last
    // byte, `s[^4..]` keeps the last four.
    assert_exit(
        "fn main() -> !int {\n\
         let s = \"wolves\"\n\
         if s[..^1] == \"wolve\" && s[^2..] == \"es\" && s[^4..^1] == \"lve\" { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

#[test]
fn slice_oob_traps_bounds() {
    // D25's first defined fault: an out-of-range byte offset.
    assert_trap(
        "fn main() -> !int {\n\
         let s = \"wolf\"\n\
         let x = s[0..9]\n\
         if x == \"\" { 1 } else { 2 }\n\
         }\n",
        "bounds",
    );
}

#[test]
fn slice_split_code_point_traps_bounds() {
    // D25's second defined fault: an offset inside a multi-byte code
    // point — deterministic, never a garbled slice.
    assert_trap(
        "fn main() -> !int {\n\
         let s = \"é\"\n\
         let x = s[0..1]\n\
         if x == \"\" { 1 } else { 2 }\n\
         }\n",
        "bounds",
    );
}

#[test]
fn inclusive_slice() {
    assert_exit(
        "fn main() -> !int {\n\
         let s = \"wolf\"\n\
         if s[0..=2] == \"wol\" { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

// ------------------------------------- the boundary primitive: `get` --

#[test]
fn get_is_the_recoverable_slice() {
    // wolf-lang#17: the same questions the checked slice would trap
    // on come back as `{none}` rows — OOB and split-boundary alike.
    assert_exit(
        "fn main() -> !int {\n\
         let s = \"héllo\"\n\
         let hit = s.get(0..1) else \"?\"\n\
         let oob = s.get(0..9) else \"?\"\n\
         let split = s.get(0..2) else \"?\"\n\
         let all = s.get(0..6) else \"?\"\n\
         if hit == \"h\" && oob == \"?\" && split == \"?\" && all == \"héllo\" { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

// ------------------------------------------------------ method set --

#[test]
fn affix_probes() {
    assert_exit(
        "fn main() -> !int {\n\
         let s = \"the wolf runs\"\n\
         let a = s.starts_with(\"the \")\n\
         let b = s.ends_with(\"runs\")\n\
         let c = s.contains(\"wolf\")\n\
         let d = s.starts_with(\"wolf\")\n\
         if a && b && c && !d { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

#[test]
fn find_returns_byte_offsets() {
    // The asymmetry that named the bug (#17): `find` over arbitrary
    // UTF-8 — `l` after a two-byte `é` sits at byte offset 3.
    assert_exit(
        "fn main() -> !int {\n\
         let s = \"héllo\"\n\
         let a = s.find(\"l\") else 0 - 1\n\
         let b = s.rfind(\"l\") else 0 - 1\n\
         let miss = s.find(\"wolf\") else 0 - 1\n\
         if a == 3 && b == 4 && miss == 0 - 1 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

#[test]
fn strip_and_trim() {
    assert_exit(
        "fn main() -> !int {\n\
         let s = \"  wolf  \"\n\
         let t = s.trim()\n\
         let ts = s.trim_start()\n\
         let te = s.trim_end()\n\
         let p = t.strip_prefix(\"wo\") else \"?\"\n\
         let q = t.strip_suffix(\"xx\") else \"?\"\n\
         if t == \"wolf\" && ts == \"wolf  \" && te == \"  wolf\" && p == \"lf\" && q == \"?\" {\n\
         0\n\
         } else {\n\
         1\n\
         }\n\
         }\n",
        0,
    );
}

#[test]
fn case_count_repeat_replace() {
    assert_exit(
        "fn main() -> !int {\n\
         let s = \"Wolf\"\n\
         let a = s.lower() == \"wolf\"\n\
         let b = s.upper() == \"WOLF\"\n\
         let c = \"aabaa\".count(\"aa\") == 2\n\
         let d = \"ab\".repeat(3) == \"ababab\"\n\
         let e = \"the wolf\".replace(\"wolf\", \"moon\") == \"the moon\"\n\
         let f = \"\".is_empty() && !s.is_empty()\n\
         if a && b && c && d && e && f { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

/// A negative count is a caller contract violation: `assert`, ruled
/// by [mem.str.repeat] (s71, #57 — previously `bounds`, which no
/// clause ever defined).
#[test]
fn repeat_negative_traps() {
    assert_trap(
        "fn main() -> !int {\n\
         let n = 0 - 1\n\
         let x = \"ab\".repeat(n)\n\
         if x == \"\" { 1 } else { 2 }\n\
         }\n",
        "assert",
    );
}

/// [mem.str.empty] (s71, #56): the searching family is defined on an
/// empty needle — count 0, split one whole piece, replace identity.
/// The checked lane's refusals die here; native always answered this.
#[test]
fn empty_needle_is_defined() {
    assert_exit(
        "fn main() -> !int {\n\
         let a = \"abc\".count(\"\") == 0\n\
         let p = \"abc\".split(\"\")\n\
         let b = p.len == 1 && p[0] == \"abc\"\n\
         let c = \"abc\".replace(\"\", \"-\") == \"abc\"\n\
         if a && b && c { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

// ----------------------------------------------------------- views --

#[test]
fn words_lines_split_bytes() {
    assert_exit(
        "fn main() -> !int {\n\
         let s = \"the wolf runs\"\n\
         let w = s.words()\n\
         let l = \"a\\nb\\nc\".lines()\n\
         let p = \"a,b,c\".split(\",\")\n\
         let b = \"é\".bytes()\n\
         let words_ok = w.len == 3 && w[1] == \"wolf\"\n\
         let lines_ok = l.len == 3 && l[2] == \"c\"\n\
         let split_ok = p.len == 3 && p[0] == \"a\"\n\
         let bytes_ok = b.len == 2 && b[0] == 195 && b[1] == 169\n\
         if words_ok && lines_ok && split_ok && bytes_ok { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

/// s89 (wolf-lang#85): the checked tier modelled two of s77's seven
/// byte-view consuming positions — iteration and `.len` — and stopped
/// at `mem` for the other five, which are exactly the ones an indexed
/// byte algorithm wants. The gap was never the view: it was the
/// TEMPORARY, since indexing rooted itself in a frame local and a
/// temporary has none. All seven run here now, and the element read is
/// the same walk (and the same bounds trap) a place receiver takes.
#[test]
fn the_byte_view_query_family_runs_on_a_temporary() {
    assert_exit(
        "fn main() -> !int {\n\
         let s = \"wolf é\"\n\
         let idx = s.bytes()[5]\n\
         let get = s.bytes().get(6) else 0 - 1\n\
         let first = s.bytes().first() else 0 - 1\n\
         let last = s.bytes().last() else 0 - 1\n\
         let count = s.bytes().count()\n\
         let empty = s.bytes().is_empty()\n\
         let len = s.bytes().len\n\
         var walked = 0\n\
         for b in s.bytes() { walked = walked + b }\n\
         let ok = idx == 195 && get == 169 && first == 119 && last == 169\n\
         let ok2 = count == 7 && !empty && len == 7 && walked == 836\n\
         if ok && ok2 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

/// The temporary the fix reaches is any List rvalue, not only a byte
/// view — a call result indexes and queries exactly as a binding does.
#[test]
fn a_list_returned_from_a_call_indexes_as_a_temporary() {
    assert_exit(
        "fn mk() -> List[int] {\n\
         var xs = List[int]()\n\
         (mut xs).push(7)\n\
         (mut xs).push(9)\n\
         xs\n\
         }\n\
         fn main() -> !int {\n\
         if mk()[1] == 9 && mk().count() == 2 && (mk().first() else 0) == 7 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

/// An out-of-range index on a temporary traps where a place receiver
/// traps — the element read shares one walk, so it shares one fault.
#[test]
fn indexing_a_byte_view_out_of_range_traps() {
    assert_trap(
        "fn main() -> !int {\n\
         let s = \"wolf\"\n\
         let b = s.bytes()[9]\n\
         b\n\
         }\n",
        "bounds",
    );
}

// -------------------------------------------------- [mem.str.order] --

#[test]
fn str_ordering_is_byte_lexicographic() {
    // The [mem.str.order] pins: shared prefix, shorter first, every
    // multi-byte code point above every ASCII one.
    assert_exit(
        "fn main() -> !int {\n\
         let a = \"wolf\" < \"wolves\"\n\
         let b = \"wolf\" < \"wolf!\"\n\
         let c = \"z\" < \"é\"\n\
         let d = \"é\" < \"🐺\"\n\
         let e = \"\" < \"a\"\n\
         if a && b && c && d && e { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

// ------------------------- s81 / #58: the validating byte source --

/// The helper every test below shares: a `List[int]` built from up to
/// four elements, decoded through the border post, with the refusal
/// spelled as an ordinary `else`.
const DECODER: &str = "fn decode(bs: List[int]) -> str {\n\
     str_from_utf8(bs) else \"X\"\n\
     }\n\
     fn seq(a: int, b: int, c: int, dd: int, n: int) -> List[int] {\n\
     var l = List[int]()\n\
     if n > 0 { (mut l).push(a) }\n\
     if n > 1 { (mut l).push(b) }\n\
     if n > 2 { (mut l).push(c) }\n\
     if n > 3 { (mut l).push(dd) }\n\
     l\n\
     }\n";

#[test]
fn from_utf8_accepts_text_including_an_interior_nul() {
    // `str_from_utf8` is the one operation that builds a `str` out of
    // numbers (s77 left the byte tier one-way on purpose). Accepted:
    // ASCII, two-byte, three-byte, four-byte, empty — and a NUL, which
    // is VALID text: a wolf `str` carries its length, so nothing
    // terminates.
    assert_stdout(
        &format!(
            "{DECODER}\
             fn main() -> !int {{\n\
             let ascii = decode(seq(119, 111, 108, 102, 4))\n\
             let two = decode(seq(195, 169, 0, 0, 2))\n\
             let three = decode(seq(226, 130, 172, 0, 3))\n\
             let four = decode(seq(240, 159, 144, 186, 4))\n\
             let empty = decode(List[int]())\n\
             let nul = decode(seq(119, 0, 102, 0, 3))\n\
             print(\"{{ascii}} {{two}} {{three}} {{four}} {{empty.len}} {{nul.len}}\")\n\
             0\n\
             }}\n"
        ),
        "wolf é € 🐺 0 3\n",
    );
}

#[test]
fn from_utf8_refuses_the_ugly_inputs_as_a_row_not_a_trap() {
    // One named failure mode per column, and every one of them is a
    // VALUE the `else` catches — refusing bytes is an outcome, and a
    // trap here would make `bytes.to_str` unwritable in wolf-std.
    // The last two are not bytes at all: `List[int]` holds `int`s.
    assert_stdout(
        &format!(
            "{DECODER}\
             fn main() -> !int {{\n\
             let lone = decode(seq(128, 0, 0, 0, 1))\n\
             let cont = decode(seq(191, 191, 0, 0, 2))\n\
             let trunc = decode(seq(226, 130, 0, 0, 2))\n\
             let trunc4 = decode(seq(240, 159, 144, 0, 3))\n\
             let overlong = decode(seq(192, 175, 0, 0, 2))\n\
             let overlong3 = decode(seq(224, 128, 175, 0, 3))\n\
             let surrogate = decode(seq(237, 160, 128, 0, 3))\n\
             let too_big = decode(seq(245, 128, 128, 128, 4))\n\
             let never = decode(seq(254, 0, 0, 0, 1))\n\
             let big = decode(seq(256, 0, 0, 0, 1))\n\
             let neg = decode(seq(0 - 1, 0, 0, 0, 1))\n\
             let poison = decode(seq(119, 111, 108, 300, 4))\n\
             print(\"{{lone}}{{cont}}{{trunc}}{{trunc4}}{{overlong}}{{overlong3}}\")\n\
             print(\"{{surrogate}}{{too_big}}{{never}}{{big}}{{neg}}{{poison}}\")\n\
             0\n\
             }}\n"
        ),
        "XXXXXX\nXXXXXX\n",
    );
}

#[test]
fn from_utf8_round_trips_the_byte_view() {
    // The witness that the s77 view and the s81 source agree: bytes out,
    // bytes back, same string — and the `?` propagation wolf-std's
    // `bytes.to_str` needs, over a caller that declares its own `utf8`.
    assert_exit(
        "fn to_str(b: List[int]) -> str ! {utf8} {\n\
         str_from_utf8(b)\n\
         }\n\
         fn shout(b: List[int]) -> str ! {utf8} {\n\
         let s = to_str(b)?\n\
         s.upper()\n\
         }\n\
         fn main() -> !int {\n\
         let src = \"wolf é\"\n\
         let back = to_str(src.bytes()) else \"X\"\n\
         let loud = shout(src.bytes()) else \"X\"\n\
         if back == src && back.len == 7 && loud == \"WOLF É\" { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

// ------------------------------------------- interpolation + specs --

#[test]
fn interp_preserves_utf8() {
    // The c06 latin-1 divergence, retired: a non-ASCII literal
    // survives interpolation byte-for-byte.
    assert_stdout(
        "fn main() -> !int {\n\
         let w = \"caf\u{e9} \u{1f43a}\"\n\
         print(\"<{w}>\")\n\
         0\n\
         }\n",
        "<café 🐺>\n",
    );
}

#[test]
fn double_braces_are_literal() {
    // `{{` / `}}` are literal braces ([gram.lex.str]) — printing them
    // doubled was a stdout divergence against the reference.
    assert_stdout(
        "fn main() -> !int {\n\
         let n = 3\n\
         print(\"{{n}} is {n}\")\n\
         0\n\
         }\n",
        "{n} is 3\n",
    );
}

#[test]
fn format_spec_fill_align_width() {
    // The implemented subset (`[[fill]align][width]`, #10/#28):
    // numbers default right, strings default left, width in bytes.
    assert_stdout(
        "fn main() -> !int {\n\
         let s = \"hi\"\n\
         let n = 42\n\
         print(\"[{s:8}]\")\n\
         print(\"[{n:8}]\")\n\
         print(\"[{n:>6}]\")\n\
         print(\"[{n:*>8}]\")\n\
         print(\"[{s:^6}]\")\n\
         print(\"[{n:2}]\")\n\
         0\n\
         }\n",
        "[hi      ]\n[      42]\n[    42]\n[******42]\n[  hi  ]\n[42]\n",
    );
}

#[test]
fn format_spec_sign_zero_and_bases() {
    // s38: the full §7.4 candidate evaluates. `+` marks non-negative
    // numbers (zero takes `+` — the wolf-std `with_sign` pin); `08`
    // is the zero FLAG plus width, zero-padding after the sign (the
    // `{n:08}` mis-read as width-8-space-fill was #28 item 3);
    // `b`/`o`/`x`/`X` are sign-magnitude with no prefix.
    assert_stdout(
        "fn main() -> !int {\n\
         let n = 42\n\
         let z = 0\n\
         let neg = 0 - 42\n\
         print(\"[{n:+}][{z:+}][{neg:+}]\")\n\
         print(\"[{n:08}][{neg:06}]\")\n\
         print(\"[{n:x}][{n:X}][{n:b}][{n:o}]\")\n\
         print(\"[{neg:x}][{n:>8x}][{n:+06}]\")\n\
         0\n\
         }\n",
        "[+42][+0][-42]\n[00000042][-00042]\n[2a][2A][101010][52]\n[-2a][      2a][+00042]\n",
    );
}

#[test]
fn format_spec_str_precision_respects_boundaries() {
    // `.N` on a `str` is a byte cap that never splits a code point —
    // the one clause the spec must say "boundary" out loud for (#28);
    // "aé" capped at 2 backs off to "a".
    assert_stdout(
        "fn main() -> !int {\n\
         let s = \"wolf\"\n\
         let u = \"a\u{e9}\"\n\
         print(\"[{s:.2}][{u:.2}][{u:.3}][{s:>6.2}]\")\n\
         0\n\
         }\n",
        "[wo][a][a\u{e9}][    wo]\n",
    );
}

#[test]
fn format_spec_floats_evaluate() {
    // The #10 headline: `{x:>8.2}` evaluates. Bare precision means
    // fixed; `e`/`E` carry a signed two-digit exponent; the default
    // rendering is the shortest round-trip decimal in the wolf-std
    // `decimal.to_str` layout.
    assert_stdout(
        "fn main() -> !int {\n\
         let x = 3.14159\n\
         let half = 0.5\n\
         print(\"[{x:>8.2}]\")\n\
         print(\"[{x:.2f}][{half:.0f}]\")\n\
         print(\"[{x:.2e}][{x:.2E}]\")\n\
         print(\"[{x}][{half}]\")\n\
         0\n\
         }\n",
        "[    3.14]\n[3.14][0]\n[3.14e+00][3.14E+00]\n[3.14159][0.5]\n",
    );
}

#[test]
fn float_arithmetic_is_ieee() {
    // Floats never trap (X3 is integer law): division by zero is
    // `inf`, and the shortest-round-trip rendering shows it.
    assert_stdout(
        "fn main() -> !int {\n\
         let a = 1.5\n\
         let b = 0.25\n\
         let zero = 0.0\n\
         print(\"{a + b} {a * b} {a / zero}\")\n\
         if a > b { print(\"ordered\") }\n\
         0\n\
         }\n",
        "1.75 0.375 inf\nordered\n",
    );
}

#[test]
fn format_spec_computed_still_refuses() {
    // `{x:{w}}` has no pinned semantics — a #28 question, an honest
    // refusal here (never a guess).
    assert_refuses(
        "fn main() -> !int {\n\
         let n = 42\n\
         let w = 8\n\
         print(\"[{n:{w}}]\")\n\
         0\n\
         }\n",
        "computed format spec",
    );
}

#[test]
fn interpolated_str_as_a_value() {
    // The checked-lane twin of corpus/strings/interp_value_position.lu
    // — the most user-visible native refusal (a function RETURNING an
    // interpolated string). The checked lane materializes it; the
    // native allocating path is the owed design.
    assert_stdout(
        "fn greet(name: str) -> str {\n\
         \x20   \"hello, {name}\"\n\
         }\n\
         \n\
         fn main() -> !int {\n\
         \x20   print(greet(\"wolf\"))\n\
         \x20   0\n\
         }\n",
        "hello, wolf\n",
    );
}

// --------------------------------------------- multiline dedent (D26) --

#[test]
fn multiline_dedents_by_closing_column() {
    // The closing delimiter's column sets the dedent; the final
    // newline stays, the closing indentation goes.
    assert_exit(
        "fn main() -> !int {\n\
         let poem = \"\"\"\n\
        \x20   the wolf runs\n\
        \x20   the moon watches\n\
        \x20   \"\"\"\n\
         let head = poem[..8]\n\
         let tail = poem[^13..]\n\
         let ok_len = poem.len == 31\n\
         if ok_len && head == \"the wolf\" && tail == \"moon watches\\n\" { 0 } else { 1 }\n\
         }\n",
        0,
    );
}

#[test]
fn multiline_with_holes_refuses() {
    // Dedent shifts hole offsets — the combination refuses rather
    // than printing undedented text (the silent-wrong class).
    assert_refuses(
        "fn main() -> !int {\n\
         let n = 3\n\
         let s = \"\"\"\n\
        \x20   count: {n}\n\
        \x20   \"\"\"\n\
         if s == \"\" { 1 } else { 0 }\n\
         }\n",
        "multiline",
    );
}

// -------------------------------------- wolf-lang#30: tag collision --

#[test]
fn raise_resolves_declared_row_before_value_namespace() {
    // A row tag sharing a name with a same-scope function: the raise
    // must mean the TAG (the declared row is the nearest scope), so
    // the caller's `else` fires. Pre-fix this returned the function
    // value and the miss path never ran (the silent-wrong class).
    assert_exit(
        "fn helper() -> int {\n\
         3\n\
         }\n\
         \n\
         fn miss(k: int) -> int ! {helper} {\n\
         if k < 0 {\n\
         return helper\n\
         }\n\
         k\n\
         }\n\
         \n\
         fn main() -> !int {\n\
         let hit = miss(5) else 0 - 1\n\
         let b = miss(0 - 1) else 0 - 7\n\
         if hit == 5 && b == 0 - 7 { 0 } else { 1 }\n\
         }\n",
        0,
    );
}
