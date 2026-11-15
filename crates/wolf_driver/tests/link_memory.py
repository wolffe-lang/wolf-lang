#!/usr/bin/env python3
"""Linux linker-entry RSS row for wolf-lang#1123.

Run with the compiler built at the commit being tested and pinned consumer
checkouts, for example:
  python3 link_memory.py --wolf target/debug/wolf --work target/link-memory \
      --case den=/path/to/den/src/main.lu --case pelt=/path/to/pelt/src/main.lu

The WOLF_LD probe samples live wolf processes in this build's process tree,
not ru_maxrss (which inherits the parent's high-water mark across exec).
Its bound is 64 MiB plus the sum of staged object sizes. Runtime archives
are not compiler heap and do not contribute to that allowance.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


def status(pid):
    try:
        values = {}
        for line in Path(f"/proc/{pid}/status").read_text().splitlines():
            key, _, value = line.partition(":")
            values[key] = value.strip()
        return values
    except (FileNotFoundError, ProcessLookupError):
        return None


def probe(args):
    root = int(Path(os.environ["WOLF_MEMORY_ROOT"]).read_text())
    processes = {}
    for path in Path("/proc").iterdir():
        if path.name.isdigit():
            values = status(int(path.name))
            if values is not None:
                processes[int(path.name)] = values
    family = {root}
    while True:
        expanded = family | {
            pid for pid, values in processes.items()
            if int(values["PPid"]) in family
        }
        if expanded == family:
            break
        family = expanded
    compiler = Path(os.environ["WOLF_MEMORY_COMPILER"])
    measured = []
    for pid in sorted(family):
        try:
            same_exe = Path(f"/proc/{pid}/exe").resolve(strict=True) == compiler
        except (FileNotFoundError, ProcessLookupError):
            same_exe = False
        if pid == root or same_exe:
            values = processes.get(pid)
            if values is not None and "VmRSS" in values:
                measured.append({"pid": pid, "rss_kib": int(values["VmRSS"].split()[0])})
    objects = sum(Path(arg).stat().st_size for arg in args if arg.endswith((".o", ".obj")))
    sample = {"processes": measured, "rss_kib": sum(p["rss_kib"] for p in measured),
              "objects_bytes": objects, "bound_kib": 65536 + (objects + 1023) // 1024}
    Path(os.environ["WOLF_MEMORY_SAMPLE"]).write_text(json.dumps(sample) + "\n")
    os.execvp(os.environ["WOLF_MEMORY_REAL_LINKER"], [os.environ["WOLF_MEMORY_REAL_LINKER"], *args])


def run():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wolf", required=True, type=Path)
    parser.add_argument("--work", required=True, type=Path)
    parser.add_argument("--case", action="append", default=[], help="NAME=ENTRY")
    parser.add_argument("--real-linker", default="cc")
    parser.add_argument("--std-root", type=Path)
    opts = parser.parse_args()
    if not Path("/proc/self/status").is_file():
        print("SKIP link-memory reason=requires-Linux-procfs")
        return 0
    wolf = opts.wolf.resolve(strict=True)
    work = opts.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    wrapper = work / "linker probe"
    # The wrapper has no shell interpolation of linker arguments or paths.
    wrapper.write_text("#!/usr/bin/env python3\nimport runpy\nrunpy.run_path(" + repr(str(Path(__file__).resolve())) + ", run_name='__main__')\n")
    wrapper.chmod(0o755)
    small = work / "small.lu"
    small.write_text("fn main() -> int { 0 }\n")
    failed = False
    for spelling in [f"small={small}", *opts.case]:
        name, entry = spelling.split("=", 1)
        if not name or Path(name).name != name:
            parser.error("case names must be single path components")
        case = work / name
        case.mkdir(exist_ok=True)
        home = case / "home"
        home.mkdir(exist_ok=True)
        temp = case / "temp"
        temp.mkdir(exist_ok=True)
        root = case / "root.pid"
        sample_path = case / "sample.json"
        root.unlink(missing_ok=True)
        sample_path.unlink(missing_ok=True)
        env = dict(os.environ, SHELL="/bin/sh", HOME=str(home), TMPDIR=str(temp),
                   WOLF_LD=str(wrapper), WOLF_MEMORY_ROOT=str(root),
                   WOLF_MEMORY_SAMPLE=str(sample_path), WOLF_MEMORY_COMPILER=str(wolf),
                   WOLF_MEMORY_REAL_LINKER=opts.real_linker)
        command = [str(wolf), "build", str(Path(entry).resolve(strict=True)), "--no-cache", "-o", str(case / "program")]
        if opts.std_root:
            command.extend(["--std-root", str(opts.std_root.resolve(strict=True))])
        print(f"CASE link-memory name={name} command={command!r}", flush=True)
        # The probe waits for the pid file; compilation need not be artificially delayed.
        with (case / "stdout").open("wb") as stdout, (case / "stderr").open("wb") as stderr:
            child = subprocess.Popen(command, env=env, stdout=stdout, stderr=stderr)
            root.write_text(str(child.pid))
            code = child.wait()
        if code != 0 or not sample_path.is_file():
            print(f"FAIL link-memory name={name} build_exit={code} sample={sample_path.exists()}", flush=True)
            print((case / "stderr").read_text(errors="replace"), flush=True)
            failed = True
            continue
        sample = json.loads(sample_path.read_text())
        print(f"MEASURE link-memory name={name} {json.dumps(sample, sort_keys=True)}", flush=True)
        if not sample["processes"] or sample["rss_kib"] > sample["bound_kib"]:
            print(f"FAIL link-memory name={name} compiler-RSS-exceeds-bound", flush=True)
            failed = True
        else:
            print(f"PASS link-memory name={name}", flush=True)
        if name == "small":
            result = subprocess.run([str(case / "program")], env=env, check=False)
            if result.returncode != 0:
                print(f"FAIL link-memory name=small program_exit={result.returncode}", flush=True)
                failed = True
    return int(failed)


if __name__ == "__main__":
    if "WOLF_MEMORY_ROOT" in os.environ:
        # Parent writes the root pid before the compile can reasonably finish,
        # but make the synchronization explicit rather than assuming timing.
        import time
        for _ in range(1000):
            path = Path(os.environ["WOLF_MEMORY_ROOT"])
            if path.is_file() and path.read_text().strip():
                break
            time.sleep(0.01)
        else:
            raise RuntimeError("missing compiler root pid")
        probe(sys.argv[1:])
    else:
        sys.exit(run())
