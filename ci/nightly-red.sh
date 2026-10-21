#!/usr/bin/env bash
# ci/nightly-red.sh — a red nightly opens an issue (tl28, ruling #92; brought
# to wolf-lang by tl30, the macOS cap, with the release tier it reports on).
#
#   REPO=owner/name WORKFLOW=<the workflow's name> RUN_URL=<the run> \
#     GH_TOKEN=... ci/nightly-red.sh
#
# Run by a workflow's `nightly-red` job when a SCHEDULED run on trunk is red,
# so the next lane on the repository owes it (the crawl's rule). One open
# issue per workflow: its title is the dedupe key. An open issue carrying
# that title gets a comment naming this run; otherwise one is opened with
# the `nightly-red` label (created here if the repository lacks it). A week
# of red nights is one issue with seven comments; closing it by hand, once a
# nightly is green again, is what resets it.
#
# ci/tiers-test.py runs this against a stand-in `gh` (T8): no open issue,
# one open issue, a label that already exists.
set -euo pipefail
: "${REPO:?REPO (owner/name) is unset}" "${WORKFLOW:?WORKFLOW is unset}" "${RUN_URL:?RUN_URL is unset}"

label=nightly-red
title="nightly red: $WORKFLOW on trunk"
body=$(printf 'The scheduled run of `%s` on trunk is red: %s\n\nThe next lane on this repository owes it (crawl 54, ruling #92). A further red night comments here; close this issue once a nightly is green again.\n' "$WORKFLOW" "$RUN_URL")

# The label may exist already; that is not a failure.
gh label create "$label" --repo "$REPO" --color B60205 \
  --description "a scheduled run on trunk is red; the next lane on the repo owes it" >/dev/null 2>&1 || true

existing=$(gh issue list --repo "$REPO" --state open --label "$label" \
  --search "in:title \"$title\"" --json number,title \
  --jq "map(select(.title == \"$title\")) | .[0].number // empty")
if [ -n "$existing" ]; then
  echo "nightly-red: commenting on #$existing ($title)"
  gh issue comment "$existing" --repo "$REPO" --body "$body"
else
  echo "nightly-red: opening \"$title\""
  gh issue create --repo "$REPO" --title "$title" --label "$label" --body "$body"
fi
