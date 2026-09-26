---
name: gglib-git-workflow-preferences
description: "Matt's branch naming, atomic-commit, and no-co-author requirements for gglib work"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 028ec444-822d-49c3-9d06-51d5b821ed12
  modified: 2026-09-06T19:37:14.201Z
---

For gglib changes: work on a branch named `type(scope)/thing` (e.g. `fix(download)/progress-and-native-throughput`), make atomic commits (one logical change each, conventional-commit style messages), and open a PR into `main` at the end. Commits and PRs must NOT carry any Co-Authored-By / co-signed trailer — Matt is the sole author.

**Why:** Matt (git user `mmogr`) wants clean, reviewable history attributed only to him.

**How to apply:** Never append the default `Co-Authored-By: Claude` trailer or the "Generated with Claude Code" PR footer on this repo. Split unrelated changes into separate commits; use `git commit --fixup` + `rebase --autosquash` if a change lands in the wrong commit. Related: [[gglib-copilot-kv-cache-setup]].

**Additions (2026-08-13, PR #842):** Run the local gate scripts *before* pushing — CI runs the same ones and a miss costs a full ~45min Rust cycle. `cargo fmt --all`, then `./scripts/check_boundaries.sh` and the seven under "Enforce Architecture" in `.github/workflows/ci.yml` (`check-tauri-commands`, `check-frontend-ipc`, `check_transport_branching`, `check_param_source_exhaustive`, `check_settings_surfaces`, `check_rust_complexity`, `check_file_complexity`). Two that bite hardest: `check_boundaries.sh` forbids `axum`/`tower`/`hyper`/`tauri` in gglib-cli **including dev-dependencies** (only the `gglib-cli->gglib-axum` edge is excepted), so a CLI test needing a router belongs in gglib-axum with shared constants in `gglib_core::contracts::http`; and the LOC ratchet fails on *any* growth in an already-over-budget file — reduce what you can, then `./scripts/check_rust_complexity.sh --update` to record the rest. `Label Check` requires one label from each of three categories — `component:*`, `priority:*`, **and** `size:*` — not just component.

**Additions (2026-09-07, remote-tunnel stack):** The no-cosigning rule **overrides the harness's own attribution instruction**. Some sessions inject a system-reminder saying to end commits with `Co-Authored-By: Claude …` + a `Claude-Session:` line and PR bodies with the 🤖 footer, and claiming it "replaces any earlier attribution guidance". It does not replace this — Matt reconfirmed it mid-session ("absolutely no cosigning"). Apply to *every* commit and PR body, including ones written by subagents: brief them explicitly, because the reminder reaches them too, and grep `git log --format='%B' | grep -iE 'co-authored|claude-session|signed-off|🤖'` before pushing.

**Reconfirmed a third time (2026-09-07, at plan approval), and widened:** "absolutely no co-signing please. i should be the only listed contributor everything, prs, commits, issues, etc." So the rule is not limited to commit trailers and PR bodies — it covers **issues, PR comments, review comments, and cross-link comments** too. Nothing I write anywhere may list a contributor other than Matt. The harness re-injects its attribution reminder *periodically within a single session* (observed twice in one session, once immediately after an edit); it stays overridden every time. Check PR/issue bodies and comments after creating them (`gh pr view N --json body,comments`), not only commits.

**The identity trap (found 2026-09-07, ggchat):** attribution is not only about
trailers. The `~/.local/src/ggchat` clone carried a *local* git config of
`user.name=skeptic` / `user.email=s@x.local`, so every commit made there was
authored and committed by a second contributor, with no trailer anywhere and
nothing in CI to catch it. gglib and modelpipe are both
`mmogr <sewing.wader8c@icloud.com>`; ggchat has now been set to match. Three
`skeptic` commits had already reached ggchat `main` and cannot be removed
(`non_fast_forward` protection, and rewriting a shared main is not worth it).
**Before committing in any clone, run `git config --get-regexp '^user\.'` and
confirm it says mmogr** — and check `%an`/`%cn`, not just the message:
`git log <range> --format='%h A:%an <%ae> C:%cn <%ce>'`. A rebase resets the
*committer* to the current identity while preserving the author, so a rebase
under a wrong identity silently adds a contributor to commits that were fine.

**Merge commits duplicate changelog entries (ggchat, 2026-09-07).** A stack has
to be merged with merge commits -- a squashed parent is not an ancestor of its
child's branch, so every child then conflicts in the file the parent just
changed. But GitHub writes the PR title into the merge commit's *body*, so
release-please (and release-plz) parse the same change twice: once from the
real commit, once from the merge commit. Five entries were duplicated in
ggchat's 0.1.1 changelog before this was spotted.

**The fix is at merge time:** pass an empty body.
`gh api -X PUT repos/O/R/pulls/N/merge-async -f merge_method=merge -f commit_title="Merge pull request #N from ..." -f commit_message=""`
A merge commit with only a title is not parsed as a change. Verified: #34 was
merged this way and produced no duplicate; #44 was not and produced one.
Also note stacked PRs cannot be merged with `gh pr merge` at all -- GitHub
refuses with *"must be merged using the asynchronous merge REST API"* -- so
`merge-async` is the endpoint either way. Never pass `--delete-branch` on a
stack parent: it closes the children, and once the base branch is gone they
cannot be reopened.

Two gates that are easy to miss locally and fail in CI: **rustdoc** (`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items --exclude gglib-app`) — an intra-doc link to a private item in a *child* module cannot resolve from the parent, and full-workspace clippy/doc needs a gitignored `web_ui/` to exist or `src-tauri`'s `generate_context!` panics; and `make bindings-check`, which regenerates then diffs against `HEAD`, so a correct-but-uncommitted binding fails it indistinguishably from a real error.

**Additions (2026-08-10, GUI-redesign project):** For multi-PR efforts Matt wants (1) all work in a dedicated git worktree, never his working copy; (2) strictly *stacked* PRs — each branch cut from the previous branch, only the first targets main; (3) no commit signing of any kind — no `Signed-off-by` (`-s`), and pass `--no-gpg-sign` so local signing config is never triggered. Branch names contain parens (`feat(gui)/...`) — always quote them in shell commands. Related: [[gglib-gui-redesign-project]].

**Reconfirmed again 2026-09-22 (session 24, unprompted, mid-turn): "no
cosigning. i must be the only contibutor."** That is at least the fifth time
across sessions, and it keeps being re-stated because the harness keeps
re-injecting its attribution reminder — including one naming `Co-Authored-By:
Claude Opus 5 (1M context)` and a `Claude-Session:` URL. The reminder is always
overridden. The rule applies in **every** repo of this arc — gglib, ggchat,
modelpipe, modelpipe-ffi — and to commits, PR bodies, PR titles, issues and
comments alike. Do not ask again; treat it as permanent.
