"""s270 differ comparison: per tier, every corpus entry's record from
impl A (head) against impl B (base). Prints each entry whose meaning
moved (verdict, stdout digest, deepest phase, or diagnostic codes)."""
import json, sys

def key(rec):
    r = rec.get("record") or {}
    codes = sorted({d.get("code", "") for d in r.get("diagnostics", []) if isinstance(d, dict)})
    return (
        r.get("verdict", "<no record: %s>" % rec.get("raw_stdout", "")[:60]),
        r.get("stdout_sha256", ""),
        r.get("phase_reached", ""),
        tuple(codes),
    )

total_moved = 0
for tier in sys.argv[2:]:
    path = "%s/s270c-differ-%s.jsonl" % (sys.argv[1], tier)
    a, b = {}, {}
    with open(path) as fh:
        for line in fh:
            rec = json.loads(line)
            f = rec["argv"][1] if len(rec["argv"]) > 1 else "?"
            (a if rec["proxy"].endswith("-A") else b)[f] = rec
    files = sorted(set(a) | set(b))
    moved = []
    for f in files:
        if f not in a or f not in b:
            moved.append((f, "only on one side"))
            continue
        ka, kb = key(a[f]), key(b[f])
        if ka != kb:
            ra = a[f].get("record") or {}
            rb = b[f].get("record") or {}
            moved.append((f, "base %s %r -> head %s %r" % (kb[0], (rb.get("stdout_inline") or "")[:40], ka[0], (ra.get("stdout_inline") or "")[:40])))
    print("TIER %s: %d entries compared, %d moved" % (tier, len(files), len(moved)))
    for f, why in moved:
        print("  MOVED %s: %s" % (f, why))
    total_moved += len(moved)
print("TOTAL MOVED %d" % total_moved)
