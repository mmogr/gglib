#!/bin/bash
# Usage: watch_prs.sh repo:N [repo:N ...]  — exits when at least one PR has every check completed; prints its state.
while true; do
  done_any=0
  for x in "$@"; do
    repo=${x%%:*}; n=${x##*:}
    j=$(gh pr view "$n" --repo "mmogr/$repo" --json state,statusCheckRollup 2>/dev/null) || continue
    st=$(jq -r .state <<<"$j")
    total=$(jq '[.statusCheckRollup | group_by(.name // .context)[] | last] | length' <<<"$j")
    pend=$(jq '[.statusCheckRollup | group_by(.name // .context)[] | sort_by(.startedAt // "") | last | select((.status // "COMPLETED") != "COMPLETED")] | length' <<<"$j")
    fails=$(jq -r '[.statusCheckRollup | group_by(.name // .context)[] | sort_by(.startedAt // "") | last | select((.conclusion // .state // "") | test("FAILURE|ERROR|CANCELLED|TIMED_OUT"))] | map(.name // .context) | join(",")' <<<"$j")
    if [ "$st" != OPEN ] || { [ "$total" -gt 0 ] && [ "$pend" = 0 ]; }; then echo "$repo#$n state=$st checks=$total failed=[$fails]"; done_any=1; fi
  done
  [ $done_any = 1 ] && exit 0
  sleep 60
done
