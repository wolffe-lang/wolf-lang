#!/usr/bin/env python3
"""s274 item 1: the #631 witnesses, cut from the test file at the checked-out commit.

usage: probe.py <region_copyout_lanes.rs> <out dir>

For each variant it writes two programs:
  <out>/row/<v>/main.lu    WITNESS with the variant's STEP, byte-for-byte what the
                           row builds (`WITNESS.replace("STEP", step)`);
  <out>/probe/<v>/main.lu  the same program with `main` marking its own memory at
                           turns 10,000 and 20,000 and at the end: VmHWM and VmRSS
                           from /proc/self/status, Rss, Anonymous and Swap from
                           /proc/self/smaps_rollup (a page-table walk), each read
                           inside a `region` so the reading keeps nothing.
Variants: flat (STEP_COPY_INT, the row), leak (the same with its `copy region`
undone: #612's shape), str (STEP_COPY_STR), ctl (STEP_NO_REGION).
"""
import os
import re
import sys

src = open(sys.argv[1]).read()
out = sys.argv[2]


def const(name):
    m = re.search(r'const ' + name + r': &str = r#"(.*?)"#;', src, re.S)
    if not m:
        sys.exit(f"probe.py: no const {name}")
    return m.group(1)


witness = const("WITNESS")
flat = const("STEP_COPY_INT")
leak = flat.replace("let k = copy region scratch {\n        build(i).len\n    }", "let k = build(i).len")
if leak == flat:
    sys.exit("probe.py: the leak variant did not undo the copy region")
steps = {
    "flat": flat,
    "leak": leak,
    "str": const("STEP_COPY_STR"),
    "ctl": const("STEP_NO_REGION"),
}

ROW_MAIN = """fn main() -> int {
    let args = env_args()
    let n = if args.len > 0 { args[0].to_int() else 20000 } else { 20000 }
    var st = St { vals: Map[str, str](), n: 0 }
    var i = 0
    while i < n {
        step(mut st, i)
        i += 1
    }
    print("{st.vals["x"] else "?"} {st.n}")
    print("hwm={hwm().trim()}")
    0
}
"""
if ROW_MAIN not in witness:
    sys.exit("probe.py: WITNESS's main is not the one this probe replaces")

PROBE_MAIN = """fn field(text: str, key: str) -> str {
    for line in text.lines() {
        if line.starts_with(key) {
            return ((line.strip_prefix(key) else "").trim().strip_suffix("kB") else "?").trim()
        }
    }
    "?"
}

fn mark(i: int) {
    region probe {
        let st = fs_read_text("/proc/self/status") else ""
        let sm = fs_read_text("/proc/self/smaps_rollup") else ""
        print("mark i={i} hwm={field(st, "VmHWM:")} rss={field(st, "VmRSS:")} srss={field(sm, "Rss:")} anon={field(sm, "Anonymous:")} swap={field(sm, "Swap:")}")
    }
}

fn main() -> int {
    let args = env_args()
    let n = if args.len > 0 { args[0].to_int() else 20000 } else { 20000 }
    var st = St { vals: Map[str, str](), n: 0 }
    var i = 0
    while i < n {
        if i == 10000 || i == 20000 {
            mark(i)
        }
        step(mut st, i)
        i += 1
    }
    mark(n)
    print("{st.vals["x"] else "?"} {st.n}")
    print("hwm={hwm().trim()}")
    0
}
"""

for v, step in steps.items():
    for kind, text in (("row", witness.replace("STEP", step)),
                       ("probe", witness.replace(ROW_MAIN, PROBE_MAIN).replace("STEP", step))):
        d = os.path.join(out, kind, v)
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "main.lu"), "w") as f:
            f.write(text)
print("probe.py: wrote", ", ".join(f"{k}/{v}" for k in ("row", "probe") for v in steps))
