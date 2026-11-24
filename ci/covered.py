#!/usr/bin/env python3
"""ci/covered.py — whether tonight's nightly may be skipped: its commit is already green in full.

    ci/covered.py [--github-output FILE]

    REPO, SHA, TIER (the nightly's own tier) and RUN_ID (tonight's run) come from the environment; `gh` on PATH,
    with a token that reads checks and actions. The event is EVENT, or else GITHUB_EVENT_NAME.

Only the nightly (`schedule`) is ever covered; for any other event this answers `none` without asking GitHub.
For the nightly it asks for every check run at SHA named `tier TIER` (the `tier` job of .github/workflows/ci.yml
names itself for the tier its plan chose) and keeps those whose app is GitHub Actions and whose workflow run is
one of .github/workflows/ci.yml at that same commit, other than tonight's own run (a rerun of a green night
runs again). The most recent of them to complete, across every run and
every attempt, has the last word: the night is covered when it succeeded and completed at most 144 hours before
tonight's run was created. A skipped night's own tier job is `tier skipped`, never `tier TIER`, so a skipped
night never covers the next one, and a commit that does not move is tested in full at least every seventh night.

It prints

    covered=TIER     with covered_by= (that check run's address), covered_at= (when it completed) and
                     covered_run= (its workflow run's id)
    covered=none     otherwise, or when the question cannot be answered (an error, an answer that is not JSON,
                     more check runs or workflow runs than one page holds)

and reason=, one line, to standard output and, with --github-output, to that file. Exit 0 when it answered;
2 when REPO, SHA, TIER or RUN_ID is missing.
"""
import datetime
import json
import os
import subprocess
import sys
import urllib.parse

WORKFLOW = ".github/workflows/ci.yml"
WINDOW = datetime.timedelta(hours=144)
PAGE = 100


class Unanswered(Exception):
    pass


def gh(path):
    try:
        r = subprocess.run(["gh", "api", path], capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired) as e:
        raise Unanswered("gh api %s: %s" % (path.split("?")[0], e))
    if r.returncode != 0:
        raise Unanswered("gh api %s: exit %d: %s" % (path.split("?")[0], r.returncode, r.stderr.strip()[-200:]))
    try:
        return json.loads(r.stdout)
    except ValueError:
        raise Unanswered("gh api %s: an answer that is not JSON" % path.split("?")[0])


def when(s):
    return datetime.datetime.strptime(s, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=datetime.timezone.utc)


def answer(repo, sha, tier, run_id, event):
    """-> (covered, covered_by, covered_at, covered_run, reason)."""
    if event != "schedule":
        return "none", "", "", "", "not the nightly (%s): only a schedule is ever skipped" % (event or "no event")
    tonight = gh("repos/%s/actions/runs/%s" % (repo, run_id))
    created = when(tonight["created_at"])
    name = "tier " + tier
    cr = gh("repos/%s/commits/%s/check-runs?check_name=%s&filter=all&per_page=%d" % (repo, sha, urllib.parse.quote(name), PAGE))
    checks = cr.get("check_runs") or []
    if (cr.get("total_count") or 0) >= PAGE or len(checks) >= PAGE:
        return "none", "", "", "", "more %s check runs at this commit than one page holds" % name
    checks = [c for c in checks if c.get("name") == name and c.get("status") == "completed" and c.get("completed_at")
              and (c.get("app") or {}).get("slug") == "github-actions"]
    if not checks:
        return "none", "", "", "", "no completed %s check run of GitHub Actions at this commit" % name
    wr = gh("repos/%s/actions/runs?head_sha=%s&per_page=%d" % (repo, sha, PAGE))
    runs = wr.get("workflow_runs") or []
    if (wr.get("total_count") or 0) >= PAGE or len(runs) >= PAGE:
        return "none", "", "", "", "more workflow runs at this commit than one page holds"
    ours = {r.get("check_suite_id"): r for r in runs
            if (r.get("path") or "").split("@")[0] == WORKFLOW and r.get("head_sha") == sha
            and r.get("check_suite_id") != tonight.get("check_suite_id")}
    checks = [c for c in checks if (c.get("check_suite") or {}).get("id") in ours]
    if not checks:
        return "none", "", "", "", "no %s check run at this commit is of %s in a run other than tonight's" % (name, WORKFLOW)
    last = max(checks, key=lambda c: c["completed_at"])
    run = ours[last["check_suite"]["id"]]
    at = last["completed_at"]
    if last.get("conclusion") != "success":
        return "none", "", "", "", "the latest %s at this commit is %s: %s" % (name, last.get("conclusion"), last.get("html_url", ""))
    if created - when(at) > WINDOW:
        return "none", "", "", "", "the latest %s at this commit completed %s, more than 144 hours before tonight's run" % (name, at)
    return tier, last.get("html_url", ""), at, str(run.get("id", "")), "%s green at this commit in run %s, completed %s" % (
        name, run.get("id", "?"), at)


def main():
    args = sys.argv[1:]
    out = None
    if args[:1] == ["--github-output"] and len(args) == 2:
        out = args[1]
    elif args:
        print("usage: ci/covered.py [--github-output FILE]", file=sys.stderr)
        return 2
    env = os.environ.get
    repo, sha, tier, run_id = env("REPO", ""), env("SHA", ""), env("TIER", ""), env("RUN_ID", "")
    if not (repo and sha and tier and run_id):
        print("covered: REPO, SHA, TIER and RUN_ID must be set", file=sys.stderr)
        return 2
    try:
        covered, by, at, run, reason = answer(repo, sha, tier, run_id, env("EVENT") or env("GITHUB_EVENT_NAME", ""))
    except (Unanswered, KeyError, TypeError, ValueError) as e:
        covered, by, at, run, reason = "none", "", "", "", "unanswered: %s" % e
    print("covered: %s at %s: covered=%s (%s)" % (repo, sha[:12], covered, reason), file=sys.stderr)
    text = "covered=%s\ncovered_by=%s\ncovered_at=%s\ncovered_run=%s\nreason=%s\n" % (covered, by, at, run, reason.replace("\n", " "))
    sys.stdout.write(text)
    if out:
        with open(out, "a", encoding="utf-8") as f:
            f.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
