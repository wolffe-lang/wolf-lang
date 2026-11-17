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
import struct
import sys

ROW = "function-placement-stability"
MODULES = 6
FUNCTIONS = 24
PAGE = 4096


def run(argv, cwd, env, log, inspect_layout_refusal=False):
    result = subprocess.run(list(map(str, argv)), cwd=cwd, env=env,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            timeout=180)
    log.with_suffix(".stdout").write_bytes(result.stdout)
    log.with_suffix(".stderr").write_bytes(result.stderr)
    log.with_suffix(".exit").write_text(str(result.returncode) + "\n")
    # A deliberate ordering plant can also trip the compiler's linked-image
    # verifier. Still inspect its actual emitted executable; no other build
    # error is accepted, and the symbol comparison must independently fail.
    layout_refusal = inspect_layout_refusal and b"wolf build: linker did not honor stable function layout:" in result.stderr
    if result.returncode and not layout_refusal:
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


def macho_table(path):
    """Read Mach-O64 section and nlist tables, not a disassembly heuristic.

    Mach-O has no ELF st_size. Its size column is explicitly an atom extent
    (next external text symbol/section end), including alignment padding.
    """
    data = path.read_bytes()
    if data[:4] != b"\xcf\xfa\xed\xfe":
        raise RuntimeError("expected little-endian Mach-O64")
    commands = struct.unpack_from("<I", data, 16)[0]
    sections = {}
    symtab = None
    offset = 32
    for _ in range(commands):
        command, size = struct.unpack_from("<II", data, offset)
        if size < 8 or offset + size > len(data):
            raise RuntimeError("invalid Mach-O load command")
        if command == 0x19:
            count = struct.unpack_from("<I", data, offset + 64)[0]
            if 72 + count * 80 > size:
                raise RuntimeError("invalid Mach-O section table")
            for i in range(count):
                at = offset + 72 + i * 80
                name = data[at:at + 16].split(b"\0")[0].decode()
                address, length = struct.unpack_from("<QQ", data, at + 32)
                sections[len(sections) + 1] = dict(name=name, address=address, size=length)
        elif command == 2:
            symtab = struct.unpack_from("<IIII", data, offset + 8)
        offset += size
    if symtab is None:
        raise RuntimeError("missing Mach-O symbol table")
    start, count, strings, string_size = symtab
    raw = []
    for i in range(count):
        name_at, kind, section, desc, address = struct.unpack_from("<IBBHQ", data, start + i * 16)
        if kind & 0xe0 or kind & 0x0e != 0x0e or not kind & 1:
            continue
        if name_at >= string_size or section not in sections:
            raise RuntimeError("invalid Mach-O symbol")
        end = data.index(b"\0", strings + name_at, strings + string_size)
        name = data[strings + name_at:end].decode()
        if sections[section]["name"] in ["__text", "__wolf_cold"]:
            raw.append(dict(symbol=name, address=address, section=str(section)))
    for row in raw:
        section = sections[int(row["section"])]
        ends = [s["address"] for s in raw if s["section"] == row["section"] and s["address"] > row["address"]]
        ends.append(section["address"] + section["size"])
        row["size"] = min(ends) - row["address"]
        row["size_kind"] = "Mach-O symbol atom extent, including padding"
    return sections, raw


def symbols(path, env, log):
    if path.read_bytes()[:4] == b"\xcf\xfa\xed\xfe":
        sections, raw = macho_table(path)
        log.with_suffix(".stdout").write_text(json.dumps(dict(sections=sections, symbols=raw), indent=2))
        return {canonical(row["symbol"][1:]): row for row in raw if canonical(row["symbol"][1:])}
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
    if path.read_bytes()[:4] == b"\xcf\xfa\xed\xfe":
        sections, _ = macho_table(path)
        log.with_suffix(".stdout").write_text(json.dumps(sections, indent=2))
        matches = [s["address"] for s in sections.values() if s["name"] == "__text"]
        if len(matches) != 1:
            raise RuntimeError("expected one linked Mach-O __text section")
        return matches[0]
    raw = run(["readelf", "--wide", "--section-headers", path], path.parent, env, log)
    match = re.search(r"\[\s*\d+\]\s+\.text(?:\.wolf)?\s+PROGBITS\s+([0-9a-fA-F]+)\s", raw)
    if not match:
        raise RuntimeError(f"no .text section in {path}")
    return int(match.group(1), 16)


def cold_pages(path, env, log):
    if path.read_bytes()[:4] != b"\x7fELF":
        return None  # Mach-O has a different section contract, tested separately.
    raw = run(["readelf", "--wide", "--section-headers", path], path.parent, env, log)
    sections = []
    for line in raw.splitlines():
        match = re.match(r"\s*\[\s*\d+\]\s+(\S+)\s+\S+\s+([0-9a-fA-F]+)\s+\S+\s+([0-9a-fA-F]+)\s+\S+\s+(\S+)", line)
        if match and "X" in match[4]:
            sections.append(dict(name=match[1], address=int(match[2], 16), size=int(match[3], 16)))
    cold = [section for section in sections if section["name"] == ".wolf.cold"]
    if len(cold) != 1:
        return dict(passed=False, reason="expected one .wolf.cold section")
    cold = cold[0]
    page = os.sysconf("SC_PAGESIZE")
    def pages(section):
        if not section["size"]:
            return set()
        return set(range(section["address"] // page, (section["address"] + section["size"] + page - 1) // page))
    overlaps = [section["name"] for section in sections if section["name"] != ".wolf.cold" and pages(section) & pages(cold)]
    aligned = cold["address"] % page == 0 and cold["size"] % page == 0
    return dict(passed=aligned and not overlaps, page_size=page, cold=cold, overlaps=overlaps)


def build(wolf, path, tier, env, cold, linker):
    exe = path / "program"
    argv = [wolf, "build", path / "main.lu", "--verbose", "-o", exe]
    if tier == "release":
        argv.append("--release")
    for name in cold:
        argv.extend(["--cold-function", name])
    if linker:
        argv.extend(["--linker", linker])
    run(argv, path, env, path / "build", inspect_layout_refusal=True)
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
    expected = "54\n" if path.name == "after" else "48\n"
    if output != expected:
        raise RuntimeError(f"fixture computed {output!r}, expected {expected!r}")
    return dict(objects=object_symbols, linked=linked, object_count=len(objects),
                build_exit=int((path / "build.exit").read_text()),
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
    parser.add_argument("--cold-function", action="append", default=[])
    parser.add_argument("--linker", type=Path)
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
    linker = args.linker.resolve() if args.linker else None
    before = build(wolf, root / "before", args.tier, env, args.cold_function, linker)
    after = build(wolf, root / "after", args.tier, env, args.cold_function, linker)
    report = compare(before, after)
    report["compiler_verified"] = before["build_exit"] == after["build_exit"] == 0
    report["pass"] = report["pass"] and report["compiler_verified"]
    if args.cold_function:
        page_results = []
        for which, image in [("before", before), ("after", after)]:
            linked = image["linked"]
            cold_rows = [linked[name] for name in args.cold_function]
            hot_rows = [row for name, row in linked.items() if name not in args.cold_function]
            cold_sections = {row["section"] for row in cold_rows}
            separate = len(cold_sections) == 1 and all(row["section"] not in cold_sections for row in hot_rows)
            trailing = min(row["address"] for row in cold_rows) >= max(row["address"] + row["size"] for row in hot_rows)
            report[which + "_cold_separate_trailing"] = separate and trailing
            report["pass"] = report["pass"] and separate and trailing
            pages = cold_pages(root / which / "program", env, root / which / "cold-pages")
            if pages is not None:
                page_results.append(pages["passed"])
                report[which + "_cold_page_isolation"] = pages
                report["pass"] = report["pass"] and pages["passed"]
        if page_results:
            print(("PASS" if all(page_results) else "FAIL") + " cold-page-isolation", flush=True)
        print(("PASS" if report["pass"] else "FAIL") + " cold-function-placement", flush=True)
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
