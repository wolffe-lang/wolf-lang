"""s287: a syntactic scan for writes to a `for` header's single name, over
every .lu file under the given dirs (a superset: no scopes, no types). For each
`for NAME in` it reads the lines after it whose indentation is deeper than the
header's, and reports a line that assigns NAME (`NAME =`, `NAME +=`, ...) or a
field of it (`NAME.f =`) unless a `var NAME` or `let NAME` came first.

usage: python3 -I forassign-scan.py DIR...
"""
import os
import re
import sys

FOR = re.compile(r"^(\s*).*\bfor\s+([a-z_][A-Za-z0-9_]*)\s+in\b")
SKIP = {"target", ".git"}
hits = 0
files = 0
for d in sys.argv[1:]:
    for root, subdirs, names in os.walk(d):
        subdirs[:] = [s for s in subdirs if s not in SKIP]
        for n in names:
            if not n.endswith(".lu"):
                continue
            files += 1
            p = os.path.join(root, n)
            lines = open(p, encoding="utf-8", errors="replace").read().split("\n")
            for i, line in enumerate(lines):
                m = FOR.match(line)
                if not m:
                    continue
                indent, name = len(m.group(1)), m.group(2)
                assign = re.compile(rf"^\s*{name}(\.[A-Za-z0-9_.]+)?\s*([-+*/%|&^]|<<|>>)?=(?!=)")
                shadow = re.compile(rf"^\s*(var|let)\s+{name}\b")
                for j in range(i + 1, min(i + 200, len(lines))):
                    body = lines[j]
                    if body.strip() == "":
                        continue
                    if len(body) - len(body.lstrip()) <= indent and not body.lstrip().startswith("//"):
                        break
                    if shadow.match(body):
                        break
                    if assign.match(body):
                        hits += 1
                        print(f"{p}:{j + 1}: {body.strip()}")
print(f"scan: {files} file(s), {hits} write(s) to a for header's name")
