#!/bin/bash
# Usage: merge_pr.sh <repo> <number> [--dry-run]
# Squash-merges one reviewed PR after re-checking the owner's rules. Prints why it refuses.
set -u
repo=$1; n=$2; dry=${3:-}
R=mmogr/$repo
j=$(gh pr view "$n" --repo "$R" --json number,title,body,isDraft,baseRefName,mergeable,mergeStateStatus,state,statusCheckRollup,commits,headRefOid) || { echo "REFUSE $repo#$n: cannot read"; exit 2; }
fail() { echo "REFUSE $repo#$n: $*"; exit 1; }
[ "$(jq -r .state <<<"$j")" = OPEN ] || fail "not open"
[ "$(jq -r .isDraft <<<"$j")" = false ] || fail "draft"
[ "$(jq -r .baseRefName <<<"$j")" = main ] || fail "base is not main"
title=$(jq -r .title <<<"$j")
if jq -r '.title + "\n" + .body' <<<"$j" | grep -qiE 'co-authored|claude|generated with|signed-off|🤖'; then fail "attribution text in title/body"; fi
# every commit authored and committed by Matt, no attribution in messages
bad=$(gh api "repos/$R/pulls/$n/commits" --paginate --jq '.[] | select(.commit.author.email!="sewing.wader8c@icloud.com" or .commit.committer.email!="sewing.wader8c@icloud.com" or (.commit.message|test("(?i)co-authored|claude-session|signed-off-by|generated with|🤖"))) | .sha[0:8]')
[ -z "$bad" ] || fail "commits with a foreign identity or attribution: $bad"
# checks: every check completed and none failed
# keep only the latest run of each check (a re-run supersedes an earlier failure)
latest='[.statusCheckRollup | group_by(.name // .context)[] | sort_by(.startedAt // .completedAt // "") | last]'
pending=$(jq "$latest | map(select((.status // \"COMPLETED\") != \"COMPLETED\")) | length" <<<"$j")
failed=$(jq -r "$latest | map(select((.conclusion // .state // \"\") | test(\"FAILURE|ERROR|CANCELLED|TIMED_OUT|ACTION_REQUIRED\"))) | map(.name // .context) | join(\",\")" <<<"$j")
[ "$pending" = 0 ] || fail "$pending checks still running"
[ -z "$failed" ] || fail "failed checks: $failed"
ms=$(jq -r .mergeStateStatus <<<"$j")
case "$ms" in CLEAN|HAS_HOOKS|UNSTABLE) ;; *) fail "merge state $ms (mergeable=$(jq -r .mergeable <<<"$j"))";; esac
if [ "$dry" = --dry-run ]; then echo "OK (dry run) $repo#$n: $title"; exit 0; fi
gh pr merge "$n" --repo "$R" --squash --subject "$title (#$n)" --body "" --match-head-commit "$(jq -r .headRefOid <<<"$j")" || fail "gh pr merge failed"
sha=$(gh pr view "$n" --repo "$R" --json mergeCommit --jq .mergeCommit.oid)
echo "MERGED $repo#$n -> ${sha:0:8}: $title"
