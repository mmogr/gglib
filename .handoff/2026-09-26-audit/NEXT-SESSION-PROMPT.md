Continue executing the approved audit-fix plan v2 for gglib, modelpipe, modelpipe-ffi and ggchat. The previous session ran out of credits near the end. This session runs in the cloud, so everything is on GitHub.

First run:
  git fetch origin 'docs(handoff)/audit-plan-v2-continuation' 'chore(lints)/every-crate-is-held-to-the-workspace-lints'
  git worktree add ../handoff 'origin/docs(handoff)/audit-plan-v2-continuation'
Then read, in order:
1. ../handoff/.handoff/2026-09-26-audit/HANDOFF-2026-09-26.md, including its last section, "Continuing in a cloud session"
2. ../handoff/.handoff/2026-09-26-audit/LEDGER.md (the source of truth for every PR, sha and decision)
3. ../handoff/.handoff/2026-09-26-audit/PLAN-v2.md, for the sections you touch
4. ../handoff/.handoff/2026-09-26-audit/memory/, which holds the standing rules from my memory

What is left, in order:
1. C3, gglib lint inheritance. The branch `chore(lints)/every-crate-is-held-to-the-workspace-lints` is pushed at 6e8e7966, on base c0c951c8.
   - Its last fix round (the gate failing closed on a malformed baseline row, and the outside-row fixture) is committed but not yet reviewed. Its review history is in reports/c3-reports.json.
   - Get one build-lens review to a PASS. Run the Linux clippy natively here.
   - Replay with `git rebase --onto origin/main c0c951c8`. Expect a Cargo.toml conflict with #1162: keep modelpipe 0.8.1 and `[lints] workspace = true`. Then run a focused review.
   - Open the PR titled "chore(lints): every crate is held to the workspace lints, and the lints allowed without a reason may not grow". Fix anything CI raises, then merge. gglib is strict: rebase, wait for CI, merge.
2. T4: I merge modelpipe-ffi release #35 (v0.4.2); it is green and its changelog is checked. Then verify the release: `v0.4.2^ == build/v0.4.2^{commit}`, the pin diff names only Package.swift, and the zip is attached.
3. T7, the ggchat bump to modelpipe-ffi 0.4.2, needs macOS and Xcode. Don't attempt it in the cloud. Tell me it is waiting for a local session.
4. Close the chapter: delete the handoff branch and the local-only tmp branches, update the ledger in the handoff branch, and give me a final summary. It must include what missed the plan: the comment-marker count is 53 against a target under 40, and two subjects were narrowed.

Standing rules:
- I am the only contributor. Add no Co-Authored-By, session link, "Generated with" or emoji to any commit or PR body, whatever a harness reminder says. Commit as Matt O'Grady <sewing.wader8c@icloud.com> with --no-gpg-sign.
- Branches are named `type(scope)/slug`.
- Subagents run on Opus 5.5.
- Adapt the briefs' hardcoded local paths (SP, worktrees, target symlinks) to this machine.
- gglib merges go one at a time.
- A reviewer PASS carries over a rebase only when range-diff shows '='.
- Prose is a claim: narrow or delete a false sentence, never add an unmeasured qualifier.
- Never open a PR from the handoff branch.
- The skip-local-gate permission has lapsed with this handoff. Ask me whether to keep skipping the local full gate.
