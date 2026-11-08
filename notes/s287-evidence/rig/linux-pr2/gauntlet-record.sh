#!/usr/bin/env bash
# gauntlet-record — derive evidence from step END lines, never wrapper success.
# In a clean checkout, run:
#   bash /path/to/gauntlet-record.sh --output /lane/green.log -- bash gauntlet.sh
# The gauntlet prints START name; its output; END name exit=N secs=S for
# each step, stops on a nonzero step, and prints SKIP name reason=... loudly.
# Or in bash: source this file; gauntlet_step name command args... || exit $?
# For a one-file, one-hunk plant based on the green head:
#   ... --output /lane/red.log --base GREEN_SHA --plant row1 row2 -- bash plant.sh
# Each named row needs a literal FAIL row1 ... line inside a nonzero step.
# The audit independently reparses both records using --verify (internal API).
gauntlet_step() {
  local name=$1 rc started=$SECONDS
  shift
  printf 'START %s\n' "$name"
  if "$@"; then rc=0; else rc=$?; fi
  printf 'END %s exit=%s secs=%s\n' "$name" "$rc" "$((SECONDS-started))"
  return "$rc"
}
[ "${BASH_SOURCE[0]}" = "$0" ] || return 0
exec python3 - "$@" <<'PY'
import argparse
import datetime as dt
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import time

NAME = r"[A-Za-z0-9_.:/-]+"
SHA = r"[0-9a-f]{40}"
UTC = r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ"
HEADER = re.compile(rf"RECORD repo=({NAME}) head=({SHA}) tree=({SHA}) host=(\S+) boot=(\S+) start=({UTC})")
FOOTER = re.compile(rf"RECORD-END worst=(\d+) steps=(\d+) end=({UTC})")
START = re.compile(rf"START ({NAME})")
END = re.compile(rf"END ({NAME}) exit=(\d+) secs=(\d+(?:\.\d+)?)")
SKIP = re.compile(rf"SKIP ({NAME}) reason=(\S.*)")

def require(condition, message):
    if not condition:
        raise ValueError(message)

def git(repo, *args):
    return subprocess.check_output(["git", "-C", str(repo), *args], text=True, stderr=subprocess.PIPE).strip()

def utc():
    return dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

def stamp(s):
    return dt.datetime.strptime(s, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=dt.timezone.utc).timestamp()

def parse_steps(lines):
    active = None
    names = set()
    steps = []
    skips = []
    for line in lines:
        if line.startswith("START"):
            m = START.fullmatch(line)
            require(m is not None and active is None, "malformed or nested START")
            name = m[1]
            require(name not in names, "duplicate step name")
            names.add(name)
            active = [name, []]
        elif line.startswith("END"):
            m = END.fullmatch(line)
            require(m is not None and active is not None and m[1] == active[0], "unmatched or malformed END")
            code = int(m[2])
            require(code <= 255, "invalid END exit")
            steps.append((active[0], code, active[1]))
            active = None
        elif line.startswith("SKIP"):
            m = SKIP.fullmatch(line)
            require(m is not None, "SKIP needs a reason")
            require(active is None and m[1] not in names, "nested or duplicate SKIP")
            names.add(m[1])
            skips.append(line)
        elif line.startswith(("RECORD", "PLANT")):
            raise ValueError("unexpected record metadata inside steps")
        elif active is not None:
            active[1].append(line)
        else:
            require(not line.strip(), "output outside a step")
    require(active is None, "unfinished step")
    require(bool(steps), "record ran no steps")
    return steps, skips

def verify(path, repo, head, expected_repo, plant=False, base=None):
    lines = Path(path).read_text().splitlines()
    require(len(lines) >= 4, "incomplete record")
    h = HEADER.fullmatch(lines[0])
    f = FOOTER.fullmatch(lines[-1])
    require(h is not None and f is not None, "malformed record header/footer")
    require(h[1] == expected_repo, "record repo does not match")
    require(h[2] == head, "record head does not match")
    require(h[3] == git(repo, "rev-parse", head + "^{tree}"), "record tree does not match")
    ct = int(git(repo, "show", "-s", "--format=%ct", head))
    start, end = stamp(h[6]), stamp(f[3])
    require(end >= ct and start >= ct, "record predates head")
    require(start <= end <= time.time() + 5, "record has invalid time order")
    body = lines[1:-1]
    rows = []
    if plant:
        require(bool(body), "missing plant metadata")
        m = re.fullmatch(rf"PLANT base=({SHA}) branch=({NAME})", body.pop(0))
        require(m is not None and m[1] == base, "plant base differs from main head")
        require(m[2] not in ("HEAD", "trunk"), "plant must name a branch")
        refs = ["refs/heads/" + m[2], "refs/remotes/origin/" + m[2]]
        heads = []
        for ref in refs:
            try:
                heads.append(git(repo, "rev-parse", "--verify", ref))
            except subprocess.CalledProcessError:
                pass
        require(bool(heads) and all(ph == head for ph in heads), "plant branch head does not match")
        require(git(repo, "rev-parse", head + "^@") == base, "plant is not one commit on main head")
        paths = git(repo, "diff", "--name-only", base, head).splitlines()
        diff = git(repo, "diff", "--no-ext-diff", "--unified=0", base, head)
        require(len(paths) == 1 and len(re.findall(r"^@@", diff, re.M)) == 1, "plant needs a one-break diff (one file, one hunk)")
        while body and body[0].startswith("PLANT-ROW "):
            row = body.pop(0)[10:]
            require(re.fullmatch(NAME, row) is not None and row not in rows, "invalid or duplicate plant row")
            rows.append(row)
        require(bool(rows), "plant names no rows")
    steps, skips = parse_steps(body)
    worst = max(code for _, code, _ in steps)
    if not plant:
        require(worst == 0, "nonzero END in local gauntlet")
    require(int(f[1]) == worst and int(f[2]) == len(steps), "record summary disagrees with END lines")
    if plant:
        for row in rows:
            matches = [line for _, code, log in steps if code for line in log
                       if re.match(r"^FAIL " + re.escape(row) + r"(?:\s|$)", line)]
            require(bool(matches), "missing FAIL line for " + row)
            for line in matches:
                print("  ok   plant evidence: " + line)
    for skip in skips:
        print("  INFO " + skip)
    print(f"  ok   {'plant' if plant else 'local'} record verified: worst={worst} steps={len(steps)} head={head}")
    return h[2]

def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--verify":
        require(len(sys.argv) in (6, 7), "invalid verify arguments")
        _, _, record, repo, head, name, *plant = sys.argv
        verify(record, repo, head, name)
        if plant:
            lines = Path(plant[0]).read_text().splitlines()
            ph = HEADER.fullmatch(lines[0]) if lines else None
            require(ph is not None, "malformed plant header")
            verify(plant[0], repo, ph[2], name, plant=True, base=head)
        return 0
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True)
    parser.add_argument("--base")
    parser.add_argument("--plant", nargs="+")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command
    if command[:1] == ["--"]:
        command = command[1:]
    require(bool(command), "a gauntlet command is required")
    require(not git(".", "status", "--porcelain", "--untracked-files=no"), "tracked checkout is dirty")
    repo = git(".", "rev-parse", "--show-toplevel")
    remote = git(repo, "remote", "get-url", "origin")
    name = remote.rstrip("/").rsplit("/", 1)[-1].removesuffix(".git")
    head = git(repo, "rev-parse", "HEAD")
    tree = git(repo, "rev-parse", "HEAD^{tree}")
    bootpath = Path("/proc/sys/kernel/random/boot_id")
    if bootpath.exists():
        boot = bootpath.read_text().strip()
    else:
        boot = subprocess.check_output(["sysctl", "-n", "kern.boottime"], text=True).strip().replace(" ", "_")
    require(re.fullmatch(NAME, name) is not None, "invalid repository name")
    with open(args.output, "x", buffering=1) as out:
        out.write(f"RECORD repo={name} head={head} tree={tree} host={socket.gethostname()} boot={boot} start={utc()}\n")
        if args.plant:
            require(args.base is not None and re.fullmatch(SHA, args.base) is not None, "plant needs --base full sha")
            branch = git(repo, "symbolic-ref", "--short", "HEAD")
            out.write(f"PLANT base={args.base} branch={branch}\n")
            for row in args.plant:
                require(re.fullmatch(NAME, row) is not None, "invalid plant row")
                out.write(f"PLANT-ROW {row}\n")
        # stdin is not the heredoc that carries this implementation.
        proc = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        lines = []
        for line in proc.stdout:
            out.write(line)
            print(line, end="", flush=True)
            lines.append(line.rstrip("\n"))
        rc = proc.wait()
        # A red wrapper cannot masquerade as green either. It is supplemental
        # evidence only: max(END exits), not rc alone, remains the verdict.
        if rc:
            code = rc if rc > 0 else 128 - rc
            extra = ["START record-wrapper", f"END record-wrapper exit={code} secs=0"]
            out.write("".join(line + "\n" for line in extra))
            lines.extend(extra)
        require(git(repo, "rev-parse", "HEAD") == head and not git(repo, "status", "--porcelain", "--untracked-files=no"), "gauntlet changed tracked checkout")
        steps, skips = parse_steps(lines)
        worst = max(code for _, code, _ in steps)
        out.write(f"RECORD-END worst={worst} steps={len(steps)} end={utc()}\n")
    if args.plant:
        verify(args.output, repo, head, name, plant=True, base=args.base)
    return worst

try:
    sys.exit(main())
except (ValueError, OSError, subprocess.CalledProcessError) as exc:
    print("  FAIL " + str(exc), file=sys.stderr)
    sys.exit(1)
PY
