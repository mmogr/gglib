# Audit-fix execution ledger (plan v2)

Plan: ~/.claude/plans/could-you-make-a-adaptive-candle.md (copy: PLAN-v2.md here).
Briefs and the lane workflow script: briefs/ (live copies in the session scratchpad).
Scratchpad (targets, review worktrees): /private/tmp/claude-501/-Users-mattogrady--local-src-gglib/d2f123ee-56ab-4c4c-a881-dbf2eef763a6/scratchpad

## Lanes (worktrees; each target symlinked under the scratchpad/targets)
| lane | worktree | PRs |
|---|---|---|
| gglib docs | ~/.local/src/worktrees/gglib/lane-docs/gglib | K1, K1b (draft, lawyer), then K-contrib, K3 (wf_4990b504-5a3) |
| gglib docs2 | ~/.local/src/worktrees/gglib/lane-docs2/gglib | K-tables, K4, then H1, H3 held (wf_ceae4bc9-383) |
| gglib proxy | ~/.local/src/worktrees/gglib/lane-proxy/gglib | A2, A3 (held until A2 merges) |
| gglib cli | ~/.local/src/worktrees/gglib/lane-cli/gglib | B4, B3, C1, then B2, B5 (wf_4307b160-c72) |
| gglib deps | ~/.local/src/worktrees/gglib/lane-deps/gglib | C2a, C2b held (wf_465e8ef6-22d) |
| gglib remote | ~/.local/src/worktrees/gglib/lane-remote/gglib | B1 (after the new issue) |
| modelpipe | ~/.local/src/worktrees/modelpipe/lane-mp/modelpipe | E6a, E1 |
| modelpipe 3 | ~/.local/src/worktrees/modelpipe/lane-mp3/modelpipe | E-priv |
| modelpipe 2 | ~/.local/src/worktrees/modelpipe/lane-mp2/modelpipe | E4, E2, E3 (held until E2 merges) |
| ffi | ~/.local/src/worktrees/modelpipe-ffi/lane-ffi/modelpipe-ffi | F1 |
| ffi 2 | ~/.local/src/worktrees/modelpipe-ffi/lane-ffi2/modelpipe-ffi | F2 |
| ggchat | ~/.local/src/worktrees/ggchat/lane-gc/ggchat | F5, F4 |

## Workflow runs (2026-09-25)
- issues: wf_a20ffaeb-0d6 (draft + review of follow-up issues)
- K1/K1b wf_57d07e47-e59; K-tables/K4 wf_c41feda8-4cb; E6a/E1/E-priv wf_ca613b25-2a5; E4/E2/E3 wf_3bbe9517-ea2; F1/F2 wf_ae1ad468-d04; F5/F4 wf_7e8eff07-d68; A2/A3 wf_d4b912ca-213; B4/B3/C1 wf_fb985491-783; B1 wf_83ac49cc-466

## Housekeeping done
- git worktree prune: gglib 57 -> 32 registrations (25 had no directory). No worktree removed: several hold uncommitted work (ggchat c2-c8, gglib b2/b5).
- Removed the 6 GiB .build of merged, clean ggchat worktree connect-side-502. Free: 359 GiB.

## Decisions taken while executing
- H1's check_adrs.py was growing into a general GitHub-heading-id emulator under review. Scoped back to decision 4: ids must match GitHub's for headings in the repo and the fixture, and the docs say no more than that. Narrow fixes via fix-review-publish.js (wf_4234b0b3-ea9); H3 waits for H1.
- B1's read/change equivalence prose ('a page may change exactly when it may read') failed three focused reviews on successive edge cases (the endpoint's own page, Origin: null, extension pages). The fix applies one reviewer-verified wording in all eight places ('a page that names any origin but the endpoint's own may change something exactly when the CORS layer lets it read the answer') via briefs/fix-review-publish.js (wf_7e86ae94-d3c). Lesson: an 'exactly when' claim in prose is a theorem; state it only in the form a test checks.
- B1 round 4 failed only on three doc sentences with exact replacements. The orchestrator applied them itself (8b8118d9, message byte-identical), replayed onto main (69881757), and sent it through one focused reviewer before publishing (briefs/review-then-publish.js, wf_0ec8fe39-1ed). rebase_pr.py's README check now compares each README's changed lines (table lines aside) rather than whole files.
- B1: the reviewer found the MCP handler's own origin check untested once the router guard answers first. Chose to keep that stricter check on /mcp (safer under a permissive CORS config) and add a test for it under CorsConfig::AllowAll, rather than delete it as plan v2 had said (wf_4d2b793d-09f).
- briefs/rebase_pr.py replaces rebase_pr.sh: it resolves only a README conflict where main's sole change was deleting the module-table block, and pushes only if the non-README patch is byte-identical (range-diff with a pathspec) and each README equals the reviewed one minus its table. Rebases run in ~/.local/src/worktrees/gglib/lane-rebase/gglib (detached after each).
- gglib merge queue (one at a time, strict ruleset): #1129 -> #1131 (then A3 rebased --onto main and published) -> #1135 -> #1136 -> later PRs.
- K-tables (#1130) merged: every gglib branch that regenerated a module table (A2 did) will conflict on rebase; resolve by dropping the table block.
- E2 missed a PASS on round 3 over three wording defects (code passed); resumed with the reviewer's exact fixes, E3 stacked behind it (wf_9bf38b58-478).
- E1 missed a PASS on round 3 over the folded-in E5 shutdown rustdoc (the fix commit 3f9477a passed). Resumed (wf_5345ab1c-711) with the rustdoc cut to measured claims; E-priv moved to its own lane (wf_9e37358e-9a6). The implementer brief gained a rule: fix a false sentence by deleting or narrowing it, never by adding an unmeasured qualifier.
- F1 missed a PASS on round 3 over its re-run recovery prose (two reviewers disagreed about GitHub's re-run semantics, one with evidence from a real ggchat run). Resumed (wf_ee83d92b-fea) with prose that asserts nothing about GitHub re-runs: recover by artifact id + checksum match; if none matches, take the next version. F2 moved to its own lane (wf_94462425-790).
- gglib's ruleset ggRules has strict_required_status_checks_policy=true (a PR must be up to date with main); modelpipe, ffi and ggchat do not. The ruleset is never bypassed: a behind gglib PR is rebased with briefs/rebase_pr.sh, which pushes only when git range-diff shows the patch unchanged (the reviewer PASS carries over), then CI reruns. gglib merges therefore go one at a time.
- Disk re-budget (decision 9), measured 2026-09-25: a gglib author lane target is 6-11 GiB, a gglib reviewer target (line-tables-only, no incremental) 3-4 GiB, modelpipe 6-7 GiB, ffi 12 GiB. The four-gglib-target cap is not binding; 287 GiB free with every lane built.
- A2 missed a PASS on round 3 over one rustdoc sentence; resumed via pr-lane.js's resume path (wf_41d12f8c-4cc), then A3.
- Worktrees are kept; only build output is deleted (standing rule), narrower than plan v2's "remove merged worktrees".
- Child PRs of a stack (A3, E3) are pushed only after their parent merges.
- The orchestrator merges reviewed, green PRs in the plan's order; release PRs, the crates-io approval and the lawyer-gated K1b stay Matt's.

## PRs
| id | repo | PR | sha | state |
|---|---|---|---|---|
| K1 | gglib | #1116 | 7ad83675 | MERGED e47a6f2f |
| E4 | modelpipe | #127 | - | MERGED 90bc0051 |
| E6a | modelpipe | #128 | - | MERGED 3e9694af |
| F5 | ggchat | #135 | - | MERGED 7a7c4742 |
| K-tables | gglib | #1130 | 654ec747 | MERGED 846259d0 |
| K4 | gglib | #1132 | 7755efda | MERGED 249a0648 |
| F4 | ggchat | #143 | 3fd804e2 | MERGED 3b474d9a (fetch-all mutation survived; plan fallback applied) |
| E-priv | modelpipe | #135 | eb764c42 | MERGED 141acbca |
| B4 | gglib | #1129 | 7de87aec | MERGED b06336e6 |
| B3 | gglib | #1135 | 4cec2f4c | MERGED 3d994f81 |
| C1 | gglib | #1136 | ab5ae0db | MERGED dc1fbca8 |
| E1 | modelpipe | #136 | e0167721 | MERGED (see main) |
| A2 | gglib | #1131 | 020e280d | MERGED 4de6590b |
| A3 | gglib | #1139 | f0a1f4f0 (replayed 4cc0a310; 3 README table rows dropped) | MERGED 55093e75 |
| A4 | gglib | #1146 | 3a75a83a replayed fe02cc1a -> 227d284a (rebased) | MERGED ad596d93; PASS both lenses (wf_cf5eac18-547); subject narrowed to 'before the first token' (plan row updated) |
| F2 | modelpipe-ffi | #39 | cc117de7 | MERGED 94d02fe6 |
| F1 | modelpipe-ffi | #40 | 777c20bc | MERGED 8b9c580e |
| E2 | modelpipe | #137 | 80833a33 | MERGED d9e143ec |
| E3 | modelpipe | #138 | 2d8dfdf6 (replayed 973e231e) | MERGED 7760a81c |
| C2a | gglib | #1137 | 5a85940c | MERGED e3c0a8fd |
| C2b | gglib | #1144 | 708e4843 | MERGED 61cd242d (a stale in_progress/success check record was re-run first) |
| B2 | gglib | #1138 | bb5ca4c6 | MERGED 17cd6e96 (follow-up #1141) |
| B5 | gglib | #1140 | 8f90b219 -> caee3730 (rebased; the shared data_dir.rs helper B2 already added, byte-identical, dropped out of the patch; range-diff = with it excluded; README lines identical) | MERGED cd96acb7 |
| H1 | gglib | #1143 | 664f6863 -> 8a171cfc (rebased; check_adrs.py green on it) | MERGED 2210ea57 |
| H3 | gglib | #1148 | 940436a8 | MERGED a6d5eca6 |
| B1 | gglib | #1145 | c1a24511 -> 222f33a5 (rebased, patch unchanged) | MERGED dffa1699 (closed #1118) |
| D1 | gglib | #1149 | fbe57bb7 | MERGED 3709913f |
| D2 | gglib | #1154 | f893b943 | MERGED 36a84bef |
| K-contrib | gglib | #1142 | 9e0c2927 -> 1092c6ff | MERGED 501a0c0c |
| K3 | gglib | #1147 | d364efd7 | MERGED ca67ef6a |
| I-2 | gglib | #1152 | c1b06354 | MERGED 2f56fe83 |
| I-1 | gglib | #1158 | 97c01560 | MERGED 5a017b3d |
| I-3 | gglib | #1159 | c7ce84f3 | MERGED 3796fbfd |
| I-4 | gglib | #1160 | a5a41a39 | MERGED 198a000b (all comment-cleanup PRs merged) |
| H4 | gglib | #1155 | 55bf692d | MERGED b0f9beee (closed #1057) |
| C3 | gglib | local (resume wf_c312adf7-494) | d5f6a5ec (on tmp/c3-base) | resume r2 FAIL: the gate failed OPEN on a malformed/duplicated baseline row or an uncountable file; the outside row's first number had no failing fixture. Title now '...may not grow'; #1157 body corrected |
| (release) | modelpipe | #134 | - | MERGED 1277c64 by Matt, tag v0.8.1; Matt approved crates-io; 0.8.1 PUBLISHED |
| T-g | gglib | #1162 | 11a76c44 | MERGED d18bc523 (gglib on modelpipe 0.8.1) |
| T3 | modelpipe-ffi | #41 | f7d36cf4 | MERGED 0ea5607e; next: check #35's changelog lists F1, F2, T3 (T4, Matt merges) |
| K1b | gglib | #1150 (replaces closed #1117) | 7c366e35 | MERGED 48f58bb2 |
| E6b | modelpipe | #139 | 053b10d6 | MERGED e000404a |
| I-M1 | modelpipe | #140 | 94db24b8 | MERGED f8fc7256 (modelpipe's plan PRs all merged) |

## Issues (filed 2026-09-25, all read back: author mmogr, no attribution)
gglib: #1118 (cross-site, closed by B1), #1119 error-code list, #1120 status mkdir, #1121 parser bypass, #1122 thin crates, #1123 parse once, #1124 ratchet counts code, #1125 idle-timeout residue, #1126 core split, #1127 GUI pairing reveal, #1128 GUI key reveal; comment on #1039 (token scope, #983 prerequisite, #1118).
modelpipe: #129-#133. modelpipe-ffi: #36-#38. ggchat: #136-#141.
Later: gglib #1133 (deploy-docs Web UI build), #1134 (README scaffold rough edges), #1141 (reset cannot recover an unreadable record), #1151 (label-check's fork message promises labels; found reviewing #1150), #1153 (gglib-proxy lib.rs counts seven pub modules, there are eight; found reviewing #1152), #1156 (request_pipeline README says every request path calls apply; found in I-1), #1157 (grandfathered clippy allows from lint inheritance are paid down; cited by C3's allow reasons), #1161 (bug: the daemon ignores the saved llama base port; ServerConfig::with_defaults 9000 is always passed as the override; found in I-4's review).
Drafts and the filer: scratchpad/issues-final.json, issues-filed.json, file_issues.py.

- 2026-09-26 queue: B2 #1138 -> B5 #1140 (hand-merge tests/support/data_dir.rs) -> H1 #1143 -> C2b -> B1 -> A4 -> H3 -> K-contrib/K3.
- 2026-09-26 queue reordered so D1 can publish: B2 #1138 -> B5 #1140 (data_dir.rs helpers are byte-identical, no hand merge) -> B1 #1145 -> H1 #1143 -> C2b #1144 -> A4 #1146 -> H3 -> K-contrib/K3.
- 2026-09-26: K-contrib #1142 and K3 #1147 published (wf_4990b504-5a3). Queue: B5 #1140 -> B1 #1145 -> H1 #1143 -> C2b #1144 -> A4 #1146 -> K-contrib #1142 -> K3 #1147 -> H3 (after H1).
- 2026-09-26 queue: C2b #1144 (CI) -> A4 #1146 -> K-contrib #1142 -> K3 #1147 -> H3 #1148 -> D1 (after review) -> D2.
- 2026-09-26 Matt: H4's #1057 correction confirmed YES (strike "and a fortnight of using it settled ... it is a step." with a dated retraction; the log entry states #1012 on 2026-09-09 to #1024 on 2026-09-10 and the trial since 2026-08-30, #963).
- 2026-09-26 Matt marked modelpipe #134 ready (9 checks green, CLEAN). Still Matt's: merge it, then approve the crates-io environment.
- 2026-09-26 Matt has no lawyer, so K1b #1117's gate cannot be met as written. Options put to Matt: close K1b and say outside PRs are not accepted for now (recommended); adopt a standard CLA text unchanged; merge the bespoke clause as is. Every commit on gglib main is Matt's or a bot's.
- 2026-09-26 Matt delegated K1b; decided: close #1117, CONTRIBUTING says outside code is not accepted yet (no contributor agreement exists for the README's commercial-license offer).
- 2026-09-26 Matt asked about relaxing the strict ruleset; advised keeping it (merge queue is not offered on user-owned repos).
- 2026-09-26 A4 #1146 MERGED ad596d93. The plan's comment-marker count used git grep -E, which prints 0 on macOS (no \b in ERE); corrected to -P in the plan (329 gglib-wide, 52 in gglib-proxy at ad596d93).
- 2026-09-26 D1 published #1149 (5d0b71dc). The GUI section heading stays 'Another machine'; only the button says Join (the body says so).
- 2026-09-26 queue reordered: K3 #1147 -> D1 #1149 -> H3 #1148 -> K1b-alt -> D2 (D2 and I-1/I-3/I-4 wait on D1).
- 2026-09-26 queue: D1 #1149 (CI) -> H3 #1148 -> K1b #1150 -> I-2 #1152 -> D2 (after D1).
- 2026-09-26 queue: I-2 #1152 (CI) -> D2 #1154 -> H4 #1155 -> I-1/I-3/I-4 (after D2, replay --from a573b639) -> I-4 baseline commit -> C3 -> T-g (after 0.8.1 publish).
- 2026-09-26 all three gglib comment-diet PRs (I-1, I-3, I-4) missed round 3 on sentences written while rewriting history into present tense: a present-tense restatement of an old causal claim is new prose and gets checked like any other.
- 2026-09-26 queue: H4 #1155 (CI) -> I-1 -> I-3 -> I-4 (+ baseline commit) -> C3 -> T-g (after 0.8.1).
- 2026-09-26 I-4's baseline re-tightening commit (check_rust_complexity.sh --update: 174 -> 166 entries, none grew, the 8 dropped files all <=300 lines) was made on tmp/c3-base as c0c951c8; cherry-pick it onto I-4 after I-1 and I-3 merge, then focused review.
- 2026-09-26 gglib-wide history-marker count (git grep -nPi, plan pattern, crates/**/*.rs + src-tauri/**/*.rs, tests excluded): 332 at f07d7895, 101 at 3796fbfd, 53 at I-4's head. Misses the plan's 'under 40'; reviewers judged every remaining hit present-tense vocabulary or dated evidence.
- 2026-09-26 T4 check: ffi release PR #35 (v0.4.2, head e4282a3e) changelog lists #40 (F1), #39 (F2), #41 (T3). Waiting on CI, then Matt marks ready and merges; nothing else merges in ffi until the release run finishes; then T7.
- 2026-09-26 ffi #35 CI green (6 checks), CLEAN, still draft: waiting on Matt to mark ready and merge (T4).
- 2026-09-26 session ended on credits: HANDOFF-2026-09-26.md and NEXT-SESSION-PROMPT.md written; C3 fix round wf_c312adf7-494 was in flight (lane head 6e8e7966).
- 2026-09-26 C3 pushed at 6e8e7966 (unreviewed fix round: fail-closed compare + outside-row fixture); handoff pushed to branch docs(handoff)/audit-plan-v2-continuation for a cloud session.
