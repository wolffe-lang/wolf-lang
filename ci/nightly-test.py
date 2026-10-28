#!/usr/bin/env python3
"""Nightly workspace coverage needs the same real link inputs as merge CI."""
import pathlib
import unittest


class Nightly(unittest.TestCase):
    def test_property_sweep_keeps_real_linker_coverage(self):
        text = (pathlib.Path(__file__).resolve().parent.parent /
                ".github/workflows/nightly.yml").read_text()
        job = text.split("  proptest-large:\n", 1)[1].split("  fuzz-long:\n", 1)[0]
        steps = ["ci/fetch-entry-bundle", "cargo xtask dist-rt --target x86_64-unknown-linux-musl",
                 "WOLF_STATIC_ENTRY_MUSL_RT=", "ci/fetch-test-link-bundle",
                 "ci/build-test-ligature", "cargo test --workspace"]
        positions = [job.index(step) for step in steps]
        self.assertEqual(positions, sorted(positions))
        self.assertIn("targets: x86_64-unknown-linux-musl", job)
        self.assertNotIn("--skip", job)

    def test_dispatch_can_select_only_one_job(self):
        text = (pathlib.Path(__file__).resolve().parent.parent /
                ".github/workflows/nightly.yml").read_text()
        self.assertIn("options: [all, proptest-large, fuzz-long, fmt-fuzz-long, bench-full]", text)
        for job in ["proptest-large", "fuzz-long", "fmt-fuzz-long", "bench-full"]:
            block = text.split(f"  {job}:\n", 1)[1].split("    runs-on:", 1)[0]
            self.assertIn(f"inputs.jobs == '{job}'", block)
            self.assertIn("github.event_name == 'schedule'", block)
            self.assertIn("inputs.jobs == 'all'", block)


if __name__ == "__main__":
    unittest.main()
