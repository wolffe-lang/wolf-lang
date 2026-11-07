#!/usr/bin/env python3
"""s285: for every census candidate (litpat-census.py's output), find the
nearest enclosing `match <scrutinee> {` above it and, for a scrutinee that is
a plain name or a `name as T` cast, the declared type of that name in the
same file (a parameter `name: T`, a `let/var name: T`, or a struct field
`name: T`). Prints one line per (file, match line) with the candidates it
holds, so each can be read by hand.

usage: census-scrutinee.py CENSUS.txt ROOT_OF_TREE...
"""
import collections
import os
import re
import sys

census, *roots = sys.argv[1:]
root_of = {os.path.basename(r): r for r in roots}
groups = collections.OrderedDict()
for line in open(census):
    if line.startswith("#"):
        continue
    head, src = line.split(" | ", 1)
    tree, loc, text, value, smallest, kind = head.split()
    rel, lno = loc.rsplit(":", 1)
    path = os.path.join(root_of[tree], rel)
    lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
    i = int(lno) - 1
    scrut = None
    mline = None
    while i >= 0:
        m = re.search(r"\bmatch\s+(.+?)\s*\{\s*(//.*)?$", lines[i]) or re.search(r"\bmatch\s+(.+?)\s*\{", lines[i])
        if m:
            scrut = m.group(1)
            mline = i + 1
            break
        i -= 1
    key = (tree, rel, mline, scrut)
    groups.setdefault(key, []).append((text, int(value), smallest, kind))

decl_cache = {}


def decl_type(tree, rel, name):
    path = os.path.join(root_of[tree], rel)
    src = open(path, encoding="utf-8", errors="replace").read()
    hits = set()
    for m in re.finditer(rf"\b(?:let|var|mut|take|read)?\s*\b{re.escape(name)}\s*:\s*([A-Za-z_][\w\[\], ]*)", src):
        hits.add(m.group(1).strip().split(",")[0].split(")")[0].strip())
    return sorted(hits)


for (tree, rel, mline, scrut), cands in groups.items():
    vals = sorted({v for _, v, _, _ in cands})
    need = min(s for _, _, s, _ in cands)
    ty = "?"
    if scrut:
        m = re.fullmatch(r"\(?\s*([A-Za-z_][\w.]*)\s*(?:as\s+([\w\[\]]+))?\s*\)?", scrut)
        if m and m.group(2):
            ty = "cast:" + m.group(2)
        elif m:
            last = m.group(1).split(".")[-1]
            ty = "decl:" + ("/".join(decl_type(tree, rel, last)) or "none-found")
        else:
            ty = "expr"
    print(f"{tree} {rel}:{mline} match {scrut!r} -> {ty} | {len(cands)} cand, values {vals[:6]}{'...' if len(vals) > 6 else ''}")
