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
      matrix, at least its longest healthy run in ci/job-limits.json plus
      a third, rounded up to the next 5 minutes; a leg with no record, or a
      record with no leg, fails too (#1217). The check is shown to say no
      to a limit under that bound and to pass one at it.

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
say(re.findall(r"^    runs-on: (.*)$", blocks["build-test"], re.M) == ["${{ matrix.os }}"] and "          - os: macos-latest\n" in blocks["build-test"],
    "T4 the merge keeps a macOS smoke: build + test on macos-latest (tl30)")
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


def leg_limits(block):
    """{leg name: limit in minutes} for a matrix job: the matrix's `timeout` when the job reads it, else the job's own."""
    own = re.search(r"^    timeout-minutes: (.+)$", block, re.M)
    own = own.group(1).strip() if own else ""
    legs = {}
    for entry in re.findall(r"^          - (os: .*\n(?:            .*\n)*)", block, re.M):
        keys = dict(re.findall(r"(\w+): (.+)", entry))
        name = keys["os"].strip() + (", " + keys["leg"].strip() if "leg" in keys else "")
        value = keys.get("timeout", "").strip() if own == "${{ matrix.timeout }}" else own
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
got = leg_limits(blocks.get("build-test", ""))
probs = limit_problems(got, record)
says_no = (limit_floor(44.6) == 60 and limit_floor(30.0) == 40 and limit_floor(15.1) == 25
           and limit_problems({"a": 59}, {"a": {"longest_minutes": 44.6}})
           and not limit_problems({"a": 60}, {"a": {"longest_minutes": 44.6}})
           and limit_problems({"a": 60, "b": 60}, {"a": {"longest_minutes": 44.6}}))
say(bool(got) and not probs and says_no,
    "T12 each build + test leg's limit is at least its longest healthy run plus a third, rounded up to 5 (%s)%s%s"
    % (", ".join("%s %s" % (k, v) for k, v in sorted(got.items())) or "no legs",
       ("; " + "; ".join(probs)) if probs else "", "" if says_no else "; the check does not say no to a limit under the rule"))

print("tiers-test: %s" % ("PASS" if fails == 0 else "FAIL (%d)" % fails))
sys.exit(1 if fails else 0)
