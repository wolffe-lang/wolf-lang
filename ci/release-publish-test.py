#!/usr/bin/env python3
"""Exercise the actual publish script against a stateful, offline gh API."""
import copy
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
TAG = "v0.2.36"
ASSETS = [f"wolf-0.2.36-{host}.tar.gz" for host in ("mac", "linux", "arm", "windows")]
ASSETS += ["wolf-rt-0.2.36-x86_64-unknown-linux-musl.tar.gz"]
MOCK = r'''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
p = Path(os.environ["MOCK_STATE"])
s = json.loads(p.read_text())
a = sys.argv[1:]
if a[:1] != ["api"]:
    raise SystemExit("only the local API mock is allowed")
method = a[a.index("--method") + 1] if "--method" in a else "GET"
path = next(x for x in a if x.startswith("repos/"))
query = a[a.index("--jq") + 1] if "--jq" in a else ""
fields = {}
for i, arg in enumerate(a):
    if arg in ("-f", "-F"):
        k, v = a[i+1].split("=", 1)
        fields[k] = (v == "true") if k == "draft" else v
s["calls"].append({"method": method, "path": path, "fields": fields})
releases = s["releases"]
if path.endswith("/releases"):
    hits = [r for r in releases if r["tag_name"] == os.environ["GITHUB_REF_NAME"]]
    if "releases=" in query:
        out = "releases=%s drafts=%s assets=%s" % (
            len(hits), sum(r["draft"] for r in hits), len(hits[0]["assets"]) if hits else 0)
    else:
        out = "\n".join("%s\t%s\t%s" % (r["id"], str(r["draft"]).lower(), len(r["assets"])) for r in hits)
else:
    ident = int(path.split("/releases/", 1)[1].split("/")[0])
    r = next((r for r in releases if r["id"] == ident), None)
    if r is None:
        p.write_text(json.dumps(s))
        raise SystemExit("no release with the handed id")
    if method == "PATCH":
        r.update(fields)
        if fields.get("draft") is False and s.get("duplicate_on_publish"):
            releases.append(dict(r, id=456))
        out = "patched owned release"
    elif path.endswith("/assets"):
        out = "\n".join(r["assets"])
    else:
        out = "%s\t%s\t%s" % (r["id"], r["tag_name"], str(r["draft"]).lower())
p.write_text(json.dumps(s))
print(out)
'''


def publish_script():
    text = (ROOT / ".github/workflows/release.yml").read_text()
    block = text.split("- name: publish the release\n", 1)[1].split("        run: |\n", 1)[1]
    lines = []
    for line in block.splitlines():
        if line and not line.startswith("          "):
            break
        lines.append(line[10:])
    return "\n".join(lines)


def release(ident=123, tag=TAG, draft=True, assets=None):
    return dict(id=ident, tag_name=tag, draft=draft,
                assets=copy.deepcopy(ASSETS if assets is None else assets))


class PublishRelease(unittest.TestCase):
    def invoke(self, releases, **options):
        with tempfile.TemporaryDirectory(prefix="r41-publish-") as directory:
            directory = Path(directory)
            state = directory / "state.json"
            state.write_text(json.dumps(dict(releases=releases, calls=[], **options)))
            gh = directory / "gh"
            gh.write_text(MOCK)
            gh.chmod(0o755)
            got = subprocess.run(
                ["bash", "-euo", "pipefail", "-c", publish_script()], cwd=directory,
                env=dict(os.environ, PATH=f'{directory}:{os.environ["PATH"]}',
                         MOCK_STATE=str(state), GH_REPO="example/repo", GH_TOKEN="offline-test",
                         RELEASE_ID="123", GITHUB_REF_NAME=TAG),
                capture_output=True, text=True)
            return got, json.loads(state.read_text())

    def assert_published_by_owned_id_and_tag(self, state):
        patches = [c for c in state["calls"] if c["fields"].get("draft") is False]
        self.assertEqual(len(patches), 1)
        self.assertEqual(patches[0]["path"], "repos/example/repo/releases/123")
        self.assertEqual(patches[0]["fields"].get("tag_name"), TAG)

    def test_the_owned_tagged_draft_publishes_atomically(self):
        got, state = self.invoke([release()])
        self.assertEqual(got.returncode, 0, got.stderr)
        self.assert_published_by_owned_id_and_tag(state)

    def test_an_untagged_owned_draft_is_repaired_before_the_one_release_guard(self):
        got, state = self.invoke([release(tag="untagged-fa9aa41467fdb63647c6")])
        self.assertEqual(got.returncode, 0, got.stderr)
        self.assert_published_by_owned_id_and_tag(state)
        self.assertEqual(state["releases"][0]["tag_name"], TAG)
        self.assertFalse(state["releases"][0]["draft"])

    def test_a_successful_publish_can_be_retried_without_redrafting(self):
        got, state = self.invoke([release(draft=False)])
        self.assertEqual(got.returncode, 0, got.stderr)
        self.assertFalse(any(c["fields"].get("draft") is True for c in state["calls"]))
        self.assert_published_by_owned_id_and_tag(state)

    def test_two_releases_on_the_tag_remain_a_hard_refusal(self):
        got, state = self.invoke([release(), release(456)])
        self.assertNotEqual(got.returncode, 0)
        self.assertIn("wanted exactly one (#226)", got.stderr)
        self.assertFalse(any(c["fields"].get("draft") is False for c in state["calls"]))

    def test_another_release_on_the_tag_cannot_be_published_instead(self):
        got, state = self.invoke([release(tag="untagged-owned"), release(456)])
        self.assertNotEqual(got.returncode, 0)
        self.assertFalse(any(c["fields"].get("draft") is False for c in state["calls"]))

    def test_a_missing_handed_id_is_not_replaced_by_a_tag_search(self):
        got, state = self.invoke([release(456)])
        self.assertNotEqual(got.returncode, 0)
        self.assertFalse(any(c["method"] == "PATCH" for c in state["calls"]))

    def test_a_published_different_tag_is_never_retagged(self):
        got, state = self.invoke([release(tag="v0.2.35", draft=False)])
        self.assertNotEqual(got.returncode, 0)
        self.assertFalse(any(c["method"] == "PATCH" for c in state["calls"]))
        self.assertEqual(state["releases"][0]["tag_name"], "v0.2.35")

    def test_missing_musl_keeps_the_owned_release_a_draft(self):
        got, state = self.invoke([release(assets=ASSETS[:-1])])
        self.assertNotEqual(got.returncode, 0)
        self.assertTrue(state["releases"][0]["draft"])
        self.assertFalse(any(c["fields"].get("draft") is False for c in state["calls"]))

    def test_five_host_archives_cannot_substitute_for_the_musl_runtime(self):
        got, state = self.invoke([release(assets=ASSETS[:-1] + ["wolf-extra.tar.gz"])])
        self.assertNotEqual(got.returncode, 0)
        self.assertTrue(state["releases"][0]["draft"])

    def test_the_post_publish_one_release_guard_still_fails_a_bad_shape(self):
        got, _ = self.invoke([release()], duplicate_on_publish=True)
        self.assertNotEqual(got.returncode, 0)
        self.assertIn("one-release, zero-draft shape", got.stdout + got.stderr)

    def test_the_guard_is_wired_into_the_small_ci_job(self):
        self.assertIn("python3 ci/release-publish-test.py",
                      (ROOT / ".github/workflows/ci.yml").read_text())


if __name__ == "__main__":
    unittest.main()
