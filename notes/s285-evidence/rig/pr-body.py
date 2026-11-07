#!/usr/bin/env python3
"""s285: the PR body: the note's summary and five sections, with the Linux
gauntlet record's and the plant record's lines quoted where the note's
test checklist names them. Every END line of both records is quoted, and
the plant's FAIL lines.

usage: pr-body.py NOTE.md GREEN.record PLANT.record OUT.md
"""
import sys
from pathlib import Path

note, green, plant, out = sys.argv[1:]
text = Path(note).read_text()
# Drop the note's title line: the PR has its own title.
body = text.split("\n", 1)[1].lstrip("\n")


def quoted(path, keep):
    lines = Path(path).read_text().splitlines()
    picked = [ln for ln in lines if keep(ln)]
    return "```text\n" + "\n".join(picked) + "\n```\n"


is_head = lambda ln: ln.startswith(("RECORD ", "RECORD-END", "PLANT"))
green_q = quoted(green, lambda ln: is_head(ln) or ln.startswith(("END ", "SKIP ")))
plant_q = quoted(plant, lambda ln: is_head(ln) or ln.startswith(("END ", "FAIL ", "SKIP ")))
records = (
    "\n### The Linux gauntlet record at the head\n\n"
    "Every step's `END` line, the header and the footer:\n\n" + green_q +
    "\n### The plant record\n\n"
    "The header, the plant's metadata, every `END` line and the `FAIL` lines:\n\n" + plant_q
)
Path(out).write_text(body.rstrip("\n") + "\n" + records)
print(f"wrote {out}: {len(Path(out).read_text())} bytes")
