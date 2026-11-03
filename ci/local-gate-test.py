#!/usr/bin/env python3
"""Local-gate wiring proofs (stdlib only).

Prediction: unlabelled PRs execute one cheap plan/legs job; ci/full-matrix
PRs retain the original full tier. Module-only pushes trigger nothing,
shared-path pushes retain the original full tier. Nightly/release work is
byte-for-byte preserved apart from event gates that are true for those events.
"""
import fnmatch
import json
import re
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
BASE = json.loads((ROOT / "ci/local-gate-baseline.json").read_text())
GATE = "github.event_name != 'pull_request' || contains(github.event.pull_request.labels.*.name, 'full-matrix') || contains(github.event.pull_request.labels.*.name, 'ci') || contains(github.event.pull_request.labels.*.name, 'release-matrix')"
EVENT = "github.event_name == 'pull_request' && !contains(github.event.pull_request.labels.*.name, 'release-matrix') && 'push' || github.event_name"


def blocks(text):
    jobs = text.split("\njobs:\n", 1)[1]
    return dict(re.findall(r"^  ([A-Za-z0-9_-]+):\n(.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)", jobs, re.M | re.S))


def expression(expr, event, labels=(), cancelled=False):
    expr = expr.strip().removeprefix("${{").removesuffix("}}").strip()
    expr = re.sub(r"contains\(github.event.pull_request.labels\.\*\.name, '([^']+)'\)",
                  lambda m: repr(m[1] in labels), expr)
    expr = expr.replace("github.event_name", repr(event)).replace("!cancelled()", repr(not cancelled))
    expr = expr.replace("&&", " and ").replace("||", " or ")
    expr = re.sub(r"!(?!=)", " not ", expr)
    return eval(expr, {"__builtins__": {}}, {})


def on_block(text, event):
    header = text.split("\non:\n", 1)[1].split("\njobs:\n", 1)[0]
    m = re.search(r"^  " + event + r":[^\n]*\n(.*?)(?=^  [A-Za-z_]+:|^[A-Za-z_]+:|\Z)", header, re.M | re.S)
    return m.group(1) if m else None


def patterns(text):
    push = on_block(text, "push")
    m = re.search(r"^    paths:\n((?:      - .*\n)+)", push or "", re.M)
    return [line.strip()[2:].strip("'\"") for line in m[1].splitlines()] if m else None


def matches(path, globs):
    if globs is None:
        return True
    chosen = False
    for pattern in globs:
        negate = pattern.startswith("!")
        p = pattern.lstrip("!")
        if fnmatch.fnmatchcase(path, p) or (p.startswith("**/") and fnmatch.fnmatchcase(path, p[3:])):
            chosen = not negate
    return chosen


class Gate(unittest.TestCase):
    def main(self):
        return (ROOT / ".github/workflows/ci.yml").read_text()

    def test_unlabelled_is_cheap_only(self):
        text = self.main()
        b = blocks(text)
        root = BASE["root"]
        cheap = f"      - name: local evidence gate\n        if: ${{{{ !({GATE}) }}}}\n        run: echo 'Local gauntlet evidence is the gate; request ci or full-matrix for CI.'\n"
        self.assertIn(cheap, b[root])
        self.assertTrue(expression("!(" + GATE + ")", "pull_request"))
        for job, body in b.items():
            if job in (root, "nightly-red"):
                continue
            self.assertIn("    if: ${{ (" + GATE + ")", body, job)
            self.assertFalse(expression(GATE, "pull_request"), job)
        original_steps = len(re.findall(r"^      - ", BASE["workflows"]["ci.yml"][root], re.M))
        self.assertEqual(b[root].count("        if: ${{ " + GATE + " }}"), original_steps + 1)

    def test_labelled_means_full_set(self):
        text = self.main()
        root = blocks(text)[BASE["root"]]
        for label in ("ci", "full-matrix"):
            self.assertTrue(expression(GATE, "pull_request", [label]))
            if BASE["planner"]:
                expected = "          EVENT: ${{ " + EVENT + " }}"
                self.assertIn(expected, root)
                event = expression(EVENT, "pull_request", [label])
                self.assertEqual(event, "push")
                result = subprocess.run(BASE["planner"] + ["--event", event, "--json"], cwd=ROOT,
                                        capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                actual = json.loads(result.stdout)
                for key in BASE["full"]:
                    self.assertEqual(actual[key], BASE["full"][key], (label, key))
            for job, body in blocks(text).items():
                if job not in (BASE["root"], "nightly-red"):
                    self.assertIn("    if: ${{ (" + GATE + ")", body, job)

    def test_off_cross_cutting_push_runs_nothing(self):
        globs = patterns(self.main())
        self.assertIsNotNone(globs, "push must filter cross-cutting paths")
        self.assertFalse(matches(BASE["module_path"], globs))
        self.assertFalse(matches("notes/example.md", globs))
        self.assertFalse(matches("README.md", globs))

    def test_cross_cutting_push_runs_full_set(self):
        globs = patterns(self.main())
        self.assertIsNotNone(globs)
        for path in BASE["cross_paths"]:
            self.assertTrue(matches(path, globs), path)
        self.assertTrue(expression(GATE, "push"))
        if BASE["planner"]:
            root = blocks(self.main())[BASE["root"]]
            self.assertIn("EVENT: ${{ " + EVENT + " }}", root)
            result = subprocess.run(BASE["planner"] + ["--event", "push", "--json"], cwd=ROOT,
                                    capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            for key, value in BASE["full"].items():
                self.assertEqual(json.loads(result.stdout)[key], value)

    def test_nightly_release_and_commands_preserved(self):
        for name, old_jobs in BASE["workflows"].items():
            text = (ROOT / ".github/workflows" / name).read_text()
            actual = blocks(text)
            self.assertEqual(set(actual), set(old_jobs), name)
            self.assertIn("  workflow_dispatch:", text)
            if BASE["schedules"].get(name):
                self.assertIn(BASE["schedules"][name], text)
            if name in BASE["pr_workflows"]:
                self.assertIsNotNone(on_block(text, "schedule"), name)
            for job, old in old_jobs.items():
                new = actual[job]
                # Remove only the new, explicitly-tested job and step gate lines.
                new = re.sub(r"^    if: \$\{\{ \(" + re.escape(GATE) + r"\)(?: && \((.*)\))? \}\}\n",
                             lambda m: "    if: " + ("${{ " + m[1] + " }}" if job == "tier" else m[1]) + "\n" if m[1] else "", new, flags=re.M)
                new = new.replace("        if: ${{ " + GATE + " }}\n", "")
                new = new.replace("          EVENT: ${{ " + EVENT + " }}", "          EVENT: ${{ github.event_name }}")
                cheap = f"      - name: local evidence gate\n        if: ${{{{ !({GATE}) }}}}\n        run: echo 'Local gauntlet evidence is the gate; request ci or full-matrix for CI.'\n"
                new = new.replace(cheap, "")
                # New wiring proof steps add coverage, not replacement commands.
                new = new.replace("      - name: local gate wiring proof\n        run: python3 -I ci/local-gate-test.py\n", "")
                self.assertEqual(new, old, (name, job))
        for event in ("schedule", "workflow_dispatch", "push"):
            self.assertTrue(expression(GATE, event))
        self.assertTrue(expression(GATE, "pull_request", ["release-matrix"]))
        self.assertEqual(expression(EVENT, "pull_request", ["release-matrix"]), "pull_request")


if __name__ == "__main__":
    unittest.main(verbosity=2)
