"""A syntactic upper bound on W0801 (binder half) and the select E0806, in
either letter case: every bare name in a binder position that equals a
variant name some enum in the same checkout declares.

usage: python3 -I variant-census.py DIR...

Per DIR (skipping target/, .git/ and vendored upstream trees): strips
comments and string literals from every `.lu` file; collects the variant
names of every `enum … { … }` body in the tree (the first identifier of each
depth-0 comma item); then inspects the pattern part of every statement
`let`/`var` binder (and the later binders of a comma group), `for` header,
`else |…|` handler and `select` receive (`NAME from`), and reports each
identifier there that is not a path segment, a constructor or a struct
pattern's type, and that equals one of those variant names. The tree-wide
set is wider than "in scope" (the binder's module and its item imports),
so this bounds from above what the compiler could ever report; the compiler
decides. Item-level bindings (column 0) are listed apart.
"""
import os
import re
import sys

SKIP = {"target", ".git", "node_modules"}
IDENT = re.compile(r"(?<![\w.])([A-Za-z_][A-Za-z0-9_]*)\b(?!\s*[.({])")
KEYWORDS = {"mut", "in", "let", "var", "for", "from", "else", "_"}


def strip(text):
    out, i, q = [], 0, None
    while i < len(text):
        c = text[i]
        if q:
            if c == "\\":
                out.append(" ")
                i += 2
                continue
            if c == q:
                q = None
            out.append("\n" if c == "\n" else " ")
            i += 1
            continue
        if c in "\"'":
            q = c
            out.append(" ")
        elif text.startswith("//", i):
            j = text.find("\n", i)
            j = len(text) if j < 0 else j
            out.append(" " * (j - i))
            i = j
            continue
        else:
            out.append(c)
        i += 1
    return "".join(out)


def variants(code):
    names = set()
    for m in re.finditer(r"\benum\b[^{;=]*\{", code):
        depth, i, start = 1, m.end(), m.end()
        while i < len(code) and depth:
            depth += {"{": 1, "(": 1, "[": 1, "}": -1, ")": -1, "]": -1}.get(code[i], 0)
            i += 1
        body = code[start:i - 1]
        item, depth = [], 0
        for c in body + ",":
            if c in "{([":
                depth += 1
            elif c in "})]":
                depth -= 1
            if c == "," and depth == 0:
                text = re.sub(r"#\[[^\]]*\]", " ", "".join(item))
                lead = re.match(r"\s*([A-Za-z_]\w*)", text)
                if lead:
                    names.add(lead.group(1))
                item = []
            else:
                item.append(c)
    return names


def untyped(pat):
    depth = 0
    for i, c in enumerate(pat):
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        elif c == ":" and depth == 0:
            return pat[:i]
    return pat


def patterns(line):
    for m in re.finditer(r"\b(let|var)\s+([^=]*?)\s*=(?!=)", line):
        yield m.group(1), untyped(m.group(2))
    if re.match(r"\s*(let|var)\b", line):
        for m in re.finditer(r",\s*(\([^=]*?\)|[A-Za-z_]\w*(?:\s*@\s*\([^=]*?\))?)\s*(?::[^=]*)?=(?!=)", line):
            yield "group", m.group(1)
    for m in re.finditer(r"\bfor\s+(.*?)\s+in\b", line):
        yield "for", m.group(1)
    for m in re.finditer(r"\belse\s*\|([^|]*)\|", line):
        yield "else", m.group(1)
    for m in re.finditer(r"^\s*([A-Za-z_]\w*)\s+from\b", line):
        yield "select", m.group(1)


total = 0
for d in sys.argv[1:]:
    files = []
    for root, subdirs, names in os.walk(d):
        subdirs[:] = sorted(s for s in subdirs if s not in SKIP and not (s == "upstream" and os.path.basename(root) == "vendor"))
        files.extend(os.path.join(root, n) for n in sorted(names) if n.endswith(".lu"))
    stripped = {}
    declared = set()
    for path in files:
        with open(path, encoding="utf-8", errors="replace") as fh:
            raw = fh.read()
        stripped[path] = (raw.splitlines(), strip(raw).splitlines())
        declared |= variants("\n".join(stripped[path][1]))
    hits, item_level = [], []
    for path in files:
        raws, codes = stripped[path]
        for no, code in enumerate(codes, 1):
            raw = raws[no - 1] if no - 1 < len(raws) else ""
            for kind, pat in patterns(code):
                found = [n for n in IDENT.findall(pat) if n in declared and n not in KEYWORDS]
                if not found:
                    continue
                rec = f"{os.path.relpath(path, d)}:{no}: {kind} {', '.join(found)} | {raw.strip()[:110]}"
                if kind in ("let", "var", "group") and not raw.startswith((" ", "\t")):
                    item_level.append(rec)
                else:
                    hits.append(rec)
    total += len(hits)
    lower = sum(1 for v in declared if v[:1].islower())
    print(f"{d}: {len(files)} file(s), {len(declared)} variant name(s) declared ({lower} lowercase), "
          f"{len(hits)} binder name(s) spelling one, {len(item_level)} item-level")
    for h in hits:
        print("  " + h)
    for h in item_level:
        print("  item-level " + h)
print(f"TOTAL binder names spelling a declared variant, any case: {total}")
