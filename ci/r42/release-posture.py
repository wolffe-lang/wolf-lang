#!/usr/bin/env python3
"""Offline release metadata gate; no compilation or network access."""
import pathlib
import re
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]


class ReleasePosture(unittest.TestCase):
    def test_workspace_and_lock_versions(self):
        self.assertIn('version = "0.2.37"', (ROOT / "Cargo.toml").read_text())
        blocks = (ROOT / "Cargo.lock").read_text().split("[[package]]")
        versions = [re.search(r'^version = "([^"]+)"', b, re.M)[1] for b in blocks
                    if re.search(r'^name = "(?:wolf_[^"]+|xtask)"', b, re.M)]
        self.assertGreater(len(versions), 20)
        self.assertEqual(set(versions), {"0.2.37"})

    def test_published_pairing(self):
        pairing = (ROOT / "crates/wolf_driver/PAIRING").read_text()
        self.assertIn("lupin-version = 0.1.59\n", pairing)
        self.assertIn("lupin-pin = b16353e8ae616ff3a35bd27adc56b727c50df345\n", pairing)

    def test_exact_bundled_std(self):
        self.assertIn("wolf-std-commit = 5b3a59942b2ee69b99682544341ce8a12c984e9e\n",
                      (ROOT / "crates/wolf_driver/STD-PIN").read_text())

    def test_release_note_records_both_unsupported_surfaces(self):
        note = (ROOT / "docs/releases/0.2.37.md").read_text()
        self.assertIn("# wolf 0.2.37", note)
        self.assertIn("0.1.59", note)
        self.assertIn("unsupported", note)
        for issue in ("315", "321"):
            self.assertIn(f"wolf-interp/issues/{issue}", note)
        for name in ("fs_open_at", "fs_stat_at", "fs_lstat_at", "fs_remove_at",
                     "fs_remove_dir_at", "fs_create_dir_at", "fs_symlink_at",
                     "fs_link_at", "fs_read_dir_entries_at", "fs_set_mode_fd"):
            self.assertIn(f"`{name}", note)
        self.assertIn("bit 32", note)

    def test_changelog_carries_previously_omitted_changes(self):
        changelog = (ROOT / "CHANGELOG.md").read_text()
        current = changelog.split("## 0.2.37", 1)[1].split("## 0.2.36", 1)[0]
        for phrase in ("s270", "s271", "E1016", "assert", "fmt", "fix"):
            self.assertIn(phrase, current)

    def test_readme_names_the_candidate_release(self):
        readme = (ROOT / "README.md").read_text()
        self.assertIn("The current release is v0.2.37", readme)
        self.assertIn("docs/releases/0.2.37.md", readme)
        self.assertNotIn("On trunk, long compilations", readme)


if __name__ == "__main__":
    unittest.main(verbosity=2)
