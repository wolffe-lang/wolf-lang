"""A syntactic upper bound on W0801 and the select E0806: every capitalized
bare name standing in a binder position, line by line, across checkouts.

usage: python3 -I binder-census.py DIR...

Reads each `.lu` file (skipping target/, .git/ and vendored upstream trees),
strips comments and string literals, and inspects the pattern part of every
statement `let`/`var` binder, `for` header, `else |…|` handler and `select`
receive (`NAME from`). A capitalized identifier there that is not followed by
`.`, `(` or `{` and not preceded by `.` (a path, a constructor, a struct
pattern's type) is reported. Item-level bindings (column 0) are not binder
positions for the rule and are listed apart. The compiler decides; this only
bounds what it could ever report.
"""
import os
import re
import sys

SKIP = {"target", ".git", "node_modules"}
CAP = re.compile(r"(?<![\w.])([A-Z][A-Za-z0-9_]*)\b(?!\s*[.({])")

def strip(line):
    out, i, q = [], 0, None
    while i < len(line):
        c = line[i]
        if q:
            if c == "\\":
                i += 2
                continue
            if c == q:
                q = None
            i += 1
            continue
        if c in "\"'":
            q = c
            out.append(" ")
        elif line.startswith("//", i):
            break
        else:
            out.append(c)
        i += 1
    return "".join(out)

def untyped(pat):
    """The pattern without a trailing `: type` annotation (a `:` at depth 0;
    a struct pattern's `k: p` sits inside its braces)."""
    depth = 0
    for i, c in enumerate(pat):
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        elif c == ":" and depth == 0:
            return pat[:i]
    return pat

def patterns(code):
    for m in re.finditer(r"\b(let|var)\s+([^=]*?)\s*=(?!=)", code):
        yield m.group(1), untyped(m.group(2))
    # the later binders of a comma group (D63): `let a = 1, (B, c) = …`
    if re.match(r"\s*(let|var)\b", code):
        for m in re.finditer(r",\s*(\([^=]*?\)|[A-Za-z_]\w*(?:\s*@\s*\([^=]*?\))?)\s*(?::[^=]*)?=(?!=)", code):
            yield "group", m.group(1)
    for m in re.finditer(r"\bfor\s+(.*?)\s+in\b", code):
        yield "for", m.group(1)
    for m in re.finditer(r"\belse\s*\|([^|]*)\|", code):
        yield "else", m.group(1)
    for m in re.finditer(r"^\s*([A-Za-z_]\w*)\s+from\b", code):
        yield "select", m.group(1)

total = 0
for d in sys.argv[1:]:
    hits, item_level, files = [], [], 0
    for root, subdirs, names in os.walk(d):
        subdirs[:] = sorted(s for s in subdirs if s not in SKIP and not (s == "upstream" and os.path.basename(root) == "vendor"))
        for n in sorted(names):
            if not n.endswith(".lu"):
                continue
            files += 1
            path = os.path.join(root, n)
            with open(path, encoding="utf-8", errors="replace") as fh:
                for no, raw in enumerate(fh, 1):
                    code = strip(raw.rstrip("\n"))
                    for kind, pat in patterns(code):
                        caps = CAP.findall(pat)
                        if not caps:
                            continue
                        rec = f"{os.path.relpath(path, d)}:{no}: {kind} {', '.join(caps)} | {raw.strip()[:110]}"
                        if kind in ("let", "var", "group") and not raw.startswith((" ", "\t")):
                            item_level.append(rec)
                        else:
                            hits.append(rec)
    total += len(hits)
    print(f"{d}: {files} file(s), {len(hits)} capitalized binder name(s), {len(item_level)} item-level")
    for h in hits:
        print("  " + h)
print(f"TOTAL capitalized binder names in binder positions: {total}")
