#!/usr/bin/env python3
"""Small staging/selection checks; TMPDIR is supplied by the locked runner."""
import importlib.util
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("census", Path(__file__).with_name("dwarf-census.py"))
census = importlib.util.module_from_spec(spec)
spec.loader.exec_module(census)


class CensusInputs(unittest.TestCase):
    def test_only_positive_entries_and_no_symlink_following(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            (root / "good.lu").write_text("//! check: run(exit=0)\nfn main() -> int { 0 }\n")
            (root / "bad.lu").write_bytes(b"//! check: error\n\xff")
            (root / "alias.lu").symlink_to(root / "good.lu")
            self.assertEqual([p.name for p in census.inputs(root)], ["good.lu"])

    def test_standalone_staging_does_not_copy_unrelated_corpus_directories(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            source = root / "corpus"
            source.mkdir()
            (source / "entry.lu").write_text("use helper\nfn main() -> int { helper.f() }\n")
            for name in ["helper", "unrelated"]:
                (source / name).mkdir()
                (source / name / "part.lu").write_text("fn f() -> int { 0 }\n")
            staged = census.stage(source / "entry.lu", root / "input")
            self.assertEqual(staged.name, "probe.lu")
            self.assertTrue((staged.parent / "helper/part.lu").exists())
            self.assertFalse((staged.parent / "unrelated").exists())

    def test_multi_file_symlink_is_not_silently_reinterpreted(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            source = root / "corpus"
            source.mkdir()
            (source / "main.lu").write_text("fn main() -> int { 0 }\n")
            (source / "escape").symlink_to(root)
            with self.assertRaises(ValueError):
                census.stage(source / "main.lu", root / "input")


if __name__ == "__main__":
    unittest.main()
