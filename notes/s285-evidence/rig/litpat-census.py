#!/usr/bin/env python3
"""s285: a syntactic census of integer literal patterns that COULD fall
outside their scrutinee's type (wolf-lang#1058), before any code.

A literal pattern outside its scrutinee's type must be at least 128: the
grammar's `literal` has no sign (`[gram.lex]`'s INT), so no negative
literal pattern parses, and 128 is the smallest max+1 of any integer
primitive (i8). So every integer literal written in an arm's pattern
(the text before `=>`, with or-pattern continuation lines) whose value
is >= 128 is a CANDIDATE; the census lists them all, with the file, the
line, the value, the smallest primitive that holds it, and whether it is
a range endpoint. A superset: the scrutinee's type decides, and is read
by hand for every candidate (or by the checker at the head).

usage: litpat-census.py OUT.txt DIR...
"""
import os
import re
import sys

out_path, *dirs = sys.argv[1:]
SKIP = {"target", ".git", "node_modules"}
INT = re.compile(
    r"(?<![\w.'])(0[xX][0-9a-fA-F_]+|0[oO][0-7_]+|0[bB][01_]+|\d[\d_]*)(?![\w.]|\.\d)"
)
FITS = [
    ("i8", 127), ("u8", 255), ("i16", 32767), ("u16", 65535),
    ("i32", 2**31 - 1), ("u32", 2**32 - 1), ("i64", 2**63 - 1), ("u64", 2**64 - 1),
]


def value(text):
    t = text.replace("_", "")
    for pre, radix in (("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)):
        if t.startswith(pre):
            return int(t[2:], radix)
    return int(t)


def strip(line):
    """Drop a `//` comment and the bodies of plain string and char
    literals (rough: enough that a digit inside text is not a pattern)."""
    out = []
    i = 0
    q = None
    while i < len(line):
        c = line[i]
        if q:
            if c == "\\":
                i += 2
                continue
            if c == q:
                q = None
                out.append(c)
            i += 1
            continue
        if c == "/" and line[i:i + 2] == "//":
            break
        if c == '"':
            q = '"'
            out.append(c)
        elif c == "'" and re.match(r"'(\\.|[^'\\])'", line[i:i + 4] or ""):
            m = re.match(r"'(\\.[^']*|[^'\\])'", line[i:])
            if m:
                out.append("''")
                i += len(m.group(0))
                continue
            out.append(c)
        else:
            out.append(c)
        i += 1
    return "".join(out)


rows = []
totals = {}
for d in dirs:
    n_files = 0
    n_arms = 0
    n_lits = 0
    for root, subdirs, names in os.walk(d):
        subdirs[:] = sorted(s for s in subdirs if s not in SKIP)
        for name in sorted(names):
            if not name.endswith(".lu"):
                continue
            path = os.path.join(root, name)
            n_files += 1
            try:
                lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
            except OSError:
                continue
            pending = []  # or-pattern lines before the `=>`
            for no, raw in enumerate(lines, 1):
                line = strip(raw)
                if "=>" not in line:
                    s = line.strip()
                    # a pattern line continued by a later `| … =>` line
                    pending = (pending + [(no, line)])[-3:] if s.endswith("|") or s.startswith("|") else []
                    continue
                pat = line.split("=>", 1)[0]
                parts = pending + [(no, pat)]
                pending = []
                n_arms += 1
                for lno, text in parts:
                    for m in INT.finditer(text):
                        n_lits += 1
                        v = value(m.group(1))
                        if v < 128:
                            continue
                        smallest = next((p for p, hi in FITS if v <= hi), "none")
                        tail = text[m.end():m.end() + 3]
                        head = text[max(0, m.start() - 3):m.start()]
                        rng = ".." in tail or ".." in head
                        rows.append((d, os.path.relpath(path, d), lno, m.group(1), v, smallest, rng, raw.strip()))
    totals[d] = (n_files, n_arms, n_lits)

with open(out_path, "w") as out:
    out.write("# s285 literal-pattern census: integer literals >= 128 in arm patterns\n")
    out.write("# columns: tree file:line literal value smallest-holder range? | source line\n")
    for d, (f, a, l) in totals.items():
        out.write(f"# {d}: {f} .lu file(s), {a} arm line(s), {l} integer literal(s) in patterns\n")
    for d, rel, lno, text, v, smallest, rng, src in rows:
        out.write(f"{os.path.basename(d)} {rel}:{lno} {text} {v} {smallest} {'range' if rng else 'lit'} | {src}\n")
print(f"{len(rows)} candidate(s) in {sum(t[0] for t in totals.values())} file(s); written to {out_path}")
