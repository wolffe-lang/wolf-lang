#!/usr/bin/env python3
"""Measure producer debug sections on positive conformance inputs.

Run under the lane's Linux memory/CPU cap. Build the inspector with:
  cargo build -p wolf_driver --example dwarf_sections
The output records every selection, refusal, digest and determinism comparison.
A missing measurement is never a pass. A head-only compile failure is a failure.
"""
import argparse
import hashlib
import json
import re
import shutil
import subprocess
from pathlib import Path


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(65536), b""):
            h.update(block)
    return h.hexdigest()


def command(argv, cwd, directory, label, timeout):
    out = directory / (label + ".stdout.txt")
    err = directory / (label + ".stderr.txt")
    with out.open("wb") as stdout, err.open("wb") as stderr:
        try:
            result = subprocess.run(argv, cwd=cwd, stdout=stdout, stderr=stderr,
                                    timeout=timeout, check=False)
            code = result.returncode
        except subprocess.TimeoutExpired:
            code = 124
    return code, out, err


def inputs(corpus, all_entries=False):
    # A corpus selects its positive run entries. A fixture directory (one
    # entry per subdirectory, no directive headers) selects every entry.
    selected = []
    for path in sorted(corpus.rglob("*.lu")):
        if path.is_symlink():
            continue
        text = path.read_bytes().decode("utf-8", errors="replace")
        if all_entries or re.search(r"(?m)^//!\s*check:\s*run\(", text):
            selected.append(path)
    return selected


DUMP_ARGS = ["--verbose", "--debug-info", "--debug-abbrev", "--debug-line", "--debug-frame"]


def vocabulary(text):
    # The DWARF names the producer emitted. The line-table header's
    # standard_opcode_lengths legend names every standard opcode, used or not.
    return sorted(set(re.findall(r"\bDW_(?:TAG|AT|FORM|CFA|OP|LNS|LNE)_[A-Za-z0-9_]+",
                                 "\n".join(line for line in text.splitlines()
                                           if not line.startswith("standard_opcode_lengths[")))))


def dump_argv(tool, obj):
    # Run in the object's directory and name it relatively, so the dump's
    # header line carries no host path. DW_AT_comp_dir and the line table's
    # file names still record the compile's directory and entry path.
    return [str(tool), *DUMP_ARGS, obj.name]


def stage(path, destination):
    # A standalone corpus entry is an isolated module. A main.lu fixture keeps
    # its multi-file module tree. Never follow corpus symlinks out of the tree.
    source = path.parent
    if path.name == "main.lu":
        if any(p.is_symlink() for p in source.rglob("*")):
            raise ValueError("multi-file fixture contains a symlink")
        shutil.copytree(source, destination)
        return destination / path.name
    destination.mkdir()
    shutil.copy2(path, destination / "probe.lu")
    for name in ("wolf.pkg", "wolf.sum"):
        candidate = source / name
        if candidate.is_file() and not candidate.is_symlink():
            shutil.copy2(candidate, destination / name)
    text = path.read_bytes().decode("utf-8", errors="replace")
    # Copy referenced module roots, not every unrelated corpus subdirectory.
    for child in sorted(source.iterdir()):
        referenced = re.search(r"\b(?:use|import)\s+" + re.escape(child.name) + r"\b|\b" + re.escape(child.name) + r"\s*(?:\.|::)", text)
        if referenced and child.is_dir() and not child.is_symlink():
            if any(p.is_symlink() for p in child.rglob("*")):
                raise ValueError("module dependency contains a symlink")
            shutil.copytree(child, destination / child.name)
    return destination / "probe.lu"


def build(binary, entry, output, cwd, debug, timeout):
    output.mkdir()
    argv = [str(binary), "build", "--no-cache", "--offline", "--emit=obj"]
    if debug:
        argv.append("-g")
    argv += [str(entry), "-o", str(output / "object.o")]
    code, _, stderr = command(argv, cwd, output, "compile", timeout)
    objects = sorted(p for p in output.glob("*.o") if ".asm-" not in p.name)
    gaps = [line for line in stderr.read_text(errors="replace").splitlines()
            if line.startswith("CFI_GAP ")]
    return code, objects, gaps


def inspect(tool, first, directory, cwd, timeout, second=None):
    argv = [str(tool), str(first)]
    if second is not None:
        argv.append(str(second))
    label = "inspect-" + first.name
    code, out, err = command(argv, cwd, directory, label, timeout)
    if code:
        raise ValueError(f"section inspector failed ({code}): {err}")
    return json.loads(out.read_text())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wolf", type=Path, required=True)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--inspector", type=Path, required=True)
    parser.add_argument("--dwarfdump", type=Path, required=True)
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--start", type=int, default=0)
    parser.add_argument("--count", type=int)
    parser.add_argument("--timeout", type=int, default=120)
    parser.add_argument("--keep-objects", action="store_true")
    parser.add_argument("--all-entries", action="store_true",
                        help="select every .lu entry, not only check: run( ones")
    args = parser.parse_args()
    for name in ("wolf", "baseline", "inspector", "dwarfdump", "corpus", "out"):
        setattr(args, name, getattr(args, name).resolve())
    if args.start < 0 or (args.count is not None and args.count < 1):
        parser.error("start/count must select a nonnegative, nonempty range")
    if args.out.is_relative_to(args.corpus):
        parser.error("output must be outside the input corpus")
    if args.out.exists():
        parser.error("output directory must be new; never overwrite an earlier record")
    args.out.mkdir(parents=True)
    cwd = args.corpus.parent
    candidates = inputs(args.corpus, args.all_entries)
    end = None if args.count is None else args.start + args.count
    chosen = candidates[args.start:end]
    if not chosen:
        raise SystemExit("FAIL dwarf-census-empty-selection")
    manifest = {
        "schema": "wolf-dwarf-census/1", "candidate_count": len(candidates),
        "all_entries": args.all_entries, "compile_cwd": str(cwd),
        "dwarfdump_argv": ["<dwarfdump>", *DUMP_ARGS, "object.o"],
        "start": args.start, "selected_count": len(chosen),
        "tools": {name: {"path": str(getattr(args, name)),
                         "sha256": digest(getattr(args, name))}
                  for name in ("wolf", "baseline", "inspector", "dwarfdump")},
        "cases": [], "paired_baseline_bytes": 0, "paired_head_bytes": 0,
        "paired_head_g_bytes": 0, "measured": 0, "not_measured": 0,
        "failures": 0, "cfi_gap_count": 0,
    }
    record_path = args.out / "census.json"
    for number, path in enumerate(chosen, args.start):
        relative = path.relative_to(args.corpus).as_posix()
        ident = hashlib.sha256(relative.encode()).hexdigest()[:20]
        case_dir = args.out / ident
        case_dir.mkdir()
        row = {"source": relative, "source_sha256": digest(path), "directory": ident}
        manifest["cases"].append(row)
        print(f"START dwarf-census-case {number} {relative}", flush=True)
        outputs = []
        try:
            entry = stage(path, case_dir / "input")
            attempts = {}
            for label, compiler, debug in [
                ("baseline", args.baseline, False), ("head-a", args.wolf, False),
                ("head-b", args.wolf, False), ("head-g-a", args.wolf, True),
                ("head-g-b", args.wolf, True),
            ]:
                code, objects, gaps = build(compiler, entry, case_dir / label, cwd,
                                            debug, args.timeout)
                attempts[label] = {"exit": code, "objects": [p.name for p in objects], "cfi_gaps": gaps}
                outputs += objects
            row["attempts"] = attempts
            head_codes = [attempts[k]["exit"] for k in ("head-a", "head-b", "head-g-a", "head-g-b")]
            if any(head_codes):
                if attempts["baseline"]["exit"] == 0 or len(set(head_codes)) != 1:
                    raise ValueError("head-only refusal, -g regression, or inconsistent compile results")
                row["status"] = "not-measured"
                row["reason"] = "both compilers refuse this staged positive input; see compile logs"
                manifest["not_measured"] += 1
                print(f"NOT-MEASURED dwarf-census {relative}", flush=True)
                continue
            if attempts["baseline"]["exit"] != 0:
                row["status"] = "not-measured"
                row["reason"] = "baseline does not compile this input; no paired size measurement"
                manifest["not_measured"] += 1
                print(f"NOT-MEASURED dwarf-census {relative}", flush=True)
                continue
            summaries = {}
            names = attempts["head-a"]["objects"]
            if not names or any(attempts[k]["objects"] != names for k in attempts):
                raise ValueError("empty or inconsistent object sets")
            for label, other in [("baseline", None), ("head-a", "head-b"), ("head-g-a", "head-g-b")]:
                summaries[label] = {}
                for name in names:
                    obj = case_dir / label / name
                    comparison = case_dir / other / name if other else None
                    summaries[label][name] = inspect(args.inspector, obj, case_dir / label,
                                                      cwd, args.timeout, comparison)
            row["sections"] = summaries
            row["dwarfdump"] = {}
            for name in names:
                obj = case_dir / "head-g-a" / name
                code, out, err = command(dump_argv(args.dwarfdump, obj), obj.parent, case_dir / "head-g-a", "dwarfdump-" + name, args.timeout)
                if code:
                    raise ValueError(f"dwarfdump failed ({code}): {err}")
                text = out.read_text(errors="strict")
                row["dwarfdump"][name] = {"sha256": digest(out), "vocabulary": vocabulary(text)}
            totals = {label: sum(obj["debug_bytes"] for obj in values.values()) for label, values in summaries.items()}
            row["debug_bytes"] = totals
            row["status"] = "measured"
            manifest["measured"] += 1
            manifest["paired_baseline_bytes"] += totals["baseline"]
            manifest["paired_head_bytes"] += totals["head-a"]
            manifest["paired_head_g_bytes"] += totals["head-g-a"]
            manifest["cfi_gap_count"] += len(attempts["head-g-a"]["cfi_gaps"])
            for gap in attempts["head-g-a"]["cfi_gaps"]:
                print(f"KNOWN-GAP cfi-teardown source={relative} {gap}", flush=True)
            print(f"PASS dwarf-census {relative} baseline={totals['baseline']} head={totals['head-a']} head-g={totals['head-g-a']}", flush=True)
        except (OSError, ValueError, json.JSONDecodeError) as error:
            row["status"] = "failed"
            row["error"] = str(error)
            manifest["failures"] += 1
            print(f"FAIL dwarf-census {relative}: {error}", flush=True)
        finally:
            if not args.keep_objects:
                for obj in outputs:
                    obj.unlink(missing_ok=True)
            record_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: v for k, v in manifest.items() if k not in ("cases", "tools")}, sort_keys=True), flush=True)
    if manifest["failures"] or manifest["measured"] == 0:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
