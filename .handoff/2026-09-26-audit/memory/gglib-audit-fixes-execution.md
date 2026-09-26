---
name: gglib-audit-fixes-execution
description: "the 2026-09-25 audit-fix plan v2 across gglib/modelpipe/modelpipe-ffi/ggchat; where the ledger, plan and lane tooling live; read LEDGER.md first when resuming"
metadata:
  node_type: memory
  type: project
  originSessionId: d2f123ee-56ab-4c4c-a881-dbf2eef763a6
  modified: 2026-09-25T18:26:17.217Z
---

Plan v2 was approved 2026-09-25. It covers audit fixes plus the maintenance block across four repos: 28 gglib PRs, 8 modelpipe, 3 ffi and 3 ggchat, plus releases.

- **Plan:** ~/.claude/plans/could-you-make-a-adaptive-candle.md (copy: ~/.local/src/handoff-2026-09-25-audit/PLAN-v2.md).
- **Resume from:** ~/.local/src/handoff-2026-09-25-audit/HANDOFF-2026-09-26.md (and NEXT-SESSION-PROMPT.md). At 2026-09-26 only C3 (gglib), T4 (ffi #35, Matt's merge) and T7 (ggchat bump) were left.
- **Ledger:** ~/.local/src/handoff-2026-09-25-audit/LEDGER.md is the source of truth for PR numbers, shas, workflow run ids and decisions taken. Read it first.
- **Tooling:** `briefs/` beside the ledger (and in the session scratchpad) holds:
  - the IMPLEMENTER, REVIEWER and PUBLISHER briefs;
  - pr-lane.js (implement, tiered review and fix for up to 3 rounds, then publish);
  - publish-only.js, review-then-publish.js and fix-review-publish.js;
  - merge_pr.sh, which refuses on attribution, identity, a failing check or merge state;
  - watch_prs.sh;
  - rebase_pr.py, which pushes only when the non-README patch is unchanged.
- **Waiting on Matt:**
  - the ffi release #35, after T3;
  - the crates-io approval for each release.
- **Decided on 2026-09-26:**
  - #134 was merged and tagged v0.8.1.
  - H4's #1057 correction is a yes.
  - K1b: the owner has no lawyer, so #1117 is closed. CONTRIBUTING will say outside code is not accepted yet.
  - The strict ruleset stays.

Rules in force: [[gglib-strict-ruleset-serial-merges]], [[an-exactly-when-claim-is-a-theorem]], [[subagents-run-on-opus]], [[skip-local-gate-until-next-handoff]], [[gglib-git-workflow-preferences]].
