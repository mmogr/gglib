---
name: reviewers-must-not-share-a-working-tree
description: "2026-09-21: two review subagents mutated one gglib checkout at once and scored each other's mutations; git status even read clean. Isolate any mutating agent in its own worktree"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: cae7f903-f776-4c1f-93a7-0e1784b5e40b
  modified: 2026-09-21T08:06:17.710Z
---

Reviewing gglib PR 3, one lens was told to mutate and run `cargo test` while
three other lenses ran in the same checkout. A reviewer watching the tree
recorded, over three minutes: one agent's mutation baked into
`target/debug/deps/…`, a *different* agent's mutation on disk, and then a
restore — with `git status --porcelain` reporting **clean** through part of it,
because git's stat cache had not been invalidated. Only `touch` made the `M`
appear.

So a `cargo test` or `git status` sampled in that window is a claim about a
tree nobody was standing in, and a mutation "surviving" may mean another agent
restored the file first.

Earlier in the same session a reviewer's mutation of
`gglib-proxy/tests/fixtures/tunnel.rs` was left behind entirely and a
`git add -A` swept `backend_auth = None; // MUTATION` into the commit. Caught
only because the next round was asked to diff the file list against
`origin/main`.

**Why:** mutation testing is a write to a shared resource, and subagents run
concurrently by default. Nothing in the harness serialises them, and the usual
"is the tree clean" check is exactly the one that lies here.

**How to apply:**
- Give any agent that mutates `isolation: 'worktree'`, or make it the only
  agent in its phase. Tell every other lens in that workflow to verify by
  reading and not to run cargo.
- Brief mutating agents to restore by rewriting the bytes *and* refreshing the
  mtime, and to end with `touch <file> && git status --porcelain` empty.
- Before believing any gate or mutation result, re-run it in a quiet tree and
  check the log's first line names the commit with `dirty=0`.
- After a review round, diff the committed file list against `origin/main`
  before amending, and grep the diff for markers. `git add -A` after a review
  is how contamination gets committed.

Related: [[a-mutation-restore-must-refresh-mtime]],
[[a-mutation-result-is-a-claim-about-a-suite]], [[a-dev-check-log-names-its-tree]],
[[gglib-adversarial-review-loop]].
