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

    def test_all_entries_selects_fixture_sources_without_directives(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            for row in ["dwarf-v1-enum", "dwarf-v1-struct"]:
                (root / row).mkdir()
                (root / row / "probe.lu").write_text("fn main() -> int { 0 }\n")
            (root / "alias.lu").symlink_to(root / "dwarf-v1-enum" / "probe.lu")
            self.assertEqual(census.inputs(root), [])
            self.assertEqual([p.parent.name for p in census.inputs(root, True)],
                             ["dwarf-v1-enum", "dwarf-v1-struct"])

    def test_dump_names_the_object_without_a_host_path(self):
        obj = Path("/some/host/case/head-g-a/object.o")
        argv = census.dump_argv(Path("/usr/bin/llvm-dwarfdump"), obj)
        self.assertEqual(argv[-1], "object.o")
        self.assertNotIn("/some/host", " ".join(argv[1:]))

    def test_vocabulary_omits_the_line_header_opcode_legend(self):
        text = ("standard_opcode_lengths[DW_LNS_set_isa] = 1\n"
                "0x00000099: 0a DW_LNS_set_prologue_end\n"
                "  DW_CFA_def_cfa: W29 +16\n")
        self.assertEqual(census.vocabulary(text), ["DW_CFA_def_cfa", "DW_LNS_set_prologue_end"])

    def test_a_cfi_gap_fails_the_census(self):
        clean = {"failures": 0, "cfi_gap_count": 0, "measured": 3}
        self.assertFalse(census.failed(clean))
        self.assertTrue(census.failed({**clean, "cfi_gap_count": 1}))
        self.assertTrue(census.failed({**clean, "failures": 1}))
        self.assertTrue(census.failed({**clean, "measured": 0}))

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
