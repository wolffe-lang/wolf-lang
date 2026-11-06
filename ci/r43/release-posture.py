#!/usr/bin/env python3
"""Offline release metadata gate for wolf 0.2.38; no compilation or network."""
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
VERSION = "0.2.38"
LUPIN = "0.1.60"
LUPIN_PIN = "5f1f1c949582f53121e4047bc83c86f23ff72240"
STD = "d4a4d4cab358e1e8c174f0206f9e13ad5cfe6c61"


def crate_versions(lock):
    blocks = (ROOT / lock).read_text().split("[[package]]")
    return [re.search(r'^version = "([^"]+)"', b, re.M)[1] for b in blocks
            if re.search(r'^name = "(?:wolf_[^"]+|xtask)"', b, re.M)]


def release_entry():
    text = (ROOT / "CHANGELOG.md").read_text()
    m = re.search(rf"^## {re.escape(VERSION)} — \d{{4}}-\d{{2}}-\d{{2}}$", text, re.M)
    assert m, f"no `## {VERSION} — <date>` heading"
    rest = text[m.end():]
    return rest[: re.search(r"^## ", rest, re.M).start()]


class ReleasePosture(unittest.TestCase):
    def test_workspace_and_lock_versions(self):
        self.assertIn(f'version = "{VERSION}"', (ROOT / "Cargo.toml").read_text())
        versions = crate_versions("Cargo.lock")
        self.assertGreater(len(versions), 20)
        self.assertEqual(set(versions), {VERSION})

    def test_fuzz_lock_records_the_release(self):
        versions = crate_versions("fuzz/Cargo.lock")
        self.assertGreater(len(versions), 10)
        self.assertEqual(set(versions), {VERSION})

    def test_interface_snapshot_carries_the_toolchain(self):
        snap = (ROOT / "crates/wolf_sema/tests/snapshots/"
                "interface_pretty__two_mod_interfaces.snap").read_text()
        self.assertEqual(snap.count(f"toolchain {VERSION} "), 2)

    def test_published_pairing(self):
        pairing = (ROOT / "crates/wolf_driver/PAIRING").read_text()
        self.assertIn(f"lupin-version = {LUPIN}\n", pairing)
        self.assertIn(f"lupin-pin = {LUPIN_PIN}\n", pairing)

    def test_exact_bundled_std(self):
        self.assertIn(f"wolf-std-commit = {STD}\n",
                      (ROOT / "crates/wolf_driver/STD-PIN").read_text())

    def test_changelog_opens_with_the_release(self):
        text = (ROOT / "CHANGELOG.md").read_text()
        self.assertNotIn("## Unreleased", text)
        self.assertTrue(text.startswith(f"# Changelog\n\n## {VERSION} — "))

    def test_changelog_carries_the_whole_round(self):
        entry = release_entry()
        # s281 and s283 changed no changelog line when they merged.
        for sprint in ("s280", "s281", "s282", "s283", "s284"):
            self.assertIn(sprint, entry)
        for word in ("W0801", "E0806", "#1043", "#1022", ".debug_frame",
                     f"lupin {LUPIN}", STD[:8]):
            self.assertIn(word, entry)

    def test_release_note_names_the_pairing_and_its_limits(self):
        note = (ROOT / f"docs/releases/{VERSION}.md").read_text()
        self.assertIn(f"# wolf {VERSION}", note)
        self.assertIn(LUPIN_PIN, note)
        self.assertIn(STD, note)
        self.assertIn("unsupported", note)
        # lupin's W0801 is a strict subset; the two compiler-only spans.
        self.assertIn("lints/binder_capitalized.lu", note)
        self.assertIn("wolf-interp/issues/344", note)
        # The TLS client's known limit is named, not claimed fixed.
        self.assertIn("wolf-std/issues/133", note)
        # The std ledger still lags the paired lupin.
        self.assertIn("wolf-std/issues/125", note)

    def test_readme_names_the_release(self):
        readme = (ROOT / "README.md").read_text()
        self.assertIn(f"The current release is v{VERSION}", readme)
        self.assertIn(f"docs/releases/{VERSION}.md", readme)
        self.assertIn(f"history of v{VERSION}", readme)


if __name__ == "__main__":
    unittest.main(verbosity=2)
