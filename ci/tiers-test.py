#!/usr/bin/env python3
"""ci/tiers-test.py — the planner's own proof (tl21; tl30's release tier and
the nightly's issue). python3, git and bash only.

    ci/tiers-test.py

  T1  the tiers by event: the nightly, a dispatch with tier release and
      the `release-matrix` label are release, every job; a trunk push, a
      dispatch with no input or tier full, and the `full-matrix` label are
      full, every job but the native macOS gauntlet (tl30); another label's
      `labeled` event is a noop, and so is `full-matrix` added beside
      `release-matrix`; a dispatch that names jobs is partial, and refuses
      a job that is none of the workflow's or whose needs it leaves out, a
      tier no dispatch can ask for, and jobs and a tier together.
  T2  the pull-request tiers by path: code is pr (the linux gauntlet, the
      fuzz smoke, this test); prose alone is noop; prose beside code is
      pr. What a gate reads is never prose: everything under spec/
      (anchors.json and prelude.json too), README.md, and the generated
      docs/diagnostics.md, docs/warnings.md and docs/api/.
  T3  full wins: a path no rule names, alone or beside anything; changed
      paths that cannot be read; a change that names no path.
  T4  the wiring of .github/workflows/ci.yml: every job needs the plan and
      runs only when the plan names it; every plan output is read with a
      default; the `tier` job needs every job, runs on `!cancelled()` and
      checks with `ci/tiers.py verdict`; cancel-in-progress is for pull
      requests only and every other run has a concurrency group of its own
      run id, each label's run its own; the trunk push, the nightly, both
      labels and the dispatch's `jobs` and `tier` inputs are there; the
      macOS, windows and freebsd jobs are no pull-request job; the native
      macOS gauntlet is the release tier's alone, and the merge keeps
      `build + test` on macOS (the smoke); a red nightly, and only a red
      nightly, runs ci/nightly-red.sh.
  T5  the tripwire: the tracked paths the planner calls prose that a line
      of crates/, xtask/ or fuzz/ reading a file names are exactly the
      one ruled so (CHANGELOG.md: one xtask unit test pins its 0.2.2
      entry). The search is shown to fire on the three it must find.
      (A WARN, not a failure: a tracked path no rule names; its pull
      request runs full.)
  T6  the verdict: green for a release, a full, a pr, a noop and a
      partial run; red for a chosen job failed, skipped or cancelled, a job
      that ran unasked, a failed plan, a plan with no tier, a tier whose
      jobs are not that tier's (a `full` that chose less, or the native
      gauntlet; a `release` that chose less), and green when GitHub drops an
      output that would have been empty.
  T7  the change read from git: a real merge commit against its first
      parent, with the base moved on meanwhile; red-to-full when HEAD is no
      merge or its second parent is not the pull request's head.
  T8  ci/nightly-red.sh against a stand-in `gh`: no open issue opens one,
      labelled, under the one title; an open one gets a comment and no new
      issue; a label that already exists is no failure.
  T11 a nightly at a commit already green in full: told by ci/covered.py
      that the commit's latest `tier release` is green, the nightly plans
      `tier skipped`, no job, and says so in plain words in its log, a
      warning and the run's summary, naming the covering run and when a
      full night is due (S1); told anything else, it plans release (S2);
      no other event is ever skipped, and no dispatch may ask for it (S3);
      the verdict of a skipped night wants every job skipped and says
      nothing was tested (S4). ci/covered.py, against a fake `gh`, covers
      only a green `tier release` check run of GitHub Actions, of this
      workflow, at this commit, of a run other than tonight's, the latest
      to complete, at most 144 hours before tonight's run; never another
      tier's, an older one, a red, cancelled or running one, another
      app's, a full page, an unanswered question or another event (S5).
      The plan job asks it before it plans, with read-only permissions,
      and only the plan step reads the answer (S6). The nightly's issue
      says a `tier skipped` night is not a green one (S7).
  T12 every leg of `build + test` has a limit of its own, read from the
      plan's matrix, at least its longest healthy run in ci/job-limits.json plus
      a third, rounded up to the next 5 minutes; a leg with no record, or a
      record with no leg, fails too (#1217). The check is shown to say no
      to a limit under that bound and to pass one at it.
  T13 a trunk push's macOS leg already green at its commit, against a
      stand-in API and a scratch git repository: a completed, successful pull
      request or dispatch run at the commit, whose plan tested the same tree
      and has `build + test (macos-latest)` once, green, on macOS, lets the
      push leave it, and the plan names the run; anything less runs it:
      another event, another tree, no tree, a missing, duplicate, cancelled
      or Linux job, a short listing, an error, a timeout, a redirect, no
      token, another commit. Only the runner's own push to trunk asks (a
      labelled pull request's run hands its plan EVENT=push). The verdict
      accepts exactly the covered leg and names its run; the wiring gives the
      plan its tree step and the token.

Exit 0 when every check passes, 1 otherwise.
"""
import json
import math
import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PLAN = os.path.join(ROOT, "ci", "tiers.py")
WORKFLOW = os.path.join(ROOT, ".github", "workflows", "ci.yml")
RED = os.path.join(ROOT, "ci", "nightly-red.sh")
ENV_KEYS = ("EVENT", "ACTION", "LABEL", "LABELS", "HEAD_SHA", "IN_JOBS", "IN_TIER", "NEEDS",
            "COVERED", "COVERED_BY", "COVERED_AT", "COVERED_RUN", "GITHUB_STEP_SUMMARY")
fails = 0


def say(ok, what):
    global fails
    print("tiers-test: %s %s" % ("PASS" if ok else "FAIL", what))
    fails += not ok


def clean_env(**more):
    env = {k: v for k, v in os.environ.items() if k not in ENV_KEYS}
    env.update(more)
    return env


def plan(paths=None, **kw):
    args = [sys.executable, PLAN, "plan", "--json"]
    flags = kw.pop("flags", [])
    for k, v in kw.items():
        args += ["--" + k.replace("_", "-"), v]
    args += flags
    if paths is not None:
        args += ["--paths-from", "-"]
    r = subprocess.run(args, input="\n".join(paths or []) + "\n", capture_output=True, text=True, env=clean_env())
    if r.returncode != 0:
        return {"tier": "ERROR rc=%d %s" % (r.returncode, r.stderr.strip()[-200:]), "jobs": []}
    return json.loads(r.stdout)


wf = open(WORKFLOW, encoding="utf-8").read()
JOBS = re.findall(r"^  ([A-Za-z0-9_-]+):\s*$", wf[wf.index("\njobs:\n"):], re.M)
EVERY = [j for j in JOBS if j not in ("plan", "tier", "nightly-red")]
PR_JOBS = ["plan-test", "lint-test", "fuzz-smoke"]
# tl30, the macOS cap: the merge's jobs are every job but the native macOS
# gauntlet, which the release tier (the nightly, a release lane) runs.
MERGE = [j for j in EVERY if j != "native-macos"]


def expect(name, got, tier, jobs):
    why = []
    if got["tier"] != tier:
        why.append("tier %s, wanted %s" % (got["tier"], tier))
    if got["jobs"] != jobs:
        why.append("jobs %s, wanted %s" % (" ".join(got["jobs"]) or "none", " ".join(jobs) or "none"))
    shown = "every job (%d)" % len(EVERY) if jobs == EVERY else (
        "the merge's jobs (%d, the native macOS gauntlet left out)" % len(MERGE) if jobs == MERGE else (" ".join(jobs) or "no job"))
    say(not why, "%s: %s%s" % (name, tier, (" (" + "; ".join(why) + ")") if why else ", " + shown))


# --- T1 the tiers by event ---------------------------------------------------
say(EVERY == ["plan-test", "tool-details", "lint-test", "variadic-abi", "static-entry", "native-macos", "fuzz-smoke", "build-test", "freebsd-check"] and JOBS[-1] == "nightly-red",
    "T1 the workflow's jobs between plan and tier: %s" % " ".join(EVERY))
expect("T1 a push to trunk", plan(event="push"), "full", MERGE)
expect("T1 the nightly", plan(event="schedule"), "release", EVERY)
expect("T1 a dispatch, no input", plan(event="workflow_dispatch"), "full", MERGE)
expect("T1 a dispatch, tier full", plan(event="workflow_dispatch", tier="full"), "full", MERGE)
expect("T1 a dispatch, tier release", plan(event="workflow_dispatch", tier="release"), "release", EVERY)
say(plan(event="workflow_dispatch", tier="pr")["tier"].startswith("ERROR rc=2"), "T1 a dispatch's tier is full or release, nothing else")
say(plan(event="workflow_dispatch", tier="release", jobs="plan-test")["tier"].startswith("ERROR rc=2"),
    "T1 a dispatch names jobs or a tier, not both")
expect("T1 a dispatch naming the planner's test", plan(event="workflow_dispatch", jobs="plan-test"), "partial", ["plan-test"])
expect("T1 a dispatch naming two jobs, in the workflow's order",
       plan(event="workflow_dispatch", jobs="freebsd-check plan-test"), "partial", ["plan-test", "freebsd-check"])
expect("T1 a dispatch naming every job is still partial",
       plan(event="workflow_dispatch", jobs=" ".join(EVERY)), "partial", EVERY)
say(plan(event="workflow_dispatch", jobs="nightly-red")["tier"].startswith("ERROR rc=2"), "T1 a dispatch cannot name the nightly's issue")
say(plan(event="workflow_dispatch", jobs="lint")["tier"].startswith("ERROR rc=2"), "T1 a dispatch naming no job of the workflow is refused")
expect("T1 a pull request labelled full-matrix, prose only",
       plan(["CHANGELOG.md"], event="pull_request", action="synchronize", labels="bug,full-matrix"), "full", MERGE)
expect("T1 the full-matrix label added",
       plan(["CHANGELOG.md"], event="pull_request", action="labeled", label="full-matrix", labels="full-matrix"), "full", MERGE)
expect("T1 the release-matrix label added",
       plan(["crates/wolf_sema/src/lib.rs"], event="pull_request", action="labeled", label="release-matrix", labels="release-matrix"), "release", EVERY)
expect("T1 a pull request labelled release-matrix, pushed",
       plan(["crates/wolf_sema/src/lib.rs"], event="pull_request", action="synchronize", labels="release-matrix"), "release", EVERY)
expect("T1 both labels: release",
       plan(["crates/wolf_sema/src/lib.rs"], event="pull_request", action="synchronize", labels="full-matrix,release-matrix"), "release", EVERY)
expect("T1 full-matrix added beside release-matrix (whose run covers it)",
       plan(["crates/wolf_sema/src/lib.rs"], event="pull_request", action="labeled", label="full-matrix", labels="release-matrix,full-matrix"), "noop", [])
expect("T1 release-matrix added beside full-matrix",
       plan(["crates/wolf_sema/src/lib.rs"], event="pull_request", action="labeled", label="release-matrix", labels="full-matrix,release-matrix"), "release", EVERY)
expect("T1 another label added", plan(["crates/wolf_sema/src/lib.rs"], event="pull_request", action="labeled", label="bug", labels="bug"), "noop", [])
expect("T1 another label added to a labelled pull request",
       plan(["crates/wolf_sema/src/lib.rs"], event="pull_request", action="labeled", label="bug", labels="bug,full-matrix"), "noop", [])
expect("T1 the jobs input means nothing to a pull request",
       plan(["crates/wolf_sema/src/lib.rs"], event="pull_request", action="synchronize", jobs="plan-test"), "pr", PR_JOBS)

# --- T2 the pull-request tiers by path ------------------------------------------
pr = dict(event="pull_request", action="synchronize")
for p in ["crates/wolf_sema/src/lib.rs", "crates/wolf_rt/src/net.rs", "crates/wolf_driver/PAIRING", "crates/wolf_wir/wprof-format.md",
          "corpus/hello.lu", "xtask/src/main.rs", "fuzz/Cargo.toml", "bench/kernels/k.lu", "bench/bench-exceptions.md",
          "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "rustfmt.toml", "clippy.toml", "deny.toml", "LICENSE",
          ".github/workflows/ci.yml", ".github/workflows/release.yml", ".github/workflows/nightly.yml",
          ".cargo/config.toml", "ci/tiers.py", "ci/tiers-test.py", "assets/wolf-logo.svg"]:
    expect("T2 %s" % p, plan([p], **pr), "pr", PR_JOBS)
for p in ["spec/anchors.json", "spec/prelude.json", "spec/02-memory-model.md", "spec/11-os.md", "spec/README.md",
          "spec/grammar.ebnf", "spec/new/clause.md", "README.md", "docs/diagnostics.md", "docs/warnings.md",
          "docs/api/index.html", "docs/api/index.json", "docs/api/notes.md", "docs/figure.png", "docs/new/page.md",
          "crates/wolf_diag/VOICE.md"]:
    expect("T2 not prose (a gate reads it, or it is no document): %s" % p, plan([p], **pr), "pr", PR_JOBS)
for p in ["CHANGELOG.md", "CONTRIBUTING.md", "docs/backlog.md", "docs/platforms.md", "docs/bench-runner.md",
          "docs/prelude-json.md", "docs/release/NOTES-v0.1.0.md", "docs/a-new-page.md"]:
    expect("T2 prose: %s" % p, plan([p], **pr), "noop", [])
expect("T2 prose alone, several", plan(["CHANGELOG.md", "CONTRIBUTING.md", "docs/platforms.md"], **pr), "noop", [])
expect("T2 prose beside code", plan(["CHANGELOG.md", "crates/wolf_sema/src/lib.rs", "docs/platforms.md"], **pr), "pr", PR_JOBS)
expect("T2 the CHANGELOG beside the spec", plan(["CHANGELOG.md", "spec/anchors.json"], **pr), "pr", PR_JOBS)
expect("T2 prose beside a generated page", plan(["docs/platforms.md", "docs/diagnostics.md"], **pr), "pr", PR_JOBS)

# --- T3 full wins ---------------------------------------------------------------
expect("T3 a path no rule names", plan(["newplace/thing.rs"], **pr), "full", MERGE)
expect("T3 a new file at the root", plan(["build.rs"], **pr), "full", MERGE)
expect("T3 a markdown file at the root no rule names", plan(["NOTES.md"], **pr), "full", MERGE)
expect("T3 an unknown path beside prose", plan(["CHANGELOG.md", "newplace/notes.md"], **pr), "full", MERGE)
expect("T3 an unknown path beside code", plan(["crates/wolf_sema/src/lib.rs", "newplace/thing.rs"], **pr), "full", MERGE)
expect("T3 no changed paths to read", plan(**pr), "full", MERGE)
expect("T3 a change that names no path", plan([], **pr), "full", MERGE)
expect("T3 a merge that cannot be read (no repository there)",
       plan(repo=os.path.join(ROOT, "no", "such", "dir"), flags=["--merge"], **pr), "full", MERGE)

# --- T6 the verdict -------------------------------------------------------------
def verdict(needs):
    r = subprocess.run([sys.executable, PLAN, "verdict"], capture_output=True, text=True, env=clean_env(NEEDS=json.dumps(needs)))
    return r.returncode, r.stdout + r.stderr


def run_of(tier, chosen, results=None, drop=()):
    outs = {"tier": tier, "jobs": json.dumps(chosen)}
    for k in drop:  # GitHub leaves an empty output out of `needs`
        del outs[k]
    n = {"plan": {"result": "success", "outputs": outs}}
    for j in EVERY:
        n[j] = {"result": "success" if j in chosen else "skipped", "outputs": {}}
    n.update(results or {})
    return n


def vexpect(name, needs, want):
    rc, text = verdict(needs)
    say(rc == want, "T6 %s: exit %d%s" % (name, rc, "" if rc == want else " (wanted %d) %s" % (want, text.strip()[-300:])))


failed = {"result": "failure", "outputs": {}}
vexpect("a green full run", run_of("full", MERGE), 0)
vexpect("a green release run", run_of("release", EVERY), 0)
vexpect("a full run that ran the native macOS gauntlet (the merge before tl30)", run_of("full", EVERY), 1)
vexpect("a release run without the native macOS gauntlet", run_of("release", MERGE), 1)
vexpect("a green pr run", run_of("pr", PR_JOBS), 0)
vexpect("a noop run", run_of("noop", []), 0)
vexpect("a noop run whose jobs output is absent", run_of("noop", [], drop=["jobs"]), 0)
vexpect("a partial run of the planner's test", run_of("partial", ["plan-test"]), 0)
vexpect("a chosen job failed", run_of("release", EVERY, {"native-macos": failed}), 1)
vexpect("the merge's macOS smoke failed", run_of("full", MERGE, {"build-test": failed}), 1)
vexpect("the planner's test failed", run_of("pr", PR_JOBS, {"plan-test": failed}), 1)
vexpect("a chosen job was skipped", run_of("pr", PR_JOBS, {"fuzz-smoke": {"result": "skipped", "outputs": {}}}), 1)
vexpect("a chosen job was cancelled", run_of("pr", PR_JOBS, {"lint-test": {"result": "cancelled", "outputs": {}}}), 1)
vexpect("a job ran unasked", run_of("pr", PR_JOBS, {"build-test": {"result": "success", "outputs": {}}}), 1)
vexpect("a job ran in a noop run", run_of("noop", [], {"freebsd-check": {"result": "success", "outputs": {}}}), 1)
vexpect("a full run that chose less than every job", run_of("full", PR_JOBS), 1)
vexpect("a pr run that chose every job", run_of("pr", EVERY), 1)
vexpect("a full run whose jobs output is absent", run_of("full", MERGE, drop=["jobs"]), 1)
vexpect("a pr run that chose one job", run_of("pr", ["plan-test"]), 1)
vexpect("a partial run that chose nothing", run_of("partial", []), 1)
vexpect("the plan failed", {"plan": {"result": "failure", "outputs": {}}, "lint-test": {"result": "skipped", "outputs": {}}}, 1)
vexpect("the plan named no tier", {"plan": {"result": "success", "outputs": {"jobs": "[]"}}, "lint-test": {"result": "skipped", "outputs": {}}}, 1)
vexpect("the plan named a tier that is none", run_of("fast", []), 1)
vexpect("the plan's jobs are no list", {"plan": {"result": "success", "outputs": {"tier": "pr", "jobs": "plan-test"}}}, 1)
vexpect("a chosen job the tier job does not need", run_of("partial", ["plan-test", "lint"]), 1)

# --- T4 the wiring --------------------------------------------------------------
blocks = {}
for j in JOBS:
    m = re.search(r"^  %s:\s*\n(.*?)(?=^  [A-Za-z0-9_-]+:\s*$|\Z)" % re.escape(j), wf, re.M | re.S)
    blocks[j] = m.group(1) if m else ""
say(JOBS[:1] == ["plan"] and JOBS[-2:] == ["tier", "nightly-red"], "T4 the plan is the first job, the tier the last but the nightly's issue")
for j in EVERY:
    b = blocks[j]
    needs = re.search(r"^    needs: (.*)$", b, re.M)
    ok = needs is not None and "plan" in re.findall(r"[A-Za-z0-9_-]+", needs.group(1))
    ok = ok and ("    if: ${{ (github.event_name != 'pull_request' || contains(github.event.pull_request.labels.*.name, 'full-matrix') || contains(github.event.pull_request.labels.*.name, 'ci') || contains(github.event.pull_request.labels.*.name, 'release-matrix')) && (contains(fromJSON(needs.plan.outputs.jobs || '[]'), '%s')) }}\n" % j) in b
    say(ok, "T4 %s needs the plan and runs only when the plan names it" % j)
reads = re.findall(r"needs\.plan\.outputs\.([a-z_]+)(\s*\|\|)?", wf)
say(len(reads) >= 7 and all(d for _, d in reads), "T4 every plan output is read with a default (%d reads: %s)" % (
    len(reads), " ".join(sorted(set(k for k, _ in reads)))))
say(all(("      %s: ${{ steps.plan.outputs.%s }}\n" % (k, k)) in blocks["plan"] for k in ("tier", "jobs")),
    "T4 the plan job declares tier and jobs")
say('        run: python3 ci/tiers.py plan --merge --github-output "$GITHUB_OUTPUT"\n' in blocks["plan"]
    and "          fetch-depth: 2\n" in blocks["plan"] and "HEAD_SHA: ${{ github.event.pull_request.head.sha }}" in blocks["plan"],
    "T4 the plan reads the merge commit (depth 2) and checks its second parent")
say("      - run: python3 ci/tiers-test.py\n" in blocks["plan-test"], "T4 the plan-test job runs this file")
t = blocks["tier"]
tn = re.findall(r"^      - ([A-Za-z0-9_-]+)\s*$", t, re.M)
say(sorted(tn) == sorted(["plan"] + EVERY), "T4 the tier job needs every job (%d of %d)" % (len(tn), len(EVERY) + 1))
ifs = re.findall(r"^    if: (.*)$", t, re.M)
say(ifs == ["${{ (github.event_name != 'pull_request' || contains(github.event.pull_request.labels.*.name, 'full-matrix') || contains(github.event.pull_request.labels.*.name, 'ci') || contains(github.event.pull_request.labels.*.name, 'release-matrix')) && (!cancelled()) }}"], "T4 the opted-in tier job runs on !cancelled(), never always() (%s)" % "; ".join(ifs))
say("    name: tier ${{ needs.plan.outputs.tier || 'unplanned' }}\n" in t, "T4 the tier job is named for the tier")
say("        run: python3 ci/tiers.py verdict\n" in t and "          NEEDS: ${{ toJSON(needs) }}\n" in t and "actions/checkout" in t,
    "T4 the tier job's check is ci/tiers.py verdict over toJSON(needs)")
say("  cancel-in-progress: ${{ github.event_name == 'pull_request' }}\n" in wf, "T4 cancel-in-progress for pull requests only")
group = re.search(r"^concurrency:\n  group: (.*)$", wf, re.M)
g = group.group(1).rstrip() if group else ""
say(g.endswith("|| format('ci-{0}-{1}', github.event_name, github.run_id) }}") and "github.ref" not in g.replace("github.head_ref", ""),
    "T4 a run that is no pull request's has a concurrency group of its own run id")
say("&& 'full' || 'pr'" in g and "format('noop-{0}', github.run_id)" in g,
    "T4 a pull request's full and pr runs are separate groups, and another label's noop cancels nothing")
say(re.search(r"^  push:\n    branches: \[trunk\]", wf, re.M) is not None, "T4 a push to trunk runs")
say(re.search(r"^  schedule:\n(\s+#.*\n)*\s+- cron: ", wf, re.M) is not None, "T4 the nightly runs")
say("    types: [opened, synchronize, reopened, labeled]\n" in wf, "T4 adding a label starts a run")
say(re.search(r"^  workflow_dispatch:\n    inputs:\n      jobs:\n(?:        .*\n)+      tier:\n", wf, re.M) is not None
    and "IN_JOBS: ${{ inputs.jobs }}" in blocks["plan"] and "IN_TIER: ${{ inputs.tier }}" in blocks["plan"],
    "T4 a dispatch takes the jobs input and the tier input")
for j in ("build-test", "freebsd-check"):
    say(j in EVERY and j not in PR_JOBS and j not in plan(["crates/wolf_sema/src/lib.rs"], **pr)["jobs"] and j in plan(event="push")["jobs"]
        and j in plan(event="schedule")["jobs"], "T4 %s is the merge's: no pull-request push runs it, the label, a trunk push and the nightly do" % j)
say("native-macos" in EVERY and "native-macos" not in plan(["crates/wolf_sema/src/lib.rs"], **pr)["jobs"]
    and "native-macos" not in plan(event="push")["jobs"]
    and "native-macos" not in plan(["CHANGELOG.md"], event="pull_request", action="labeled", label="full-matrix", labels="full-matrix")["jobs"]
    and "native-macos" in plan(event="schedule")["jobs"]
    and "native-macos" in plan(["CHANGELOG.md"], event="pull_request", action="labeled", label="release-matrix", labels="release-matrix")["jobs"],
    "T4 the native macOS gauntlet is the release tier's alone: the nightly and the release-matrix label run it, the merge does not (tl30)")
BT_LEGS = plan(event="push").get("matrix", {}).get("matrix_build_test", [])
say(re.findall(r"^    runs-on: (.*)$", blocks["build-test"], re.M) == ["${{ matrix.os }}"]
    and "        include: ${{ fromJSON(needs.plan.outputs.matrix_build_test || '[]') }}\n" in blocks["build-test"]
    and any(l.get("os") == "macos-latest" for l in BT_LEGS),
    "T4 the merge keeps a macOS smoke: build + test on macos-latest, its legs the plan's (tl30)")
say("contains(github.event.pull_request.labels.*.name, 'release-matrix') && 'release'" in g
    and "github.event.label.name != 'release-matrix'" in g, "T4 the release-matrix run is a concurrency group of its own, and the label starts it")
n = blocks.get("nightly-red", "")
say("    needs: tier\n" in n and "    if: ${{ failure() && github.event_name == 'schedule' }}\n" in n
    and "      issues: write\n" in n and "        run: ci/nightly-red.sh\n" in n and "WORKFLOW: ${{ github.workflow }}" in n,
    "T4 a red nightly, and only a red nightly, runs ci/nightly-red.sh with issues: write")
runs_on = {j: re.findall(r"^    runs-on: (.*)$", blocks[j], re.M) for j in PR_JOBS}
say(all(v == ["ubuntu-24.04"] for v in runs_on.values()), "T4 every pull-request job runs on ubuntu (%s)" % runs_on)

expect("variadic plants run only their selected job", plan(event="workflow_dispatch", jobs="variadic-abi"), "partial", ["variadic-abi"])
expect("lane evidence remains a PR-class input", plan(["notes/s263-evidence/reproducer.sh"], **pr), "pr", PR_JOBS)
say("variadic-abi" not in PR_JOBS and "variadic-abi" in plan(event="push")["jobs"],
    "the focused variadic job is full/release and explicit partial dispatch, not a second PR gauntlet")
expect("static-entry plants run only their selected job", plan(event="workflow_dispatch", jobs="static-entry"), "partial", ["static-entry"])
say("static-entry" not in PR_JOBS and "static-entry" in plan(event="push")["jobs"],
    "the focused static-entry job (s267) is full/release and explicit partial dispatch; lint-test's gauntlet runs its rows on a PR")

expect("tool reporting plants run only their selected job", plan(event="workflow_dispatch", jobs="tool-details"), "partial", ["tool-details"])
say("tool-details" not in PR_JOBS and "tool-details" in plan(event="push")["jobs"],
    "the focused tool-details job is full/release and explicit partial dispatch; the PR gauntlet runs its rows")

# --- T7 the change read from git ----------------------------------------------------
def git(repo, *args):
    cmd = ["git", "-C", repo, "-c", "user.name=tiers-test", "-c", "user.email=tiers-test@invalid",
           "-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"] + list(args)
    return subprocess.run(cmd, check=True, capture_output=True, text=True).stdout.strip()


def write(repo, rel, text):
    full = os.path.join(repo, rel)
    os.makedirs(os.path.dirname(full), exist_ok=True)
    with open(full, "w", encoding="utf-8") as f:
        f.write(text)


def merged(topic_paths):
    """A repository whose HEAD merges a topic branch (changing topic_paths)
    into a base that moved on meanwhile -> (repo, the topic's head)."""
    repo = tempfile.mkdtemp(prefix="tiers-test-")
    git(repo, "init", "-q", "-b", "trunk")
    for rel in ("crates/a/src/lib.rs", "docs/platforms.md", "CHANGELOG.md", "spec/anchors.json"):
        write(repo, rel, "one\n")
    git(repo, "add", ".")
    git(repo, "commit", "-q", "-m", "base")
    git(repo, "checkout", "-q", "-b", "topic")
    for rel in topic_paths:
        write(repo, rel, "the topic\n")
    git(repo, "add", ".")
    git(repo, "commit", "-q", "-m", "topic")
    head = git(repo, "rev-parse", "HEAD")
    git(repo, "checkout", "-q", "trunk")
    write(repo, "crates/a/src/moved.rs", "the base moved on\n")
    git(repo, "add", ".")
    git(repo, "commit", "-q", "-m", "trunk moves")
    git(repo, "merge", "-q", "--no-ff", "-m", "merge", "topic")
    return repo, head


made = []
try:
    repo, head = merged(["docs/platforms.md", "CHANGELOG.md", "docs/a new page.md"])
    made.append(repo)
    expect("T7 a prose-only merge, the base moved on (its crates/a/src/moved.rs is not the pull request's)",
           plan(repo=repo, head_sha=head, flags=["--merge"], **pr), "noop", [])
    expect("T7 the same merge, the head not given", plan(repo=repo, flags=["--merge"], **pr), "noop", [])
    expect("T7 the merge's second parent is not the pull request's head",
           plan(repo=repo, head_sha="0" * 40, flags=["--merge"], **pr), "full", MERGE)
    git(repo, "checkout", "-q", "topic")
    expect("T7 HEAD is the branch, not a merge commit", plan(repo=repo, head_sha=head, flags=["--merge"], **pr), "full", MERGE)
    repo, head = merged(["docs/platforms.md", "crates/a/src/lib.rs"])
    made.append(repo)
    expect("T7 a merge that changes code and prose", plan(repo=repo, head_sha=head, flags=["--merge"], **pr), "pr", PR_JOBS)
    repo, head = merged(["CHANGELOG.md", "spec/anchors.json"])
    made.append(repo)
    expect("T7 a merge that changes the CHANGELOG and spec/anchors.json", plan(repo=repo, head_sha=head, flags=["--merge"], **pr), "pr", PR_JOBS)
    repo, head = merged(["CHANGELOG.md", "somewhere/new.txt"])
    made.append(repo)
    expect("T7 a merge that adds a path no rule names", plan(repo=repo, head_sha=head, flags=["--merge"], **pr), "full", MERGE)
except (subprocess.CalledProcessError, OSError) as e:
    say(False, "T7 could not build its repositories: %s %s" % (e, getattr(e, "stderr", "")))
finally:
    for d in made:
        shutil.rmtree(d, ignore_errors=True)

# --- T8 the nightly's issue -------------------------------------------------------
FAKE_GH = r"""#!/usr/bin/env bash
printf '%s\n' "$*" >> "$FAKE_LOG"
case "$1 $2" in
  "label create") exit "${FAKE_LABEL_RC:-0}" ;;
  "issue list") printf '%s' "${FAKE_EXISTING:-}"; [ -n "${FAKE_EXISTING:-}" ] && echo; exit 0 ;;
esac
exit 0
"""


def nightly_red(existing="", label_rc=0):
    d = tempfile.mkdtemp(prefix="tiers-test-gh-")
    try:
        with open(os.path.join(d, "gh"), "w") as f:
            f.write(FAKE_GH)
        os.chmod(os.path.join(d, "gh"), 0o755)
        log = os.path.join(d, "log")
        env = dict(os.environ, PATH=d + os.pathsep + os.environ.get("PATH", ""), FAKE_LOG=log, FAKE_EXISTING=existing,
                   FAKE_LABEL_RC=str(label_rc), REPO="wolffe-lang/wolf-lang", WORKFLOW="ci",
                   RUN_URL="https://github.com/wolffe-lang/wolf-lang/actions/runs/1")
        r = subprocess.run(["bash", RED], capture_output=True, text=True, env=env)
        calls = open(log).read().splitlines() if os.path.exists(log) else []
        return r.returncode, calls
    finally:
        shutil.rmtree(d, ignore_errors=True)


TITLE = "nightly red: ci on trunk"
rc, calls = nightly_red()
creates = [c for c in calls if c.startswith("issue create")]
say(rc == 0 and len(creates) == 1 and "--title %s" % TITLE in creates[0] and "--label nightly-red" in creates[0]
    and not [c for c in calls if c.startswith("issue comment")],
    "T8 no open issue: one is opened, labelled nightly-red, titled '%s'" % TITLE)
lists = [c for c in calls if c.startswith("issue list")]
say(len(lists) == 1 and "--state open" in lists[0] and "--label nightly-red" in lists[0] and TITLE in lists[0],
    "T8 the dedupe asks for an open nightly-red issue under the title")
rc, calls = nightly_red(existing="41")
say(rc == 0 and [c for c in calls if c.startswith("issue comment 41 ")] and not [c for c in calls if c.startswith("issue create")],
    "T8 an open issue: a comment on it, no new issue")
rc, calls = nightly_red(label_rc=1)
say(rc == 0 and [c for c in calls if c.startswith("issue create")], "T8 a label that already exists is no failure")
r = subprocess.run(["bash", RED], capture_output=True, text=True, env=clean_env(PATH=os.environ.get("PATH", "")))
say(r.returncode != 0, "T8 the script refuses to run without REPO, WORKFLOW and RUN_URL")
say(re.search(r"^name: ci$", wf, re.M) is not None, "T8 the workflow's name is ci, the title's")


# --- T5 the tripwire: what the planner calls prose, nothing reads ---------------------
READS = re.compile(r"read_to_string|include_str!|include_bytes!|File::open|fs::read\b|read_dir|\.join\(|Path::new|PathBuf::from")
# Ruled prose although a line reads it (the contract names the CHANGELOG):
# xtask's `the_repos_own_changelog_yields_the_0_2_2_entry` pins the 0.2.2
# entry, which a new entry cannot move, and `cargo xtask release-notes`
# runs at a tag. The label's full run before the merge runs the test.
RULED_PROSE = ["CHANGELOG.md"]


def tracked():
    try:
        return subprocess.run(["git", "-C", ROOT, "ls-files"], check=True, capture_output=True, text=True).stdout.split("\n")
    except (subprocess.CalledProcessError, FileNotFoundError):
        return []


def classes(paths):
    out = {}
    for i in range(0, len(paths), 500):
        r = subprocess.run([sys.executable, PLAN, "classify"] + paths[i:i + 500], check=True, capture_output=True, text=True)
        out.update(line.split("\t")[:2] for line in r.stdout.splitlines())
    return out


def readers(paths):
    """{path: ["file:line", ...]}: the lines of code that read a file and
    name the path, or a directory above it, as a string."""
    code = [p for p in files if p.startswith(("crates/", "xtask/", "fuzz/")) and p.endswith(".rs")]
    names = {}
    for p in paths:
        cand = {p}
        d = os.path.dirname(p)
        while d:
            cand.add(d)
            d = os.path.dirname(d)
        names[p] = cand
    hits = {p: [] for p in paths}
    for c in code:
        with open(os.path.join(ROOT, c), encoding="utf-8", errors="replace") as f:
            for n, line in enumerate(f, 1):
                if '"' not in line or line.lstrip().startswith("//") or not READS.search(line):
                    continue
                lits = set(re.findall(r'"/?(?:\.\./)*([A-Za-z0-9_./-]+?)/?"', line))
                if not lits:
                    continue
                for p in paths:
                    if lits & names[p]:
                        hits[p].append("%s:%d" % (c, n))
    return hits


files = [f for f in tracked() if f]
if files:
    cls = classes(files)
    prose = sorted(p for p in files if cls.get(p) == "prose")
    unknown = sorted(p for p in files if cls.get(p) == "full")
    print("tiers-test: the %d tracked paths that are prose: %s" % (len(prose), " ".join(prose)))
    must = ["README.md", "docs/diagnostics.md", "spec/11-os.md"]
    seen = readers(must)
    say(all(seen[p] for p in must), "T5 the search fires: it finds what reads %s" % ", ".join(
        "%s (%s)" % (p, seen[p][0] if seen[p] else "NOTHING") for p in must))
    hit = {p: w for p, w in readers(prose).items() if w}
    say(sorted(hit) == RULED_PROSE, "T5 the prose paths a line that reads a file names are the ruled one: %s" % (
        "; ".join("%s at %s" % (p, " ".join(w[:3])) for p, w in sorted(hit.items())) or "none"))
    spec = [p for p in files if p.startswith("spec/")]
    say(len(spec) >= 15 and all(cls.get(p) == "pr" for p in spec), "T5 no tracked path under spec/ is prose (%d paths)" % len(spec))
    odd = [p for p in prose if not p.endswith(".md")]
    say(not odd, "T5 nothing but a .md file is prose%s" % ("" if not odd else ": " + " ".join(odd[:10])))
    if unknown:
        print("tiers-test: WARN T5 %d tracked path(s) no rule names (a pull request touching one runs full): %s"
              % (len(unknown), " ".join(unknown[:20])))
    else:
        print("tiers-test: PASS T5 every one of %d tracked paths has a rule" % len(files))
else:
    say(False, "T5 not a git checkout: the tracked paths cannot be listed")

# T9: a runner image named `latest` moves under every job at once. On 2026-11-18
# ubuntu-latest began handing out Ubuntu 26.04 runners, whose toolchain and libc
# differ from 24.04's, and a pull request's run could pass on one image while the
# trunk push ran on the other. No workflow line and no planner leg may say
# ubuntu-latest: the Linux runners are pinned by version, and moving them is a
# change of its own.
import glob
stale = []
for path in sorted(glob.glob(os.path.join(ROOT, ".github", "workflows", "*"))) + [PLAN]:
    with open(path, encoding="utf-8") as fh:
        for n, line in enumerate(fh, 1):
            if "ubuntu-latest" in line:
                stale.append("%s:%d" % (os.path.relpath(path, ROOT), n))
say(not stale, "T9 no workflow or planner leg says ubuntu-latest, the Linux runner image is pinned"
    + ("" if not stale else " (%d places: %s)" % (len(stale), ", ".join(stale))))

# T11 a nightly at a commit already green in full is skipped, and says so (ci/covered.py, against a fake gh).
COVERED_PY = os.path.join(ROOT, "ci", "covered.py")
SKIP_TIER = "release"
PLAIN_ = re.compile(r"\b(?:lanes?|rulings?|crawls?|sprints?)\b|\b[a-z]{2}\d{2,3}[a-z]?\b", re.I)
GREEN = dict(covered=SKIP_TIER, covered_by="https://github.com/o/r/runs/555", covered_at="2026-11-18T12:00:00Z", covered_run="777")
job0 = EVERY[0]

# S1: the nightly, covered: skipped, no job; its line, its warning and its summary name the covering run.
expect("T11 S1 the nightly at a commit whose tier %s is green" % SKIP_TIER, plan(event="schedule", **GREEN), "skipped", [])
d = tempfile.mkdtemp(prefix="tiers-test-skip-")
try:
    summ, gout = os.path.join(d, "summary.md"), os.path.join(d, "out")
    args = [sys.executable, PLAN, "plan", "--event", "schedule", "--github-output", gout] + [
        x for k, v in GREEN.items() for x in ("--" + k.replace("_", "-"), v)]
    r = subprocess.run(args, capture_output=True, text=True, env=clean_env(GITHUB_STEP_SUMMARY=summ))
    outs = open(gout, encoding="utf-8").read() if os.path.exists(gout) else ""
    summary = open(summ, encoding="utf-8").read() if os.path.exists(summ) else ""
    warn = [l for l in r.stdout.splitlines() if l.startswith("::warning title=nightly skipped::")]
    said_ = ("nothing tested tonight", "tier %s green at this commit in run 777" % SKIP_TIER, "https://github.com/o/r/runs/555",
             "completed 2026-11-18T12:00:00Z", "due by 2026-11-24T12:00:00Z")
    say(r.returncode == 0 and "tier=skipped\n" in outs and "jobs=[]\n" in outs and "covered_by=https://github.com/o/r/runs/555\n" in outs
        and len(warn) == 1 and all(s in warn[0] for s in said_) and all(s in r.stderr for s in said_) and all(s in summary for s in said_)
        and not PLAIN_.search(warn[0] + summary),
        "T11 S1 a skipped night says so in its log, a warning and the run's summary, in plain words, naming the covering run and when a full night is due%s" % (
            "" if r.returncode == 0 else " (rc %d: %s)" % (r.returncode, r.stderr.strip()[-200:])))
finally:
    shutil.rmtree(d, ignore_errors=True)

# S2: anything else the nightly is told is no skip: the nightly's tier, as today.
for c in ("none", "", "full", "pr", "partial", "skipped", "noop", "all", SKIP_TIER.upper(), SKIP_TIER + " yes"):
    expect("T11 S2 the nightly told covered=%r" % c, plan(event="schedule", covered=c), "release", EVERY)
expect("T11 S2 the nightly told nothing", plan(event="schedule"), "release", EVERY)

# S3: a skip is the nightly's only.
expect("T11 S3 a push told covered=%s" % SKIP_TIER, plan(event="push", **GREEN), "full", MERGE)
expect("T11 S3 a dispatch told covered=%s" % SKIP_TIER, plan(event="workflow_dispatch", **GREEN), "full", MERGE)
expect("T11 S3 a dispatch asking for tier release, told covered=%s" % SKIP_TIER, plan(event="workflow_dispatch", tier="release", **GREEN), "release", EVERY)
expect("T11 S3 a code pull request told covered=%s" % SKIP_TIER,
       plan(["crates/wolf_sema/src/lib.rs"], event="pull_request", action="synchronize", **GREEN), "pr", PR_JOBS)
say(plan(event="workflow_dispatch", tier="skipped")["tier"].startswith("ERROR rc=2"), "T11 S3 a dispatch asking for tier skipped is refused")

# S4: the verdict of a skipped night.
vexpect("T11 S4 a skipped night, every job skipped", run_of("skipped", []), 0)
vexpect("T11 S4 a skipped night in which a job ran", run_of("skipped", [], {job0: {"result": "success", "outputs": {}}}), 1)
vexpect("T11 S4 a skipped night whose plan chose a job", run_of("skipped", [job0]), 1)
n_ = run_of("skipped", [])
n_["plan"]["outputs"]["covered_by"] = "https://github.com/o/r/runs/555"
rc, text = verdict(n_)
say(rc == 0 and "nothing was tested" in text and "https://github.com/o/r/runs/555" in text,
    "T11 S4 the verdict of a skipped night says nothing was tested and names the covering run")

# S5: ci/covered.py against a fake gh.
FAKE_GH_COV = """#!/usr/bin/env python3
import json, os, sys
a = sys.argv[1:]
with open(os.environ["FAKE_GH_LOG"], "a", encoding="utf-8") as f:
    f.write(json.dumps(a) + "\\n")
if os.environ.get("FAKE_GH_FAIL"):
    sys.stderr.write("HTTP 502: the fake refuses\\n")
    sys.exit(1)
path = a[-1] if a[:1] == ["api"] else ""
if "/check-runs" in path:
    sys.stdout.write(os.environ["FAKE_CHECKS"])
elif "/actions/runs?" in path:
    sys.stdout.write(os.environ["FAKE_RUNS"])
elif "/actions/runs/" in path:
    sys.stdout.write(os.environ["FAKE_RUN"])
else:
    sys.stderr.write("the fake knows no %r\\n" % a)
    sys.exit(1)
"""
COV_SHA = "0e536e8f" + "0" * 32


def check(name="tier " + SKIP_TIER, concl="success", done="2026-11-18T12:00:00Z", suite=1, app="github-actions",
          url="https://github.com/o/r/runs/555", status="completed"):
    return {"name": name, "status": status, "conclusion": concl, "completed_at": done, "check_suite": {"id": suite},
            "app": {"slug": app}, "html_url": url}


def wrun(suite=1, path=".github/workflows/ci.yml", sha=COV_SHA, rid=777):
    return {"id": rid, "check_suite_id": suite, "path": path, "head_sha": sha}


def covered(checks, runs=None, event="schedule", fail=False, total=None, drop=()):
    d = tempfile.mkdtemp(prefix="tiers-test-cov-")
    try:
        with open(os.path.join(d, "gh"), "w", encoding="utf-8") as f:
            f.write(FAKE_GH_COV)
        os.chmod(os.path.join(d, "gh"), 0o755)
        log = os.path.join(d, "calls.jsonl")
        runs = [wrun()] if runs is None else runs
        env = clean_env(PATH=d + os.pathsep + os.environ.get("PATH", ""), FAKE_GH_LOG=log, REPO="o/r", SHA=COV_SHA,
                        TIER=SKIP_TIER, RUN_ID="999", EVENT=event, GITHUB_EVENT_NAME="schedule",
                        FAKE_RUN=json.dumps({"id": 999, "check_suite_id": 9, "created_at": "2026-11-20T12:00:00Z"}),
                        FAKE_CHECKS=json.dumps({"total_count": len(checks) if total is None else total, "check_runs": checks}),
                        FAKE_RUNS=json.dumps({"total_count": len(runs), "workflow_runs": runs}))
        for k in drop:
            env.pop(k, None)
        if fail:
            env["FAKE_GH_FAIL"] = "1"
        r = subprocess.run([sys.executable, COVERED_PY], capture_output=True, text=True, env=env)
        got = dict(l.split("=", 1) for l in r.stdout.splitlines() if "=" in l)
        calls = [json.loads(x) for x in open(log, encoding="utf-8")] if os.path.exists(log) else []
        return r.returncode, got, calls, r.stderr
    finally:
        shutil.rmtree(d, ignore_errors=True)


def cexpect(name, res, want):
    rc, got, calls, err = res
    ok = rc == 0 and got.get("covered") == want and bool(got.get("reason"))
    say(ok, "T11 S5 %s: covered=%s%s" % (name, got.get("covered"), "" if ok else " (wanted %s; rc %d; %s)" % (want, rc, err.strip()[-200:])))


if os.name == "nt":
    say(True, "T11 S5 skipped: the fake gh is a script with a shebang")
else:
    res = covered([check()])
    rc, got, calls, err = res
    say(rc == 0 and got.get("covered") == SKIP_TIER and got.get("covered_by") == "https://github.com/o/r/runs/555"
        and got.get("covered_at") == "2026-11-18T12:00:00Z" and got.get("covered_run") == "777"
        and any(("check_name=tier%%20%s" % SKIP_TIER) in c[-1] and "filter=all" in c[-1] for c in calls),
        "T11 S5 (a) a green tier %s of ci.yml at the commit, 2 days old: covered, naming the check run, its time and its run%s" % (
            SKIP_TIER, "" if rc == 0 and got.get("covered") == SKIP_TIER else " (%s; %s)" % (got, err.strip()[-200:])))
    cexpect("(b) the same, 7 days old", covered([check(done="2026-11-13T11:00:00Z")]), "none")
    cexpect("(b) the same, 144 hours old to the second", covered([check(done="2026-11-14T12:00:00Z")]), SKIP_TIER)
    for nm in ("tier full", "tier pr", "tier partial", "tier skipped", "tier noop", "tier " + SKIP_TIER + "s"):
        cexpect("(c) a green %s" % nm, covered([check(name=nm)]), "none")
    cexpect("(d) a green tier %s of another workflow" % SKIP_TIER, covered([check()], runs=[wrun(path=".github/workflows/release.yml")]), "none")
    cexpect("(d) a green tier %s whose run is of another commit" % SKIP_TIER, covered([check()], runs=[wrun(sha="1" * 40)]), "none")
    cexpect("(d) a green tier %s whose run is not at this commit at all" % SKIP_TIER, covered([check()], runs=[]), "none")
    for c in ("failure", "cancelled", "timed_out"):
        cexpect("(e) the tier %s %s" % (SKIP_TIER, c), covered([check(concl=c)]), "none")
    cexpect("(e) the tier %s still running" % SKIP_TIER, covered([check(status="in_progress", concl=None, done=None)]), "none")
    res = covered([check()], fail=True)
    cexpect("(g) gh exits non-zero", res, "none")
    say("unanswered" in res[1].get("reason", ""), "T11 S5 (g) the reason says the question went unanswered")
    cexpect("(h) a green tier %s of another app" % SKIP_TIER, covered([check(app="some-app")]), "none")
    cexpect("(i) a full page of check runs", covered([check()], total=100), "none")
    cexpect("(i) a full page of workflow runs", covered([check()], runs=[wrun(rid=700 + i, suite=1 if i == 0 else 100 + i) for i in range(100)]), "none")
    cexpect("the latest word: a red tier %s after a green one at the commit" % SKIP_TIER,
            covered([check(), check(concl="failure", done="2026-11-19T12:00:00Z", suite=2, url="https://github.com/o/r/runs/556")],
                    runs=[wrun(), wrun(suite=2, rid=778)]), "none")
    cexpect("the latest word: a red first attempt, then a green rerun",
            covered([check(concl="failure", done="2026-11-18T10:00:00Z"), check(done="2026-11-18T12:00:00Z")]), SKIP_TIER)
    cexpect("an earlier attempt of tonight's own run, green (a rerun of a green night)",
            covered([check(suite=9)], runs=[wrun(suite=9, rid=999)]), "none")
    res = covered([check()], event="push")
    cexpect("a push: never covered", res, "none")
    say(res[0] == 0 and not res[2] and "not the nightly" in res[1].get("reason", ""),
        "T11 S5 a push: covered.py asks GitHub nothing (%d calls) and says why" % len(res[2]))
    cexpect("no EVENT: GITHUB_EVENT_NAME decides (schedule)", covered([check()], event=""), SKIP_TIER)
    res = covered([check()], drop=("TIER",))
    say(res[0] == 2 and "must be set" in res[3] and not res[2], "T11 S5 covered.py refuses to run without TIER, and asks nothing")

# S6: the wiring of the plan job.
pb = blocks["plan"]
cov_step = re.search(r"^      - name: what this commit already has green \(ci/covered\.py\)\n((?:        .*\n)+)", pb, re.M)
cs = cov_step.group(1) if cov_step else ""
plan_step = re.search(r"^      - id: plan\n((?:        .*\n)+)", pb, re.M)
ps = plan_step.group(1) if plan_step else ""
pperms = re.search(r"^    permissions:\n((?:      .*\n)+)", pb, re.M)
say(bool(cs) and "        id: covered\n" in cs and '        run: python3 ci/covered.py --github-output "$GITHUB_OUTPUT"\n' in cs
    and all(("          %s\n" % e) in cs for e in ("GH_TOKEN: ${{ github.token }}", "REPO: ${{ github.repository }}",
                                                   "SHA: ${{ github.sha }}", "RUN_ID: ${{ github.run_id }}", "TIER: %s" % SKIP_TIER))
    and bool(ps) and pb.index("ci/covered.py") < pb.index("ci/tiers.py plan"),
    "T11 S6 the plan job asks ci/covered.py, with the nightly's tier, before it plans")
say(all(("          %s: ${{ steps.covered.outputs.%s }}\n" % (k.upper(), k)) in ps for k in ("covered", "covered_by", "covered_at", "covered_run"))
    and wf.count("steps.covered.outputs.") == 4 and "      covered_by: ${{ steps.plan.outputs.covered_by }}\n" in pb,
    "T11 S6 only the plan step reads ci/covered.py's answer, and the plan passes covered_by on to the verdict")
say(pperms is not None and sorted(pperms.group(1).split()) == sorted(["contents:", "read", "checks:", "read", "actions:", "read"]),
    "T11 S6 the plan job reads the code, the checks and the runs, and writes nothing")

# S7: the nightly's issue says a skipped night is not a green one.
# The stand-in logs each call on one line, and the body's own lines after it: read each run's log whole.
said_ = ["\n".join(nightly_red(existing=e)[1]) for e in ("", "41")]
say("issue create" in said_[0] and "issue comment 41" in said_[1]
    and all("close this issue once a nightly is green again (a `tier skipped` night is not one)" in t for t in said_),
    "T11 S7 the nightly's issue, opened or commented, says to close it once a nightly is green again, and that a tier skipped night is not one")

# T12 each `build + test` leg's limit is its longest healthy run plus a third, rounded up to 5 minutes (#1217).
# The macOS leg filled the 45 minutes the matrix shared: healthy runs reached 44.6, and two were cut inside
# `cargo test --workspace`. ci/job-limits.json holds each leg's longest healthy run and the runs it was read from.
LIMITS = json.load(open(os.path.join(ROOT, "ci", "job-limits.json"), encoding="utf-8"))


def limit_floor(longest):
    """The least limit the rule allows: the longest healthy run plus a third, rounded up to the next 5 minutes."""
    return math.ceil(round(longest * 4 / 3, 6) / 5) * 5


def leg_limits(block, matrix):
    """{leg name: limit in minutes} for a matrix job whose legs are the plan's:
    each leg's `timeout` when the job reads it, else the job's own."""
    own = re.search(r"^    timeout-minutes: (.+)$", block, re.M)
    own = own.group(1).strip() if own else ""
    legs = {}
    for keys in matrix:
        name = str(keys["os"]) + (", " + str(keys["leg"]) if keys.get("leg") else "")
        value = str(keys.get("timeout", "")) if own == "${{ matrix.timeout }}" else own
        legs[name] = int(value) if value.isdigit() else None
    return legs


def limit_problems(legs, record):
    found = []
    for name in sorted(set(legs) | set(record)):
        if name not in record:
            found.append("%s: no record in ci/job-limits.json" % name)
        elif name not in legs:
            found.append("%s: a record but no leg" % name)
        elif legs[name] is None:
            found.append("%s: no limit in minutes" % name)
        elif legs[name] < limit_floor(record[name]["longest_minutes"]):
            found.append("%s: limit %d under %d (longest healthy %.1f, plus a third, rounded up to 5)"
                         % (name, legs[name], limit_floor(record[name]["longest_minutes"]), record[name]["longest_minutes"]))
    return found


record = LIMITS["build-test"]
got = leg_limits(blocks.get("build-test", ""), plan(event="push").get("matrix", {}).get("matrix_build_test", []))
probs = limit_problems(got, record)
says_no = (limit_floor(44.6) == 60 and limit_floor(30.0) == 40 and limit_floor(15.1) == 25
           and limit_problems({"a": 59}, {"a": {"longest_minutes": 44.6}})
           and not limit_problems({"a": 60}, {"a": {"longest_minutes": 44.6}})
           and limit_problems({"a": 60, "b": 60}, {"a": {"longest_minutes": 44.6}}))
say(bool(got) and not probs and says_no,
    "T12 each build + test leg's limit is at least its longest healthy run plus a third, rounded up to 5 (%s)%s%s"
    % (", ".join("%s %s" % (k, v) for k, v in sorted(got.items())) or "no legs",
       ("; " + "; ".join(probs)) if probs else "", "" if says_no else "; the check does not say no to a limit under the rule"))


# T13: the macOS leg a trunk push may leave (name: runner label), as GitHub names
# it; T13_JOBS, each one's job id; T13_LINUX, other jobs a run has.
T13_REPO = "wolffe-lang/wolf-lang"
T13_MAC = {"build + test (macos-latest)": "macos-latest"}
T13_JOBS = {"build + test (macos-latest)": "build-test"}
T13_WHOLE = []
T13_LINUX = ["lint + test + corpus (linux x86-64)", "build + test (windows-latest, tests)", "build + test (windows-latest, rung)"]
WORKFLOW_FILE = "ci.yml"

# --- T13 a trunk push leaves the macOS legs already green at its commit -----------
# Merges are fast-forwards, so a trunk push runs the commit its pull request's run
# has just passed. On the runner's own push to trunk the plan asks the REST API for
# this workflow's runs at the commit and leaves a macOS leg or job only on positive
# evidence. These rows run the planner as the workflow does (EVENT, and the
# runner's GITHUB_EVENT_NAME, GITHUB_REF, GITHUB_SHA, GITHUB_REPOSITORY,
# GITHUB_API_URL; PLAN_TOKEN) over a scratch git repository (--repo), against a
# stand-in API on 127.0.0.1; then the verdict's rows and the wiring's.
import http.server
import math
import threading
import time
import urllib.parse

T13_TOKEN = "tiers-test-stand-in-token-5e8a"
T13_SCRUB = ENV_KEYS + ("PLAN_TOKEN", "PLAN_API_TIMEOUT", "GH_TOKEN", "GITHUB_TOKEN", "GITHUB_EVENT_NAME", "GITHUB_REF",
                        "GITHUB_SHA", "GITHUB_REPOSITORY", "GITHUB_API_URL", "GITHUB_SERVER_URL", "GITHUB_STEP_SUMMARY",
                        "GITHUB_OUTPUT")
T13_WF_PATH = ".github/workflows/" + WORKFLOW_FILE


class T13Server(http.server.ThreadingHTTPServer):
    # A client that timed out leaves a handler writing to a closed socket.
    daemon_threads = True

    def handle_error(self, request, client_address):
        pass


class T13API:
    """The REST API's two answers the plan reads: the runs of the workflow at a
    commit, and a run's jobs (paged). `fault` is (path prefix, kind)."""

    def __init__(self, runs=(), jobs=None, fault=None, page_cap=100, ignore_filter=False, totals=None):
        self.runs, self.jobs = list(runs), dict(jobs or {})
        self.fault, self.page_cap, self.ignore_filter = fault, page_cap, ignore_filter
        self.totals, self.requests, self.redirect_to = dict(totals or {}), [], None
        api = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                api.requests.append((self.path, self.headers.get("Authorization")))
                status, body, extra = api.answer(self.path)
                data = body if isinstance(body, bytes) else json.dumps(body).encode()
                self.send_response(status)
                for k, v in extra.items():
                    self.send_header(k, v)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def log_message(self, *args):
                pass

        self.server = T13Server(("127.0.0.1", 0), Handler)
        threading.Thread(target=self.server.serve_forever, kwargs={"poll_interval": 0.05}, daemon=True).start()
        self.url = "http://127.0.0.1:%d" % self.server.server_address[1]

    def close(self):
        self.server.shutdown()
        self.server.server_close()

    def answer(self, path):
        u = urllib.parse.urlsplit(path)
        q = dict(urllib.parse.parse_qsl(u.query))
        if self.fault and u.path.startswith(self.fault[0]):
            kind = self.fault[1]
            if kind in (500, 403, 404):
                return kind, {"message": "stand-in error %d" % kind}, {}
            if kind == "html":
                return 200, b"<html>not json</html>", {}
            if kind == "keys":
                return 200, {"message": "an answer without the expected keys"}, {}
            if kind == "slow":
                time.sleep(4)
            if kind == "redirect":
                return 302, {}, {"Location": self.redirect_to + path}
        if u.path == "/repos/%s/actions/workflows/%s/runs" % (T13_REPO, WORKFLOW_FILE):
            rs = [x for x in self.runs if self.ignore_filter or x["head_sha"] == q.get("head_sha")]
            return 200, {"total_count": len(rs), "workflow_runs": rs}, {}
        m = re.fullmatch(r"/repos/%s/actions/runs/(\d+)/jobs" % re.escape(T13_REPO), u.path)
        if m:
            rid = int(m.group(1))
            js = self.jobs.get(rid, [])
            per, page = min(int(q.get("per_page", 30)), self.page_cap), int(q.get("page", 1))
            return 200, {"total_count": self.totals.get(rid, len(js)), "jobs": js[(page - 1) * per:page * per]}, {}
        return 404, {"message": "Not Found"}, {}


T13_GIT = """#!/bin/sh
# A stand-in for git: the plan's two questions about the checkout, the
# pushed commit and its tree, and nothing else (any other call fails, so a
# plan that asks git anything more fails its rows).
[ "$1" = "-C" ] && shift 2
if [ "$*" = "rev-parse HEAD" ]; then echo "$T13_HEAD"; exit 0; fi
if [ "$*" = "rev-parse HEAD^{tree}" ]; then echo "$T13_TREE"; exit 0; fi
echo "stand-in git: not answered: $*" >&2
exit 1
"""


def t13_repo():
    """The checkout a trunk push plans in: a directory whose `git` (first on
    the plan's PATH) answers for a pushed commit and its tree, so these rows
    need no git of their own."""
    d = tempfile.mkdtemp(prefix="tiers-test-push-")
    with open(os.path.join(d, "git"), "w") as f:
        f.write(T13_GIT)
    os.chmod(os.path.join(d, "git"), 0o755)
    return d, "a1" * 20, "b2" * 20


T13_DIR, T13_SHA, T13_TREE = t13_repo()
T13_SEEN = []


def t13_plan(api, event="push", event_name="push", ref="refs/heads/trunk", sha=None, token=T13_TOKEN, timeout=None,
             summary=None):
    d = tempfile.mkdtemp(prefix="tiers-test-out-")
    out = os.path.join(d, "output")
    open(out, "w").close()
    env = {k: v for k, v in os.environ.items() if k not in T13_SCRUB}
    env.update(GITHUB_EVENT_NAME=event_name, GITHUB_REF=ref, GITHUB_SHA=sha or T13_SHA, GITHUB_REPOSITORY=T13_REPO,
               GITHUB_API_URL=api.url, GITHUB_SERVER_URL="https://github.com", T13_HEAD=T13_SHA, T13_TREE=T13_TREE)
    env["PATH"] = T13_DIR + os.pathsep + env.get("PATH", "")
    if token:
        env["PLAN_TOKEN"] = token
    if timeout:
        env["PLAN_API_TIMEOUT"] = str(timeout)
    if summary:
        env["GITHUB_STEP_SUMMARY"] = summary
    r = subprocess.run([sys.executable, PLAN, "plan", "--event", event, "--repo", T13_DIR, "--github-output", out],
                       capture_output=True, text=True, env=env, timeout=120)
    text = open(out).read()
    shutil.rmtree(d, ignore_errors=True)
    T13_SEEN.append(r.stdout + r.stderr + text)
    if r.returncode != 0:
        return {"ERROR": r.returncode}, r.stderr
    o = dict(l.split("=", 1) for l in text.splitlines() if "=" in l)
    got = {"tier": o.get("tier"), "jobs": json.loads(o.get("jobs", "null")),
           "matrix": {k: json.loads(v) for k, v in o.items() if k.startswith("matrix_")}}
    got["covered"] = json.loads(o["macos_covered"]) if "macos_covered" in o else None
    return got, r.stderr


def t13_render(job, leg):
    if job.replace("_", "-") == "build-test":
        return "build + test (%s%s)" % (leg["os"], ", " + leg["leg"] if leg.get("leg") else "")
    return "%s (%s)" % (job, ", ".join(str(v).lower() if isinstance(v, bool) else str(v) for v in leg.values()))


def t13_less(p, names):
    """A plan less the macOS legs and jobs named."""
    q = {"tier": p["tier"], "jobs": [j for j in p["jobs"] if not (j in T13_WHOLE and j in names)]}
    q["matrix"] = {k: [l for l in v if t13_render(k[len("matrix_"):], l) not in names] for k, v in p["matrix"].items()}
    return q


def t13_wrun(rid, event="pull_request", conclusion="success", status="completed", sha=None, path=None):
    return {"id": rid, "event": event, "status": status, "conclusion": conclusion, "head_sha": sha or T13_SHA,
            "path": path or T13_WF_PATH, "run_attempt": 1}


def t13_job(name, conclusion="success", labels=None, steps=None):
    return {"name": name, "status": "completed", "conclusion": conclusion,
            "labels": labels if labels is not None else [T13_MAC.get(name, "ubuntu-24.04")], "steps": steps or []}


def t13_green(tree=None, step=True, drop=(), swap=None, extra=()):
    steps = [{"name": "Set up job", "conclusion": "success"}]
    if step:
        steps.append({"name": "the tree under test is %s" % (tree or T13_TREE), "conclusion": "success"})
    js = [t13_job("plan", steps=steps), t13_job("tier full")] + [t13_job(n) for n in T13_LINUX]
    js += [(swap or {}).get(n, t13_job(n)) for n in T13_MAC if n not in drop]
    return js + list(extra)


T13_PLANS = []
T13_LINE = re.compile(r"^tiers: (skip|keep) (.+?): (.*)$", re.M)


def t13_row(what, api, want_names, **kw):
    """`want_names`: the macOS names the plan should leave (the rest run)."""
    got, err = t13_plan(api, **kw)
    T13_PLANS.append(got)
    want = t13_less(T13_FULL, want_names)
    lines = T13_LINE.findall(err)
    skipped = {n: why for v, n, why in lines if v == "skip"}
    ok = ("ERROR" not in got and {k: got[k] for k in ("tier", "jobs", "matrix")} == want
          and sorted(got["covered"] or {}) == sorted(want_names) and sorted(n for _, n, _ in lines) == sorted(T13_MAC)
          and sorted(skipped) == sorted(want_names))
    say(ok, "T13 %s: leaves %s%s" % (what, sorted(want_names) or "no macOS leg",
                                      "" if ok else " (got %s; lines %s)" % (got, [(v, n) for v, n, _ in lines])))
    return got, err, skipped


T13_FULL, _ = t13_plan(T13API(), token=None)
say("ERROR" not in T13_FULL and T13_FULL["tier"] == "full" and T13_FULL["covered"] == {}
    and sorted(n for n in T13_MAC) == sorted(
        [t13_render(k[len("matrix_"):], l) for k, v in T13_FULL["matrix"].items() for l in v if l["os"].startswith("macos")]
        + [j for j in T13_FULL["jobs"] if j in T13_WHOLE]),
    "T13 a trunk push with no token plans the full tier, covers nothing, and its macOS legs are %s" % sorted(T13_MAC))

summary = os.path.join(tempfile.mkdtemp(prefix="tiers-test-sum-"), "summary.md")
api = T13API([t13_wrun(7001)], {7001: t13_green()})
got, err, skipped = t13_row("a green pull request run at the commit and tree", api, list(T13_MAC), summary=summary)
say(got.get("covered") and all((c["run"], c["event"], c["sha"], c["tree"], c["label"]) == (7001, "pull_request", T13_SHA, T13_TREE, T13_MAC[n])
                               for n, c in got["covered"].items())
    and all(("green in pull_request run 7001 at %s, tree %s" % (T13_SHA, T13_TREE)) in why for why in skipped.values()),
    "T13 each skip names the run it relied on (pull_request run 7001), the commit and the tree")
say(os.path.exists(summary) and open(summary).read().count("7001") >= len(T13_MAC), "T13 the step summary names the run for each skipped leg")
jq = [urllib.parse.parse_qs(urllib.parse.urlsplit(p).query) for p, _ in api.requests if "/jobs" in p]
say(jq and all(q.get("filter") == ["latest"] for q in jq), "T13 a run's jobs are read at its latest attempt (filter=latest)")
say(api.requests and all(a == "Bearer " + T13_TOKEN for _, a in api.requests), "T13 every request carries the workflow's token")
api.close()

api = T13API([t13_wrun(7002, event="workflow_dispatch")], {7002: t13_green()})
t13_row("a green dispatch run at the commit counts", api, list(T13_MAC))
api.close()
for ev in ("schedule", "push"):
    api = T13API([t13_wrun(7003, event=ev)], {7003: t13_green()})
    t13_row("a green %s run at the commit does not count" % ev, api, [])
    api.close()
api = T13API([t13_wrun(7004, conclusion="failure"), t13_wrun(7005, conclusion="cancelled"),
              t13_wrun(7006, status="in_progress", conclusion=None)], {i: t13_green() for i in (7004, 7005, 7006)})
t13_row("a failed, a cancelled and a running pull request run do not count", api, [])
api.close()
for what, jobs in (("a run that tested another tree", t13_green(tree="0" * 40)), ("a run whose plan names no tree", t13_green(step=False)),
                   ("a green run with no macOS job at all", t13_green(drop=list(T13_MAC)))):
    api = T13API([t13_wrun(7007)], {7007: jobs})
    t13_row(what + ": every macOS leg runs", api, [])
    api.close()
first = sorted(T13_MAC)[0]
rest = [n for n in T13_MAC if n != first]
for what, jobs in (("missing", t13_green(drop=[first])), ("cancelled", t13_green(swap={first: t13_job(first, conclusion="cancelled")})),
                   ("listed twice, once red", t13_green(extra=[t13_job(first, conclusion="failure")])),
                   ("run on Linux", t13_green(swap={first: t13_job(first, labels=["ubuntu-24.04"])}))):
    api = T13API([t13_wrun(7008)], {7008: jobs})
    t13_row("%s %s: it runs, the green ones do not" % (first, what), api, rest)
    api.close()
api = T13API([t13_wrun(7009), t13_wrun(7010, conclusion="failure")], {7009: t13_green(drop=[first]), 7010: t13_green()})
t13_row("two runs at the commit, the red one with every leg: only the green one counts", api, rest)
api.close()
if rest:
    api = T13API([t13_wrun(7011), t13_wrun(7012, event="workflow_dispatch")], {7011: t13_green(drop=[first]), 7012: t13_green(drop=rest)})
    t13_row("two green runs, each with some legs: each leg relies on a run that has it", api, list(T13_MAC))
    api.close()
api = T13API([t13_wrun(7013)], {7013: t13_green()}, page_cap=2)
t13_row("a jobs listing over several pages is read to its count", api, list(T13_MAC))
pages = math.ceil(len(t13_green()) / 2)
say(pages >= 2 and len([p for p, _ in api.requests if "/jobs" in p]) == pages, "T13 the planner read every page (%d)" % pages)
api.close()
api = T13API([t13_wrun(7014)], {7014: t13_green()}, totals={7014: 999})
t13_row("a jobs listing short of its total_count is no evidence", api, [])
api.close()
api = T13API([t13_wrun(7015, sha="1" * 40), t13_wrun(7016, path=".github/workflows/other.yml")], {7015: t13_green(), 7016: t13_green()},
             ignore_filter=True)
t13_row("runs of another commit or another workflow, listed anyway, do not count", api, [])
api.close()
for where, kind in (("/repos/%s/actions/workflows" % T13_REPO, 500), ("/repos/%s/actions/runs" % T13_REPO, 403),
                    ("/repos/%s/actions/workflows" % T13_REPO, "html"), ("/repos/%s/actions/runs" % T13_REPO, "keys"),
                    ("/repos/%s/actions/workflows" % T13_REPO, 404)):
    api = T13API([t13_wrun(7017)], {7017: t13_green()}, fault=(where, kind))
    t13_row("the lookup answered %s at %s: every macOS leg runs" % (kind, where.rsplit("/", 1)[-1]), api, [])
    api.close()
api = T13API([t13_wrun(7018)], {7018: t13_green()}, fault=("/repos/%s/actions/workflows" % T13_REPO, "slow"))
t0 = time.time()
t13_row("the lookup timed out: every macOS leg runs", api, [], timeout=1)
say(time.time() - t0 < 30, "T13 the timeout holds (%.1f s)" % (time.time() - t0))
api.close()
elsewhere = T13API([t13_wrun(7019)], {7019: t13_green()})
api = T13API([t13_wrun(7019)], {7019: t13_green()}, fault=("/repos/%s/actions/workflows" % T13_REPO, "redirect"))
api.redirect_to = elsewhere.url
t13_row("the API redirected: not followed, every macOS leg runs", api, [])
say(elsewhere.requests == [], "T13 a redirect takes the token nowhere (the other host saw %d requests)" % len(elsewhere.requests))
api.close()
elsewhere.close()
api = T13API([t13_wrun(7020)], {7020: t13_green()})
t13_row("no token: every macOS leg runs", api, [], token=None)
t13_row("the checkout is not the pushed commit: every macOS leg runs", api, [], sha="2" * 40)
say(api.requests == [], "T13 without a token, or off the pushed commit, nothing is asked")
api.close()
# A labelled pull request's run hands its plan EVENT=push: only the runner's own
# event and ref decide. Neither that run, another branch's push, the nightly nor a
# dispatch asks.
for what, kw in (("a labelled pull request's run (EVENT=push)", dict(event_name="pull_request")),
                 ("a push to another branch", dict(ref="refs/heads/a-lane")),
                 ("the nightly", dict(event="schedule", event_name="schedule")),
                 ("a dispatch", dict(event="workflow_dispatch", event_name="workflow_dispatch"))):
    api = T13API([t13_wrun(7021)], {7021: t13_green()})
    got, err = t13_plan(api, **kw)
    say(api.requests == [] and "ERROR" not in got and got["covered"] == {} and not T13_LINE.search(err),
        "T13 %s asks nothing and leaves nothing" % what)
    api.close()
say(all("ERROR" not in p for p in T13_PLANS), "T13 every trunk push plan exits 0")
say(T13_SEEN and not [s for s in T13_SEEN if T13_TOKEN in s], "T13 the token is never printed")
shutil.rmtree(T13_DIR, ignore_errors=True)

# The verdict: a full run less the covered legs, each covered leg named.
T13_COVER = {n: {"job": T13_JOBS[n], "label": T13_MAC[n], "run": 7001, "event": "pull_request",
                 "sha": "a" * 40, "tree": "b" * 40} for n in T13_MAC}
for n, c in T13_COVER.items():
    c["leg"] = None if n in T13_WHOLE else next((l for l in T13_FULL.get("matrix", {}).get("matrix_" + c["job"].replace("-", "_"), [])
                                                 if t13_render(c["job"], l) == n), None)


def t13_needs(plan_, covered, ran=()):
    outs = {"tier": plan_["tier"], "jobs": json.dumps(plan_["jobs"]), "macos_covered": json.dumps(covered)}
    for k, v in plan_["matrix"].items():
        outs[k] = json.dumps(v, separators=(",", ":"))
    n = {"plan": {"result": "success", "outputs": outs}}
    for j in EVERY:
        n[j] = {"result": "success" if j in plan_["jobs"] or j in ran else "skipped", "outputs": {}}
    return n


left = t13_less(T13_FULL, list(T13_MAC))
rc, text = verdict(t13_needs(left, T13_COVER))
say(rc == 0 and all(("covered %s: green in pull_request run 7001 at %s, tree %s" % (n, "a" * 40, "b" * 40)) in text for n in T13_MAC),
    "T13 a full run less its covered macOS legs reads green and names each one's run%s" % ("" if rc == 0 else ": " + text.strip()[-300:]))
for what, needs in (
        ("the covered legs left without their cover", t13_needs(left, {})),
        ("a cover in a pr tier", t13_needs(dict(left, tier="pr"), T13_COVER)),
        ("a cover that names a push run", t13_needs(left, {n: dict(c, event="push") for n, c in T13_COVER.items()})),
        ("a cover with no run", t13_needs(left, {n: dict(c, run="x") for n, c in T13_COVER.items()})),
        ("a cover that is no object", dict(t13_needs(left, {}), plan={"result": "success", "outputs": dict(
            t13_needs(left, {})["plan"]["outputs"], macos_covered='["x"]')}))):
    rc, text = verdict(needs)
    say(rc == 1, "T13 the verdict says no to %s (exit %d)" % (what, rc))
linux = [(k, l) for k, v in T13_FULL["matrix"].items() for l in v if not l["os"].startswith("macos")]
if linux:
    k, l = linux[0]
    job = k[len("matrix_"):].replace("_", "-")
    name = t13_render(job, l)
    bad = dict(T13_COVER, **{name: {"job": job, "leg": l, "label": l["os"], "run": 7001, "event": "pull_request",
                                    "sha": "a" * 40, "tree": "b" * 40}})
    shorter = dict(left, matrix={kk: [x for x in vv if not (kk == k and x == l)] for kk, vv in left["matrix"].items()})
else:
    job = [j for j in left["jobs"] if j not in T13_WHOLE][0]
    bad = dict(T13_COVER, **{job: {"job": job, "leg": None, "label": "ubuntu-24.04", "run": 7001, "event": "pull_request",
                                   "sha": "a" * 40, "tree": "b" * 40}})
    shorter = dict(left, jobs=[j for j in left["jobs"] if j != job])
rc, _ = verdict(t13_needs(shorter, bad))
say(rc == 1, "T13 the verdict says no to a covered Linux leg or job (exit %d)" % rc)
rc, _ = verdict(t13_needs(T13_FULL, T13_COVER))
say(rc == 1, "T13 the verdict says no to a leg both covered and run (exit %d)" % rc)

# The wiring.
tb = dict(re.findall(r"^  ([A-Za-z0-9_-]+):\n(.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)", wf.split("\njobs:\n", 1)[1], re.M | re.S))
pb = tb.get("plan", "")
say("      macos_covered: ${{ steps.plan.outputs.macos_covered }}\n" in pb, "T13 the plan job declares macos_covered")
TREE_NAME = "      - name: the tree under test is ${{ steps.tree.outputs.tree }}\n"
say("        id: tree\n" in pb and TREE_NAME in pb and pb.index("        id: tree\n") < pb.index(TREE_NAME) < pb.index("      - id: plan\n")
    and "test \"$(git rev-parse 'HEAD^{tree}')\" = \"${{ steps.tree.outputs.tree }}\"" in pb,
    "T13 the plan job names the tree it tests in a step's name, checked, before the plan")
say(wf.count("PLAN_TOKEN") == 1 and re.search(r"^      - id: plan\n        if: [^\n]*\n        env:\n(          .*\n|          #.*\n)*          PLAN_TOKEN: \$\{\{ github\.token \}\}\n", pb, re.M) is not None,
    "T13 the plan's step, and no other, is given the workflow's token")
say(re.search(r"^    permissions:\n(      [a-z-]+: read\n)*      actions: read\n", pb, re.M) is not None
    and re.search(r"^    permissions:\n(      [a-z-]+: read\n)*      contents: read\n", pb, re.M) is not None
    and not re.search(r"^    permissions:\n(      [a-z-]+: (?!read)\S+\n)", pb, re.M),
    "T13 the plan job reads the repository and its runs, and writes nothing")
named = []
for n, label in T13_MAC.items():
    if n in T13_WHOLE:
        b = tb.get(n, "")
        named.append(("    runs-on: %s\n" % label) in b and not re.search(r"^    name: ", b, re.M))
    else:
        b = tb.get(T13_JOBS[n], "")
        named.append("    runs-on: ${{ matrix.os }}\n" in b and "    name: build + test (${{ matrix.os }}${{ matrix.leg && format(', {0}', matrix.leg) || '' }})\n" in b)
say(named and all(named), "T13 the macOS jobs a trunk push looks for are named as this workflow names them: %s" % sorted(T13_MAC))

print("tiers-test: %s" % ("PASS" if fails == 0 else "FAIL (%d)" % fails))
sys.exit(1 if fails else 0)
