"""Measure addition locality from real ELF objects and linked function symbols.

This also runs against a released compiler, before a compiler fix exists. A
compile/tool/fixture failure exits 2; only a measured locality violation exits 1
and prints the named FAIL row. All inputs and symbol tables remain in --output.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROW = "function-placement-stability"
MODULES = 6
FUNCTIONS = 24
PAGE = 4096


def run(argv, cwd, env, log):
    result = subprocess.run(list(map(str, argv)), cwd=cwd, env=env,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            timeout=180)
    log.with_suffix(".stdout").write_bytes(result.stdout)
    log.with_suffix(".stderr").write_bytes(result.stderr)
    if result.returncode:
        raise RuntimeError(f"{argv}: exit {result.returncode}\n"
                           + result.stderr.decode(errors="replace"))
    return result.stdout.decode()


def body(module, function):
    # Recursive calls are not WIR-inline candidates. Distinct constants prevent
    # content deduplication; the dynamic depth prevents whole-program folding.
    name = f"f{function:02}"
    salt = 101 + module * 37 + function
    return (f"pub fn {name}(x: int) -> int {{\n"
            f"    if x == 0 {{ {salt} }} else {{\n"
            f"        ({name}(x - 1) ^ {salt}) + (x & 7)\n"
            "    }\n}\n")


def fixture(path, added):
    path.mkdir()
    for module in range(MODULES):
        folder = path / f"m{module:02}"
        folder.mkdir()
        text = "//! member: true\n\n"
        text += "\n".join(body(module, f) for f in range(FUNCTIONS))
        if added and module == 0:
            text += "\n" + body(module, 99)
        (folder / "code.lu").write_text(text)
    text = "\n".join(f"use m{m:02}" for m in range(MODULES))
    text += "\n\nfn main() {\n    let depth = env_args().len + 1\n    var answer = 0\n"
    for module in range(MODULES):
        for function in range(FUNCTIONS):
            text += f"    answer = answer ^ m{module:02}.f{function:02}(depth)\n"
    if added:
        text += "    answer = answer ^ m00.f99(depth)\n"
    text += '    print("{answer}")\n}\n'
    (path / "main.lu").write_text(text)


def canonical(symbol):
    match = re.fullmatch(r"_W(m[0-9]{2}\.f[0-9]{2})\$[^\s]+", symbol)
    return match.group(1) if match else None


def symbols(path, env, log):
    raw = run(["readelf", "--wide", "--symbols", path], path.parent, env, log)
    out = {}
    for line in raw.splitlines():
        parts = line.split()
        if len(parts) != 8 or parts[3] != "FUNC" or parts[6] == "UND":
            continue
        key = canonical(parts[7])
        if key is None:
            continue
        value = dict(symbol=parts[7], address=int(parts[1], 16),
                     size=int(parts[2]), section=parts[6])
        if key in out and out[key] != value:
            raise RuntimeError(f"conflicting definitions of {key} in {path}")
        out[key] = value
    return out


def text_origin(path, env, log):
    raw = run(["readelf", "--wide", "--section-headers", path], path.parent, env, log)
    match = re.search(r"\[\s*\d+\]\s+\.text\s+PROGBITS\s+([0-9a-fA-F]+)\s", raw)
    if not match:
        raise RuntimeError(f"no .text section in {path}")
    return int(match.group(1), 16)


def build(wolf, path, tier, env):
    exe = path / "program"
    argv = [wolf, "build", path / "main.lu", "--verbose", "-o", exe]
    if tier == "release":
        argv.append("--release")
    run(argv, path, env, path / "build")
    objects = sorted((path / ".lu-cache" / "obj").glob("*.o"))
    if not objects:
        raise RuntimeError("no real relocatable objects found")
    if tier == "release" and len(objects) < 2:
        raise RuntimeError("fixture did not exercise multiple release clusters")
    object_symbols = {}
    for index, obj in enumerate(objects):
        rows = symbols(obj, env, path / f"object-{index:02}-symbols")
        for name, row in rows.items():
            if name in object_symbols:
                raise RuntimeError(f"duplicate object definition of {name}")
            object_symbols[name] = dict(row, object=obj.name)
    linked = symbols(exe, env, path / "linked-symbols")
    origin = text_origin(exe, env, path / "linked-sections")
    for name, row in linked.items():
        if name not in object_symbols:
            raise RuntimeError(f"linked {name} has no object definition")
        if row["symbol"] != object_symbols[name]["symbol"]:
            raise RuntimeError(f"object/linked identity mismatch for {name}")
        row["offset"] = row["address"] - origin
    output = run([exe, "one", "two"], path, env, path / "run")
    return dict(objects=object_symbols, linked=linked, object_count=len(objects),
                text_origin=origin, output=output,
                executable_sha256=hashlib.sha256(exe.read_bytes()).hexdigest())


def compare(before, after):
    left, right = before["linked"], after["linked"]
    inserted = right.get("m00.f99")
    if inserted is None or not 0 < inserted["size"] < PAGE // 2:
        raise RuntimeError(f"inserted function not retained or too large: {inserted}")
    expected = {f"m{m:02}.f{f:02}" for m in range(1, MODULES)
                for f in range(FUNCTIONS)}
    missing = sorted(expected - (left.keys() & right.keys()))
    if missing:
        raise RuntimeError(f"unrelated fixture functions not retained: {missing}")
    common = sorted(expected)
    changed_size = [n for n in common if left[n]["size"] != right[n]["size"]]
    same_size = [n for n in common if n not in changed_size]
    if len(same_size) < 80:
        raise RuntimeError(f"too few unchanged-size functions: {len(same_size)}")
    before_order = sorted(common, key=lambda n: (left[n]["address"], n))
    after_order = sorted(common, key=lambda n: (right[n]["address"], n))
    deltas = {n: right[n]["offset"] - left[n]["offset"] for n in same_size}
    moved = {n: d for n, d in deltas.items() if abs(d) > PAGE}
    report = dict(inserted_bytes=inserted["size"], compared=len(common),
                  unchanged_size=len(same_size), changed_size=changed_size,
                  relative_order_unchanged=before_order == after_order,
                  max_absolute_displacement=max(map(abs, deltas.values())),
                  beyond_page=moved, displacements=deltas,
                  before_order=before_order, after_order=after_order)
    report["pass"] = report["relative_order_unchanged"] and not moved
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wolf", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--tier", choices=["native", "release"], default="release")
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ)
    env.update(SHELL="/bin/sh", LC_ALL="C", RAYON_NUM_THREADS="1")
    for name in ["home", "tmp"]:
        (root / name).mkdir()
    env.update(HOME=str(root / "home"), TMPDIR=str(root / "tmp"))
    wolf = args.wolf.resolve()
    version = run([wolf, "--version"], root, env, root / "compiler")
    fixture(root / "before", False)
    fixture(root / "after", True)
    before = build(wolf, root / "before", args.tier, env)
    after = build(wolf, root / "after", args.tier, env)
    report = compare(before, after)
    (root / "evidence.json").write_text(json.dumps(dict(
        compiler=version.strip(), compiler_sha256=hashlib.sha256(wolf.read_bytes()).hexdigest(),
        tier=args.tier, before=before, after=after, comparison=report), indent=2) + "\n")
    print(json.dumps(report, sort_keys=True), flush=True)
    verdict = "PASS" if report["pass"] else "FAIL"
    print(f"{verdict} {ROW}", flush=True)
    return 0 if report["pass"] else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError, ValueError) as error:
        print(f"ERROR {ROW}: {error}", file=sys.stderr, flush=True)
        sys.exit(2)
