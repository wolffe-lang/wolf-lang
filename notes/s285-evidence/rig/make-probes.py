#!/usr/bin/env python3
"""s285: write the probe programs for wolf-lang#1058 (a literal pattern
outside its scrutinee's type) under PROBE_DIR/<name>/main.lu.

Each probe is one small program whose exit code says which arm ran:
0 = the out-of-range arm matched, 2 = the fallback arm ran.
usage: make-probes.py PROBE_DIR
"""
import sys
from pathlib import Path

out = Path(sys.argv[1])

PRIMS = {
    # name: (min, max)
    "i8": (-(1 << 7), (1 << 7) - 1),
    "i16": (-(1 << 15), (1 << 15) - 1),
    "i32": (-(1 << 31), (1 << 31) - 1),
    "i64": (-(1 << 63), (1 << 63) - 1),
    "int": (-(1 << 63), (1 << 63) - 1),
    "u8": (0, (1 << 8) - 1),
    "u16": (0, (1 << 16) - 1),
    "u32": (0, (1 << 32) - 1),
    "u64": (0, (1 << 64) - 1),
    "uint": (0, (1 << 64) - 1),
}

probes = {}


def prog(ty, pat, arg):
    return (
        f"fn f(v: {ty}) -> int {{\n"
        f"    match v {{ {pat} => 0, _ => 2 }}\n"
        f"}}\n"
        f"fn main() -> int {{ f({arg}) }}\n"
    )


# 1. The issue's row, verbatim.
probes["p00_issue_u8_300"] = prog("u8", "300", "44")
# 2. Every primitive: max + 1, called with the value it wraps to (the
#    type's min: 0 unsigned, MIN signed), and the in-range control max.
for i, (ty, (lo, hi)) in enumerate(PRIMS.items()):
    probes[f"p1{i}_{ty}_max_plus_1"] = prog(ty, str(hi + 1), str(lo))
    probes[f"p2{i}_{ty}_max_control"] = prog(ty, str(hi), str(hi))
# 3. min - 1 and a negative literal: the grammar's literal has no sign.
probes["p30_u8_negative_one"] = prog("u8", "-1", "255")
probes["p31_i8_min_minus_1"] = prog("i8", "-129", "127")
# 4. wrapping[T] wraps its arithmetic, never its literals.
probes["p40_wrapping_u8_300"] = prog("wrapping[u8]", "300", "44 as wrapping[u8]")
probes["p41_wrapping_i8_128"] = prog("wrapping[i8]", "128", "-128 as wrapping[i8]")
# 5. Range endpoints are literal patterns too.
probes["p50_u8_range_hi_out_incl"] = prog("u8", "250..=300", "255")
probes["p51_u8_range_hi_excl_256"] = prog("u8", "0..256", "255")
probes["p52_u8_range_both_out"] = prog("u8", "256..=300", "44")
probes["p53_u8_range_lo_out"] = prog("u8", "300..=400", "44")
probes["p54_i8_range_hi_out"] = prog("i8", "100..=200", "127")
probes["p55_u8_range_control"] = prog("u8", "250..=255", "255")
# 6. Nested positions.
probes["p60_tuple_elem"] = (
    "fn f(v: u8) -> int {\n"
    "    match (v, 1) { (300, _) => 0, _ => 2 }\n"
    "}\n"
    "fn main() -> int { f(44) }\n"
)
probes["p61_enum_payload"] = (
    "enum E { A(u8), B }\n"
    "fn f(e: E) -> int {\n"
    "    match e { A(300) => 0, _ => 2 }\n"
    "}\n"
    "fn main() -> int { f(E.A(44)) }\n"
)
probes["p62_struct_field"] = (
    "struct P { a: u8, b: int }\n"
    "fn f(p: P) -> int {\n"
    "    match p { P { a: 300, .. } => 0, _ => 2 }\n"
    "}\n"
    "fn main() -> int { f(P { a: 44, b: 1 }) }\n"
)
probes["p63_or_alt"] = prog("u8", "1 | 300", "44")
probes["p64_at_binding"] = (
    "fn f(v: u8) -> int {\n"
    "    match v { n @ 300 => n as int - 44, _ => 2 }\n"
    "}\n"
    "fn main() -> int { f(44) }\n"
)
probes["p65_cast_scrutinee"] = (
    "fn main() -> int {\n"
    "    let x: int = 300\n"
    "    match x as u8 { 300 => 0, _ => 2 }\n"
    "}\n"
)
probes["p66_statement_match"] = (
    "fn main() -> int {\n"
    "    let v: u8 = 44\n"
    "    var r = 2\n"
    "    match v {\n"
    "        300 => { r = 0 }\n"
    "        _ => {}\n"
    "    }\n"
    "    r\n"
    "}\n"
)
probes["p67_hex_spelling"] = prog("u8", "0x12c", "44")
probes["p68_underscore_spelling"] = prog("u16", "65_536", "0")
# 7. No context but the match: the scrutinee is a literal too.
probes["p70_default_i32_scrutinee"] = (
    "fn main() -> int {\n"
    "    match 5 { 3000000000 => 0, _ => 2 }\n"
    "}\n"
)
probes["p71_default_binding"] = (
    "fn main() -> int {\n"
    "    let v = 5\n"
    "    match v { 3000000000 => 0, _ => 2 }\n"
    "}\n"
)
# 8. Exhaustiveness: an out-of-range arm then the in-range ones.
probes["p80_u8_unreachable_dup"] = (
    "fn f(v: u8) -> int {\n"
    "    match v { 44 => 1, 300 => 0, _ => 2 }\n"
    "}\n"
    "fn main() -> int { f(44) }\n"
)
# 9. The expression-side control: the same literal as a value is E0415.
probes["p90_expr_control_e0415"] = (
    "fn main() -> int {\n"
    "    let v: u8 = 300\n"
    "    v as int\n"
    "}\n"
)
probes["p91_compare_control"] = (
    "fn f(v: u8) -> int {\n"
    "    if v == 300 { 0 } else { 2 }\n"
    "}\n"
    "fn main() -> int { f(44) }\n"
)

for name, text in probes.items():
    d = out / name
    d.mkdir(parents=True, exist_ok=True)
    (d / "main.lu").write_text(text)
print(f"wrote {len(probes)} probes under {out}")
