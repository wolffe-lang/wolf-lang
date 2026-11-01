#!/usr/bin/env bash
# s274 item 4: which bounded-memory rows have reddened on CI? The failed jobs of the
# last N failed ci.yml runs, each job's log grepped for the rows' failure messages.
set -uo pipefail
repo=wolffe-lang/wolf-lang
N=${1:-80}
out=/Users/mfwolffe/lanes/s274/scan
pat='a region gave back less|the per-turn region keeps|kept-str loop grew|no-region control grew|B an entry \(bound|scalar loop grew|the control without a region keeps|three doublings moved|resident memory grew|KiB of RSS per parked task|n=8192 peaked'
gh run list -R $repo --workflow ci.yml --status failure --limit "$N" --json databaseId,headSha,headBranch,event,createdAt \
  -q '.[] | "\(.databaseId) \(.createdAt) \(.event) \(.headBranch) \(.headSha[0:8])"' > "$out/failed-runs.txt"
# Runs a re-run turned green: their earlier attempts' failed jobs (filter=all below).
gh run list -R $repo --workflow ci.yml --limit 300 --json databaseId,headSha,headBranch,event,createdAt,attempt \
  -q '.[] | select(.attempt > 1) | "\(.databaseId) \(.createdAt) \(.event) \(.headBranch) \(.headSha[0:8])"' >> "$out/failed-runs.txt"
sort -u -o "$out/failed-runs.txt" "$out/failed-runs.txt"
printf 'S274_SCAN runs=%s %s\n' "$(wc -l < "$out/failed-runs.txt")" "$(date -u +%FT%TZ)"
: > "$out/hits.txt"
while read -r run created event branch sha; do
  jobs=$(gh api "repos/$repo/actions/runs/$run/jobs?per_page=100&filter=all" -q '.jobs[] | select(.conclusion=="failure") | "\(.id)\t\(.name)"' 2>/dev/null)
  [ -n "$jobs" ] || continue
  while IFS=$'\t' read -r job name; do
    log=$(gh api --allow-escape-sequences "repos/$repo/actions/jobs/$job/logs" 2>/dev/null)
    hits=$(printf '%s\n' "$log" | grep -a -o -E "($pat)[^\"]{0,90}" | sort -u)
    printf 'S274_JOB run=%s job=%s %s %s %s %s name=%s hits=%s\n' "$run" "$job" "$created" "$event" "$branch" "$sha" "$name" "$(printf '%s' "$hits" | grep -c .)"
    [ -n "$hits" ] && printf '%s\n' "$hits" | sed "s/^/run=$run job=$job $created $branch $sha [$name] /" >> "$out/hits.txt"
  done <<< "$jobs"
done < "$out/failed-runs.txt"
printf 'S274_SCAN_DONE %s hits=%s\n' "$(date -u +%FT%TZ)" "$(wc -l < "$out/hits.txt")"
