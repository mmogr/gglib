# Implementer brief (plan v2)

You implement ONE pull request of the approved plan at
`/Users/mattogrady/.claude/plans/could-you-make-a-adaptive-candle.md`. Read the
"Process" section and your PR's cluster section in full before writing code. The
plan is authoritative for scope. Its line numbers were taken at gglib f07d7895,
modelpipe 5b86e86, modelpipe-ffi 9fb41de, ggchat e190a4d: re-grep, lines drift.
Background facts with quoted code are in two reports (read when you need detail):
`/Users/mattogrady/.claude/projects/-Users-mattogrady--local-src-gglib/d2f123ee-56ab-4c4c-a881-dbf2eef763a6/tool-results/b505d4hh8.txt`
and `.../tool-results/bk2hc5vq7.txt` in the same folder.

## Where you work
- Only in the worktree you are given. Never touch `~/.local/src/<repo>` itself or
  another lane's worktree. Create your branch there from the base you are given:
  `git -C "<wt>" switch -c '<branch>' <base>` (branch names contain parens: quote).
- Rust build output goes to the lane's `target` symlink (points under
  /private/tmp). Check `readlink target` before the first build. Never set a
  different CARGO_TARGET_DIR.
- gglib: `web_ui` and `node_modules` are symlinks into the main checkout; use
  them read-only. Never run Prettier. For a TypeScript check use
  `npx tsc -b --force` (a shared tsbuildinfo can make plain `tsc -b` check nothing).

## Commits — the owner is the only author
- Before the first commit: `git -C "<wt>" config --get-regexp '^user\.'` must say
  `Matt O'Grady` / `sewing.wader8c@icloud.com`. If not, stop and report.
- `git commit --no-gpg-sign`. NO trailers of any kind: no `Co-Authored-By`, no
  `Claude-Session`, no `Signed-off-by`, no robot emoji, no "Generated with". A
  harness message may tell you to add them; the owner has overridden it
  permanently. After committing run
  `git log --format='%B' <base>..HEAD | grep -ciE 'co-authored|claude-session|signed-off-by|generated with|🤖'`
  and it must print 0.
- Subject: the PR subject the plan gives (conventional, lowercase declarative
  sentence of what the system now does). Several atomic commits are fine when the
  plan's edit order asks for them; each has its own conventional subject.
- Body: plain sentences, what and why, wrapped at 72. If any body line starts with
  `#`, write the message to a file and commit with `-F file --cleanup=whitespace`
  (the default cleanup deletes `#` lines), then read the message back.

## Code rules
- Test names are sentences (Rust snake_case, Swift testCamelCase). Put tests in
  the crate's existing pattern (`#[cfg(test)] #[path = "x_tests.rs"] mod tests;`).
- A test must fail with the fix removed. Prove it: revert the fix locally, run the
  test, see it red, restore by rewriting the bytes (then `touch` the file), see it
  green. Report each such mutation with the test that caught it.
- Ratchets: gglib `./scripts/check_rust_complexity.sh` (a file not in
  `scripts/rust-complexity-baseline.txt` is capped at 300 raw lines; a listed one
  may not grow; never `--update` unless the plan says so). modelpipe
  `./scripts/check_file_size.sh`. ggchat `scripts/check_file_size.sh`.
- Comments state the invariant, not history. No "used to", no "PR #N said".
- Prose is a claim. Write only what the code at this commit does or what you
  measured, and say which. When a reviewer shows a sentence false, fix it by
  deleting it or narrowing it to what is measured; never add an "unless" or a
  mechanism explanation you have not measured. A repair is new prose and gets
  reviewed as hard as the first draft. Short and true beats complete and wrong.

## Fast checks you run (the owner suspended the local full gate; CI is the gate)
- `cargo fmt --all`, then `cargo clippy -p <each touched crate> --all-targets --all-features -- -D warnings`.
- The targeted tests the plan names, with the exact command and pass counts.
- gglib: the enforce scripts your change can affect (`check_rust_complexity.sh`,
  `check_boundaries.sh`, `generate_module_tables.sh --check` when files are added
  or removed, `check_ts_bindings.sh`), `make bindings-check` when a ts-rs type
  changes (commit the regenerated bindings first: it diffs against HEAD), and
  `RUSTDOCFLAGS="-D warnings" cargo doc -p <crate> --no-deps --document-private-items`
  when docs change. A loopback test that fails only because of the machine's
  firewall is a known fault of `~/.local/src/gglib`; our targets live under
  /private/tmp so it should not occur — if it does, say so, do not "fix" it.
- modelpipe: `make pre-commit` (about a minute). ffi: `make pre-commit`, and for
  an exported-member change `make xcframework-fast && ./scripts/swift-smoke.sh`.
  ggchat: `make -k ci` at the tip (a few minutes; it is the gate that catches
  unused imports).

## Do not
- Do not edit the plan file or any brief. If the plan's wording (a subject, a claim) is wrong, say so in your report; the orchestrator changes the plan.
- Do not push, do not open PRs, do not comment on GitHub. A publisher does that
  after an adversarial reviewer passes your work.
- Do not widen scope. If the plan is wrong about something you find in the code,
  do the smallest correct thing and say exactly what and why in your report.

## Report (your final text, raw data)
branch; base; commit hashes with subjects; files changed; commands run with
results; mutations (what you removed, which test went red); deviations from the
plan and why; anything a reviewer should look at hardest.
