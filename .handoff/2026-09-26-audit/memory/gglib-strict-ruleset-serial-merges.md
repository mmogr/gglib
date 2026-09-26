---
name: gglib-strict-ruleset-serial-merges
description: "gglib's ruleset requires a PR up to date with main, so merges go one at a time; how a reviewed PR is rebased without losing its PASS, and how a stack child is based on unmerged parents"
metadata:
  node_type: memory
  type: project
  originSessionId: d2f123ee-56ab-4c4c-a881-dbf2eef763a6
  modified: 2026-09-25T18:26:05.855Z
---

gglib's ruleset "ggRules" sets `strict_required_status_checks_policy=true`: a PR must be up to date with main to merge. modelpipe, modelpipe-ffi and ggchat are not strict. All four squash-merge. Never bypass the ruleset.

**Why:** found 2026-09-25 when #1130 showed BEHIND during the audit-fix execution ([[gglib-audit-fixes-execution]]).

**How to apply:**
- gglib merges go serially: rebase onto origin/main, wait for CI, merge, next.
- A reviewer PASS carries over a rebase only when `git range-diff old-base..old origin/main..new` shows `=` for every commit. Anything else goes back to a reviewer.
- A stack child waits locally until its parent squash-merges, then replays with `git rebase --onto origin/main <old-parent-tip>`.
- When a child renames files that several open PRs edit, base it on a local integration branch (origin/main plus those PRs cherry-picked), then replay `--onto origin/main <integration-tip>` after they all merge. D1 used `tmp/d1-base` this way on 2026-09-26.
- `git cherry-pick` has no `-q`. In zsh, `$VAR:c...` is a history modifier, so write `${VAR}:path` or use `bash -c`. See [[zsh-traps-in-the-bash-tool]].
- A watcher started with a plain `&` inside the Bash tool never notifies. Use `run_in_background`.
