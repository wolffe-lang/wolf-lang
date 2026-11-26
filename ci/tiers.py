#!/usr/bin/env python3
"""ci/tiers.py — which jobs a CI run runs, and whether it ran them (tl21).

    ci/tiers.py plan [--event E] [--action A] [--label L] [--labels L1,L2]
                     [--merge [--head-sha SHA] | --paths-from FILE]
                     [--jobs "J ..." | --tier full|release] [--repo DIR]
                     [--github-output FILE] [--json]
    ci/tiers.py verdict        (NEEDS: the `tier` job's `toJSON(needs)`)
    ci/tiers.py classify PATH...

`plan` options also read an environment variable of their name in capitals
(EVENT, ACTION, LABEL, LABELS, HEAD_SHA, IN_JOBS, IN_TIER, COVERED,
COVERED_BY, COVERED_AT, COVERED_RUN), which is how the workflow's `plan`
job passes the event in, and ci/covered.py's answer.

THE TIERS. A job is a job of .github/workflows/ci.yml other than `plan`,
`tier` and `nightly-red` (which runs after the verdict).

  release  every job: lint + test + corpus on linux, the macOS native
           gauntlet's three shards, the fuzz smoke, build + test on macOS
           and windows, the freebsd cross-check, the planner's own test.
           The nightly `schedule`, a `workflow_dispatch` with `tier`
           release, and a pull request carrying the `release-matrix` label
           (a release lane, before its tag: lane-audit.sh asks an rNN
           branch for a green `tier release` at the head it tags). A red
           nightly opens the `nightly-red` issue.
  full     the merge gate: every job but MERGE_OMITS, the macOS native
           gauntlet's three shards. `build + test (macos-latest)` is the
           merge's macOS smoke: the workspace built for aarch64-apple-darwin
           and every crate's tests, the signal and terminal lanes among
           them. A pull request carrying the `full-matrix` label, a push to
           trunk, a `workflow_dispatch` with neither `jobs` nor `tier`; and
           a pull request with a changed path no rule below names, or whose
           changed paths cannot be read: what is not understood never
           shrinks a run below the merge's. (tl30, the macOS cap: of the
           macOS jobs here only `build + test` and the native build shard
           caught a red no linux job did in the last 60 runs with one,
           s246's two, and `build + test` caught both; ci/tl30.md.)
  pr       any other pull-request run: PR_JOBS, the linux lint + test +
           corpus job (the whole gauntlet, `cargo xtask ci`), the fuzz
           smoke and the planner's test. No path picks among them: almost
           any change to a compiler can move any test.
  noop     nothing but the plan and the verdict: a pull request whose
           changed paths are ALL prose, a `labeled` event for a label other
           than `full-matrix` and `release-matrix`, and `full-matrix` added
           beside `release-matrix` (whose run covers it).
  partial  a `workflow_dispatch` that names jobs (`-f jobs="plan-test"`):
           those jobs alone, for a planted break. It is never `full` or
           `release`, whatever it names: `tier full` green says the merge's
           jobs ran, `tier release` every job.
  skipped  the nightly at a commit already green in full: ci/covered.py
           says the commit's latest `tier release` check run of this
           workflow is green, completed at most 144 hours before tonight's
           run. Nothing runs but the plan and the verdict; the plan's log,
           a warning on the run's page and the run's summary say so and
           name the covering run. The tier job is `tier skipped`, which
           ci/covered.py never counts, so a skipped night never covers the
           next one: a commit that does not move is tested in full at
           least every seventh night. No dispatch may ask for it.

PROSE IS NARROW (RULES, below): the CHANGELOG, CONTRIBUTING and the
hand-written pages of docs/. Not prose, because a gate or a test reads
it: everything under spec/ (spec-extract, anchors.json, prelude.json),
README.md (xtask's readme tests; the archive ships it), and the pages
xtask generates and holds in sync, docs/diagnostics.md, docs/warnings.md
and docs/api/. ci/tiers-test.py holds the classes, and trips when a test
starts reading a path this file calls prose.

`verdict` checks a finished run: exit 0 when the plan succeeded, the jobs
it chose are the tier's jobs, every chosen job succeeded and every other
job was skipped; 1 otherwise. GitHub leaves an EMPTY output out of
`needs`, so each is read with a default.

`plan` prints `tier=`, `jobs=` (a JSON list) and `covered_by=` (a skipped
night's covering check run; empty otherwise), to --github-output
(appended) and to standard output; with --json, one JSON object. The
reasons, one line per changed path, go to standard error, so a run's log
says why it has the jobs it has. Exit 0; 2 on a usage error.
"""
import argparse
import datetime
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORKFLOW = ".github/workflows/ci.yml"
FULL_LABEL = "full-matrix"
RELEASE_LABEL = "release-matrix"
# The plan, the verdict, and what runs after the verdict (the nightly's
# issue) are no tier's choice.
NOT_SELECTABLE = ("plan", "tier", "nightly-red")
# The jobs of a pull-request run. A job the workflow gains is the full
# tier's alone until it is named here.
PR_JOBS = ("plan-test", "lint-test", "fuzz-smoke")
# The jobs the merge tier leaves to the release tier (the nightly, a
# release lane's label): tl30, the macOS cap. The org's five macOS slots
# were busy 55% of the time after the Team plan (tl29), and the native
# gauntlet held three of them for 49 minutes at every merge, twice.
MERGE_OMITS = ("native-macos",)
TIERS = ("release", "full", "pr", "noop", "partial", "skipped")
# The nightly's own tier, and how long a green one at the same commit covers a night (ci/covered.py).
NIGHTLY = "release"
COVER_HOURS = 144
DISPATCH_TIERS = ("full", "release")

PROSE, PR, FULL = "prose", "pr", "full"

# A changed path's class: the first rule that matches decides. `*` stays
# inside one path segment; a trailing `/**` is everything below.
RULES = [
    # Generated by xtask and held in sync by its gates: never prose.
    ("docs/diagnostics.md", PR),
    ("docs/warnings.md", PR),
    ("docs/api/**", PR),
    # Read by xtask's readme tests, shipped in the archive.
    ("README.md", PR),
    # Prose.
    ("CHANGELOG.md", PROSE),
    ("CONTRIBUTING.md", PROSE),
    ("docs/*.md", PROSE),
    ("docs/release/*.md", PROSE),
    # The compiler, its corpus, its spec, its tools, and CI itself.
    ("crates/**", PR),
    ("corpus/**", PR),
    ("spec/**", PR),
    ("xtask/**", PR),
    ("fuzz/**", PR),
    ("bench/**", PR),
    ("ci/**", PR),
    ("docs/**", PR),
    # Lane evidence includes executable reproducers and test inputs.
    ("notes/**", PR),
    ("assets/**", PR),
    (".github/**", PR),
    (".cargo/**", PR),
    ("Cargo.toml", PR),
    ("Cargo.lock", PR),
    ("rust-toolchain.toml", PR),
    ("rustfmt.toml", PR),
    ("clippy.toml", PR),
    ("deny.toml", PR),
    ("LICENSE", PR),
    ("LICENSE-TRAINING-DATA", PR),
    (".gitignore", PR),
    (".gitattributes", PR),
    (".editorconfig", PR),
]


def die(msg):
    print("tiers: " + msg, file=sys.stderr)
    sys.exit(2)


def matches(glob, path):
    if glob.endswith("/**"):
        return path.startswith(glob[:-2])
    rx = "".join("[^/]*" if c == "*" else re.escape(c) for c in glob)
    return re.fullmatch(rx, path) is not None


def classify(path):
    """-> (class, the rule that decided)."""
    for glob, cls in RULES:
        if matches(glob, path):
            return cls, glob
    return FULL, "no rule names it"


def workflow_jobs():
    """The workflow's jobs, in order: {id: [the ids it needs]}."""
    jobs, cur, inside = {}, None, False
    with open(os.path.join(ROOT, WORKFLOW), encoding="utf-8") as f:
        for line in f.read().splitlines():
            if re.match(r"^jobs:\s*$", line):
                inside = True
            elif inside and re.match(r"^\S", line):
                inside = False
            elif inside:
                m = re.match(r"^  ([A-Za-z0-9_-]+):\s*$", line)
                if m:
                    cur = m.group(1)
                    jobs[cur] = []
                    continue
                m = re.match(r"^    needs:\s*(.*)$", line)
                if m and cur:
                    jobs[cur] = re.findall(r"[A-Za-z0-9_-]+", m.group(1))
    return jobs


def jobs_of(tier, every):
    """The jobs a tier runs, of `every` selectable job, in order."""
    if tier == "release":
        return list(every)
    if tier == "full":
        return [j for j in every if j not in MERGE_OMITS]
    if tier == "pr":
        return [j for j in every if j in PR_JOBS]
    return []


def merge_paths(repo, head_sha):
    """-> (the paths a pull request changes, how they were read): the
    checked-out merge commit against its first parent, the base it was
    merged into. (None, the reason) when that cannot be read."""
    def git(*args):
        return subprocess.run(["git", "-C", repo] + list(args), check=True,
                              capture_output=True, text=True).stdout

    try:
        parents = git("rev-list", "--parents", "-n", "1", "HEAD").split()
        if len(parents) != 3:
            return None, "HEAD has %d parent(s), not a pull request's merge commit" % (len(parents) - 1)
        if head_sha and parents[2] != head_sha:
            return None, "the merge's second parent %s is not the pull request's head %s" % (parents[2][:9], head_sha[:9])
        out = git("diff", "--name-only", "--no-renames", "-z", parents[1], parents[0])
    except (subprocess.CalledProcessError, FileNotFoundError, OSError) as e:
        return None, "git could not read the change (%s)" % e
    return [p for p in out.split("\0") if p], "%s..%s" % (parents[1][:9], parents[0][:9])


def plan(a):
    needs = workflow_jobs()
    every = [j for j in needs if j not in NOT_SELECTABLE]
    for j in PR_JOBS + MERGE_OMITS:
        if j not in every:
            die("PR_JOBS or MERGE_OMITS names %s, which is no job of %s" % (j, WORKFLOW))
    log = []

    def result(tier, chosen=None):
        chosen = jobs_of(tier, every) if chosen is None else [j for j in every if j in chosen]
        return {"tier": tier, "jobs": chosen, "log": log}

    labels = [x for x in (a.labels or "").split(",") if x]
    asked = (a.jobs or "").split()
    ev = a.event or "workflow_dispatch"
    log.append("event=%s action=%s label=%s labels=%s" % (ev, a.action or "-", a.label or "-", ",".join(labels) or "-"))

    if ev == "pull_request" and a.action == "labeled" and a.label not in (FULL_LABEL, RELEASE_LABEL):
        log.append("a label other than %s and %s was added: noop" % (FULL_LABEL, RELEASE_LABEL))
        return result("noop")
    if ev == "pull_request" and a.action == "labeled" and a.label == FULL_LABEL and RELEASE_LABEL in labels:
        log.append("%s was added beside %s, whose run covers it: noop" % (FULL_LABEL, RELEASE_LABEL))
        return result("noop")
    if ev == "workflow_dispatch" and asked and a.tier:
        die("--jobs and --tier: a dispatch names jobs (partial) or a tier, not both")
    if ev == "workflow_dispatch" and a.tier and a.tier not in DISPATCH_TIERS:
        die("--tier: %s is no tier a dispatch can ask for (%s)" % (a.tier, ", ".join(DISPATCH_TIERS)))
    if ev == "workflow_dispatch" and asked:
        for j in asked:
            if j not in every:
                die("--jobs: %s is no job of %s (the jobs: %s)" % (j, WORKFLOW, " ".join(every)))
        for j in asked:
            for n in needs[j]:
                if n not in NOT_SELECTABLE and n not in asked:
                    die("--jobs: %s needs %s; name both" % (j, n))
        log.append("dispatch naming %s: partial" % " ".join(asked))
        return result("partial", asked)
    if ev == "workflow_dispatch" and a.tier:
        log.append("dispatch asking for tier %s: %s" % (a.tier, a.tier))
        return result(a.tier)
    if ev == "schedule":
        if a.covered == NIGHTLY:
            try:
                due = (datetime.datetime.strptime(a.covered_at or "", "%Y-%m-%dT%H:%M:%SZ")
                       + datetime.timedelta(hours=COVER_HOURS)).strftime("%Y-%m-%dT%H:%M:%SZ")
            except ValueError:
                due = "?"
            line = ("the nightly: skipped, nothing tested tonight: tier %s green at this commit in run %s (%s), completed %s; "
                    "a full night is due by %s" % (NIGHTLY, a.covered_run or "?", a.covered_by or "?", a.covered_at or "?", due))
            log.append(line)
            r = result("skipped")
            r["covered_by"], r["notice"] = a.covered_by or "", line
            return r
        log.append("the nightly: release%s" % ("" if a.covered in (None, "", "none") else
                                               " (covered=%s is not tier %s: no skip)" % (a.covered, NIGHTLY)))
        return result("release")
    if ev != "pull_request":
        log.append("%s: full" % ev)
        return result("full")
    if RELEASE_LABEL in labels:
        log.append("the %s label: release" % RELEASE_LABEL)
        return result("release")
    if FULL_LABEL in labels:
        log.append("the %s label: full" % FULL_LABEL)
        return result("full")

    if a.paths_from:
        f = sys.stdin if a.paths_from == "-" else open(a.paths_from, encoding="utf-8")
        paths, how = [p.strip() for p in f.read().splitlines() if p.strip()], a.paths_from
    elif a.merge:
        paths, how = merge_paths(a.repo or ROOT, a.head_sha)
    else:
        paths, how = None, "no --merge and no --paths-from"
    if paths is None:
        log.append("the changed paths cannot be read (%s): full" % how)
        return result("full")
    if not paths:
        log.append("the change names no path (%s), which no pull request's does: full" % how)
        return result("full")

    count = {PROSE: 0, PR: 0, FULL: 0}
    for p in paths:
        cls, why = classify(p)
        count[cls] += 1
        log.append("%s -> %s  [%s]" % (p, cls, why))
    if count[FULL]:
        log.append("%d of %d path(s) no rule names: full" % (count[FULL], len(paths)))
        return result("full")
    if count[PR] == 0:
        log.append("%d path(s), every one prose: noop" % len(paths))
        return result("noop")
    log.append("%d path(s), %d prose: pr" % (len(paths), count[PROSE]))
    return result("pr")


def verdict(needs):
    """The `tier` job's check, from `toJSON(needs)`."""
    plan_job = needs.get("plan") or {}
    if plan_job.get("result") != "success":
        print("tier: the plan did not succeed (%s)" % plan_job.get("result"))
        return 1
    out = plan_job.get("outputs") or {}
    tier = out.get("tier", "")
    if tier not in TIERS:
        print("tier: the plan named no tier (%r)" % tier)
        return 1
    try:
        chosen = json.loads(out.get("jobs", "[]"))
        assert isinstance(chosen, list)
    except (ValueError, AssertionError):
        print("tier: the plan's list of jobs cannot be read (%r)" % out.get("jobs"))
        return 1
    every = [j for j in needs if j != "plan"]
    bad = 0
    try:
        covered = read_covered(out, tier, jobs_of(tier, every))
    except ValueError as e:
        print("tier: %s" % e)
        return 1
    want_jobs = [j for j in jobs_of(tier, every) if j not in [c["job"] for c in covered.values() if c["leg"] is None]]
    if tier == "skipped":
        print("tier skipped: nothing was tested this night; tier %s is green at this commit in %s" % (
            NIGHTLY, out.get("covered_by") or "the run the plan named"))
    if tier == "partial":
        if not chosen:
            print("tier: a partial run that chose no job")
            bad += 1
    elif sorted(chosen) != sorted(want_jobs):
        print("tier: the plan chose %s; tier %s is %s" % (
            " ".join(chosen) or "nothing", tier, " ".join(want_jobs) or "nothing"))
        bad += 1
    # Each matrix the plan handed out is the tier's, less the legs it left as
    # covered. (A plan that wrote none left none: the job then reads the default
    # and fails by itself.)
    for name, legs in sorted(less_covered(matrix_of(tier), covered).items()):
        if name not in out and not covered:
            continue
        try:
            got = json.loads(out.get(name, "null"))
        except ValueError:
            got = out.get(name)
        if got != legs:
            print("tier: the plan's %s is %s; tier %s runs %s" % (name, json.dumps(got), tier, json.dumps(legs)))
            bad += 1
    print("tier %s: %d of %d job(s) chosen" % (tier, len(chosen), len(every)))
    for name, c in sorted(covered.items()):
        print("covered %s: green in %s run %d at %s, tree %s" % (name, c["event"], c["run"], c["sha"], c["tree"]))
    for job in every:
        want = "success" if job in chosen else "skipped"
        got = (needs[job] or {}).get("result")
        ok = got == want
        bad += not ok
        print("  %-18s %-9s %s" % (job, got, "ok" if ok else "WANTED " + want))
    missing = [j for j in chosen if j not in every]
    if missing:
        print("tier: chosen, but the tier job does not need it: %s" % " ".join(missing))
        bad += 1
    return 1 if bad else 0


# --- build + test's legs, the plan's ---------------------------------------------
# Every leg of `build + test`, as the workflow had them (tl23, tl30, #1217): the
# macOS leg is the merge's macOS smoke; windows runs in two legs, each on its own
# runner and Rust cache. Each tier that chooses the job runs every leg; a trunk
# push may leave the macOS leg (below). `timeout` is each leg's limit in minutes,
# its longest healthy run in ci/job-limits.json plus a third, rounded up to 5.
TRUNK = "trunk"
MATRIX = {
    "build-test": [
        {"os": "macos-latest", "timeout": 60},
        {"os": "windows-latest", "leg": "tests", "timeout": 45},
        {"os": "windows-latest", "leg": "rung", "timeout": 45},
    ],
}
# A matrix job with a `name:` of its own: how GitHub names each leg.
LEG_NAMES = {"build-test": lambda leg: "build + test (%s%s)" % (leg["os"], ", " + leg["leg"] if leg.get("leg") else "")}


def output_name(job):
    return "matrix_" + job.replace("-", "_")


def matrix_of(tier):
    """{output name: a matrix job's legs}: every leg in every tier (a tier that
    does not choose the job skips it by its `if:`, so no matrix is empty)."""
    return {output_name(j): list(legs) for j, legs in MATRIX.items()}


def git(repo, *args):
    return subprocess.run(["git", "-C", repo] + list(args), check=True, capture_output=True, text=True).stdout


# --- a trunk push leaves the macOS legs already green at its commit ---------------
# Merges are fast-forwards, so a push to trunk runs the commit its pull request's
# run has just passed. On the runner's own push to trunk (GITHUB_EVENT_NAME and
# GITHUB_REF: a labelled pull request's run hands the plan EVENT=push, so EVENT
# alone never decides) the plan asks the REST API, with the workflow's own
# read-only token (PLAN_TOKEN), for this workflow's runs at GITHUB_SHA. It leaves a
# macOS leg, or a whole macOS job, only when a completed, successful pull_request
# or workflow_dispatch run there tested the same tree (its `plan` job's step `the
# tree under test is <tree>`: a pull request's run tests its merge with the base,
# which is this tree only when the base is already in the commit) and has, at its
# latest attempt, that job once, green, on its macOS runner. Anything else runs
# it: no such run, a lookup that fails, times out, redirects or answers oddly, a
# jobs listing short of its count, no token, a checkout that is not the pushed
# commit. A matrix job keeps at least one leg, and a job another chosen job needs
# is never left. The plan writes `macos_covered=` (each leg or job it left, with
# the run, event, commit and tree it relied on) and logs one line a kind; the
# verdict accepts exactly those in place of a green job and names the run.
EVIDENCE_EVENTS = ("pull_request", "workflow_dispatch")
TREE_STEP = re.compile(r"^the tree under test is ([0-9a-f]{40}|[0-9a-f]{64})$")
HEX = re.compile(r"[0-9a-f]{40}|[0-9a-f]{64}")
MAX_RUNS, MAX_PAGES = 10, 5


def leg_name(job, leg):
    """The name GitHub gives a leg of a matrix job that has no `name:`."""
    return "%s (%s)" % (job, ", ".join(str(v).lower() if isinstance(v, bool) else str(v) for v in leg.values()))


def render_leg(job, leg):
    """A leg's name as GitHub gives it: the job's own `name:` where it has one."""
    return LEG_NAMES[job](leg) if job in LEG_NAMES else leg_name(job, leg)


def job_hosts():
    """{job: (its literal `name:` or None, its literal `runs-on:` or None)}."""
    out, cur, inside = {}, None, False
    with open(os.path.join(ROOT, WORKFLOW), encoding="utf-8") as f:
        for line in f.read().splitlines():
            if re.match(r"^jobs:\s*$", line):
                inside = True
            elif inside and re.match(r"^\S", line):
                inside = False
            elif inside:
                m = re.match(r"^  ([A-Za-z0-9_-]+):\s*$", line)
                if m:
                    cur = m.group(1)
                    out[cur] = [None, None]
                    continue
                m = re.match(r"^    (name|runs-on):\s*(.*?)\s*$", line)
                if m and cur:
                    out[cur][0 if m.group(1) == "name" else 1] = m.group(2)
    return {j: tuple(v) for j, v in out.items()}


def macos_kinds(r):
    """{name: (job, leg or None, runner label)}: each macOS leg of a chosen matrix
    job with no `name:`, and each chosen job that runs on a literal macOS label."""
    hosts, kinds = job_hosts(), {}
    for job in r["jobs"]:
        name, runs_on = hosts.get(job, (None, None))
        if job in MATRIX:
            if name is None or job in LEG_NAMES:
                for leg in r["matrix"].get(output_name(job), []):
                    if str(leg.get("os", "")).startswith("macos"):
                        kinds[render_leg(job, leg)] = (job, leg, leg["os"])
        elif runs_on and runs_on.startswith("macos") and "${{" not in (name or "") + runs_on:
            kinds[name or job] = (job, None, runs_on)
    return kinds


class NoRedirect(urllib.request.HTTPRedirectHandler):
    """The plan follows no redirect: the token goes to the API alone."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise urllib.error.HTTPError(req.full_url, code, "a redirect, not followed", headers, fp)


class Api:
    def __init__(self, base, repo, token, timeout):
        self.base, self.repo, self.token, self.timeout = base.rstrip("/"), repo, token, timeout
        self.opener = urllib.request.build_opener(NoRedirect)

    def get(self, path, **query):
        url = "%s/repos/%s/%s?%s" % (self.base, self.repo, path, urllib.parse.urlencode(query))
        req = urllib.request.Request(url, headers={
            "Authorization": "Bearer " + self.token,
            "Accept": "application/vnd.github+json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "ci-tiers-plan",
        })
        with self.opener.open(req, timeout=self.timeout) as resp:
            return json.loads(resp.read().decode("utf-8"))


def run_jobs(api, run_id):
    """A run's jobs at its latest attempt, every page; None when the listing
    falls short of its own count."""
    jobs = []
    for page in range(1, MAX_PAGES + 1):
        d = api.get("actions/runs/%d/jobs" % run_id, filter="latest", per_page=100, page=page)
        got, total = d["jobs"], int(d["total_count"])
        jobs += got
        if len(jobs) >= total:
            return jobs if len(jobs) == total else None
        if not got:
            return None
    return None


def tested_tree(jobs):
    """The tree a run's `plan` job named, or None."""
    plan_job = [j for j in jobs if j.get("name") == "plan"]
    if len(plan_job) != 1 or plan_job[0].get("conclusion") != "success":
        return None
    trees = [m.group(1) for s in (plan_job[0].get("steps") or []) if s.get("conclusion") == "success"
             for m in [TREE_STEP.match(s.get("name") or "")] if m]
    return trees[0] if len(trees) == 1 else None


def green_kind(jobs, names, label):
    """Whether every job named is in the run once, green, on its macOS runner."""
    found = [[j for j in jobs if j.get("name") == n] for n in names]
    return all(len(f) == 1 and f[0].get("conclusion") == "success" and label in (f[0].get("labels") or []) for f in found)


def macos_green(kinds, repo, log):
    """-> ({name: the run it is green in}, sha, tree); nothing on any doubt."""
    env = os.environ
    token = env.get("PLAN_TOKEN") or ""
    hide = lambda s: s.replace(token, "<token>") if token else s
    sha, slug = env.get("GITHUB_SHA") or "", env.get("GITHUB_REPOSITORY") or ""
    if not token:
        log.append("no token for the lookup: every macOS leg runs")
        return {}, sha, ""
    if not HEX.fullmatch(sha) or not slug:
        log.append("no pushed commit or repository named: every macOS leg runs")
        return {}, sha, ""
    try:
        head, tree = git(repo, "rev-parse", "HEAD").strip(), git(repo, "rev-parse", "HEAD^{tree}").strip()
    except (subprocess.CalledProcessError, FileNotFoundError, OSError) as e:
        log.append("git could not name the checkout (%s): every macOS leg runs" % type(e).__name__)
        return {}, sha, ""
    if head != sha:
        log.append("the checkout %s is not the pushed commit %s: every macOS leg runs" % (head, sha))
        return {}, sha, ""
    log.append("the pushed commit %s, tree %s" % (sha, tree))
    try:
        api = Api(env.get("GITHUB_API_URL") or "https://api.github.com", slug, token,
                  float(env.get("PLAN_API_TIMEOUT") or 20))
        listing = api.get("actions/workflows/%s/runs" % os.path.basename(WORKFLOW), head_sha=sha, per_page=100)
        runs = [x for x in listing["workflow_runs"]
                if x.get("head_sha") == sha and x.get("path") == WORKFLOW and x.get("status") == "completed"
                and x.get("conclusion") == "success" and x.get("event") in EVIDENCE_EVENTS]
        runs.sort(key=lambda x: -int(x["id"]))
        found = {}
        for x in runs[:MAX_RUNS]:
            rid = int(x["id"])
            jobs = run_jobs(api, rid)
            if jobs is None:
                log.append("%s run %d: its jobs listing falls short of its count, not relied on" % (x["event"], rid))
                continue
            t = tested_tree(jobs)
            if t != tree:
                log.append("%s run %d tested tree %s, not %s: not relied on" % (x["event"], rid, t or "(not named)", tree))
                continue
            for name, (_, _, label) in kinds.items():
                if name not in found and green_kind(jobs, [name], label):
                    found[name] = x
        return found, sha, tree
    except Exception as e:  # any failure of the lookup runs every macOS leg
        log.append("the lookup failed (%s: %s): every macOS leg runs" % (type(e).__name__, hide(str(e))))
        return {}, sha, ""


def trunk_cover(r, a):
    """The plan less the macOS legs and jobs already green at this commit, on the
    runner's own push to trunk; the plan as it was otherwise."""
    r["macos_covered"] = {}
    env = os.environ
    if not (a.event == "push" and env.get("GITHUB_EVENT_NAME") == "push" and env.get("GITHUB_REF") == "refs/heads/" + TRUNK):
        return r
    if r["tier"] != "full":
        r["log"].append("tier %s: no macOS leg is left" % r["tier"])
        return r
    kinds = macos_kinds(r)
    found, sha, tree = macos_green(kinds, a.repo or ROOT, r["log"])
    needs = workflow_jobs()
    covered, why = {}, {}
    for name, (job, leg, label) in kinds.items():
        x = found.get(name)
        if x is None:
            why[name] = ("no completed, successful pull_request or workflow_dispatch run of %s at this commit "
                         "tested this tree with it green on %s" % (os.path.basename(WORKFLOW), label))
            continue
        if leg is not None:
            left = [l for l in r["matrix"][output_name(job)] if render_leg(job, l) not in found]
            if not left:
                why[name] = "green in %s run %d, but %s would be left no leg" % (x["event"], int(x["id"]), job)
                continue
        elif [j for j in r["jobs"] if j != job and job in needs.get(j, [])]:
            why[name] = "green in %s run %d, but a chosen job needs %s" % (x["event"], int(x["id"]), job)
            continue
        covered[name] = {"job": job, "leg": leg, "label": label, "run": int(x["id"]), "event": x["event"],
                         "sha": sha, "tree": tree}
    for name, c in covered.items():
        if c["leg"] is None:
            r["jobs"] = [j for j in r["jobs"] if j != c["job"]]
        else:
            out = output_name(c["job"])
            r["matrix"][out] = [l for l in r["matrix"][out] if l != c["leg"]]
    server, slug = env.get("GITHUB_SERVER_URL") or "https://github.com", env.get("GITHUB_REPOSITORY") or ""
    lines = []
    for name in kinds:
        c = covered.get(name)
        if c:
            lines.append("skip %s: green in %s run %d at %s, tree %s (%s/%s/actions/runs/%d)" % (
                name, c["event"], c["run"], c["sha"], c["tree"], server, slug, c["run"]))
        else:
            lines.append("keep %s: %s" % (name, why[name]))
    r["log"] += lines
    summary = env.get("GITHUB_STEP_SUMMARY")
    if summary and lines:
        try:
            with open(summary, "a", encoding="utf-8") as f:
                f.write("### The trunk push's macOS legs\n\n" + "".join("- %s\n" % l for l in lines) + "\n")
        except OSError as e:
            r["log"].append("the step summary could not be written (%s)" % e)
    r["macos_covered"] = covered
    return r


def read_covered(out, tier, want_jobs):
    """The plan's `macos_covered`, checked: only in tier full, only a macOS leg of
    the tier's matrix or a whole macOS job of the tier, each named as GitHub names
    it, with the run, event, commit and tree it relied on."""
    try:
        covered = json.loads(out.get("macos_covered") or "{}")
    except ValueError:
        raise ValueError("the plan's macos_covered cannot be read (%r)" % out.get("macos_covered"))
    if not isinstance(covered, dict):
        raise ValueError("the plan's macos_covered is no object (%r)" % out.get("macos_covered"))
    hosts, matrix = job_hosts(), matrix_of(tier)
    for name, c in covered.items():
        if tier != "full":
            raise ValueError("%s covered in tier %s: only the full tier leaves a macOS leg" % (name, tier))
        if not isinstance(c, dict) or not isinstance(c.get("run"), int) or c.get("event") not in EVIDENCE_EVENTS \
                or not all(HEX.fullmatch(str(c.get(k) or "")) for k in ("sha", "tree")):
            raise ValueError("%s covered with no run it relied on (%r)" % (name, c))
        job, leg = c.get("job"), c.get("leg")
        if job not in want_jobs:
            raise ValueError("%s covered, but %s is no job of tier %s" % (name, job, tier))
        if leg is not None:
            ok = (job in MATRIX and (hosts.get(job, (None,))[0] is None or job in LEG_NAMES)
                  and leg in matrix.get(output_name(job), [])
                  and str(leg.get("os", "")).startswith("macos") and render_leg(job, leg) == name)
        else:
            jname, runs_on = hosts.get(job, (None, None))
            ok = job not in MATRIX and bool(runs_on) and runs_on.startswith("macos") and (jname or job) == name
        if not ok:
            raise ValueError("%s covered, but it is no macOS leg or job of tier %s" % (name, tier))
    return covered


def less_covered(matrix, covered):
    """A tier's matrices less the legs the plan left as covered."""
    gone = [(c["job"], c["leg"]) for c in covered.values() if c.get("leg") is not None]
    return {name: [l for l in legs if not any(output_name(j) == name and l == leg for j, leg in gone)]
            for name, legs in matrix.items()}


def main():
    argv = sys.argv[1:]
    if argv == ["verdict"]:
        sys.exit(verdict(json.loads(os.environ["NEEDS"])))
    if argv[:1] == ["classify"]:
        for p in argv[1:]:
            print("%s\t%s\t%s" % ((p,) + classify(p)))
        return
    if argv[:1] != ["plan"]:
        die("usage: ci/tiers.py plan [options] | verdict | classify PATH...")
    env = os.environ.get
    ap = argparse.ArgumentParser(prog="ci/tiers.py plan")
    ap.add_argument("--event", default=env("EVENT"))
    ap.add_argument("--action", default=env("ACTION"))
    ap.add_argument("--label", default=env("LABEL"))
    ap.add_argument("--labels", default=env("LABELS"))
    ap.add_argument("--head-sha", default=env("HEAD_SHA"))
    ap.add_argument("--jobs", default=env("IN_JOBS"))
    ap.add_argument("--tier", default=env("IN_TIER") or None)
    ap.add_argument("--covered", default=env("COVERED"))
    ap.add_argument("--covered-by", default=env("COVERED_BY"))
    ap.add_argument("--covered-at", default=env("COVERED_AT"))
    ap.add_argument("--covered-run", default=env("COVERED_RUN"))
    ap.add_argument("--merge", action="store_true")
    ap.add_argument("--paths-from")
    ap.add_argument("--repo")
    ap.add_argument("--github-output")
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args(argv[1:])
    r = plan(a)
    r["matrix"] = matrix_of(r["tier"])
    r = trunk_cover(r, a)
    for line in r["log"]:
        print("tiers: " + line, file=sys.stderr)
    if a.json:
        print(json.dumps({k: r[k] for k in ("tier", "jobs", "matrix", "macos_covered")}))
        return
    if r.get("notice"):
        # A skipped night must not read as a tested one: a warning on the run's page, and the run's summary.
        print("::warning title=nightly skipped::%s" % r["notice"])
        summary = os.environ.get("GITHUB_STEP_SUMMARY")
        if summary:
            with open(summary, "a", encoding="utf-8") as f:
                f.write("**Nightly skipped.** %s\n" % r["notice"])
    out = "tier=%s\njobs=%s\ncovered_by=%s\n" % (r["tier"], json.dumps(r["jobs"]), r.get("covered_by", ""))
    for name, legs in r["matrix"].items():
        out += "%s=%s\n" % (name, json.dumps(legs, separators=(",", ":")))
    out += "macos_covered=%s\n" % json.dumps(r["macos_covered"], separators=(",", ":"))
    sys.stdout.write(out)
    if a.github_output:
        with open(a.github_output, "a", encoding="utf-8") as f:
            f.write(out)


if __name__ == "__main__":
    main()
