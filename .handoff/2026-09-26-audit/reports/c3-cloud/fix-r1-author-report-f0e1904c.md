Author's fix report for C3 at f0e1904c (UNVERIFIED — treat every claim as a claim)

Branch chore(lints)/every-crate-is-held-to-the-workspace-lints, base d18bc523 (origin/main). Head f0e1904c5340a2723c6375cab15eb2c5dcbdf93f.
Commits: 731c554 (=), 5844d61 (=), aa1ad48 (was adb04cf, changed), f0e1904 (was 462d0c3, =). The change against 462d0c3c touches only scripts/README.md and scripts/check_lint_inheritance.sh (mode 100755). Built with --fixup=adb04cf + autosquash onto d18bc523, with an exec `git commit --amend --no-gpg-sign --cleanup=whitespace -F fix-r1/msg.new`; message read back byte-identical.

Defects (from review/c3-build-r1-verdict.md):
- 1 fixed: "a counter that exits 1" → "…without printing a count" in header, ✓ line, fixture comment and both fixture messages, README, body.
- 2 fixed: rule sentence → "a baseline row whose second or third field is not a number" in header, README, body. Optional "a count field that is not a number" applied in header fixture list, README, ✓ line. Deviation: "count field" also put in the fixture comment above the counter fixture.
- 3 fixed: closing MSG is the reviewer's four lines verbatim.
- 4 fixed: "whole-line `//` comments not" in header and README.
- R1 added: after the `$2 = 14` fixture, baseline with good's field 3 = 6, plain check, must exit non-zero and print "good: 7 lints allowed without a reason, baseline 6".
- R2 not added.
- Header lines 12–17 and 35–44 and the README paragraph rewrapped to existing widths.

Commands claimed: shellcheck 0.9.0 rc 0; self-test and plain check with LANG=C.UTF-8 under bash 5.2.21 and 3.2.57, rc 0; LANG=C.UTF-8 make enforce rc 0; attribution grep 0; %an/%cn Matt on all four; git status clean.

Mutations claimed (runner fix-r1/mut.py extended from the reviewer's; works on fix-r1/new.sh whose sha256 equals the script at aa1ad48/HEAD; KILLED = rc≠0 and first line "❌ self-test:"; log fix-r1/mut.log; same verdict under both bashes for all 66 mutant×environment pairs):
- 23 killed: K1–K7 (ignore cfg_attr; first allow only in cfg_attr; count comments; drop every line holding //; count bare attributes; count only src/; skip outside files), D1–D4 (drop each git listing flag), E1 (empty the missing/empty [lints] check), G1, G2 (either comparison never firing), O2 (outside first number grows), P1 (member second number grows only without --update), U5 (--update not raising the first number), L1, L4, L5, L6, L8, L11.
- 10 killed that keep the message and drop the failure: R1–R10.
- 2 survive: L2, L3 (drop one count from the number check).
- P1 by hand: new script → "❌ self-test: an allow without a reason above the baseline exited 0", rc 1 both bashes; same mutation on 462d0c3c's script → ✓ rc 0.
- Isolation matrix: host env has no excludes file, no /etc/gitconfig, default template. I1 (drop XDG_CONFIG_HOME) killed only with ~/.config/git/ignore; I2 (drop GIT_CONFIG_GLOBAL) only with an excludes file named in ~/.gitconfig; I3 (drop GIT_CONFIG_NOSYSTEM) only with one in the system config; I4 (drop --template=) only with template info/exclude; I5 (drop the export line) killed with the first three, passes with the template. Unmutated passes in all five environments.
- Reviewer's full set re-run (fix-r1/reviewer-set.log): only change is P1 now killed; L2, L3, L14, L16, U4 still survive. The body counts only the mutations it names.

Body before → after:
- Para 3: "…a baseline field that is not a number each fail the check…" → "…a baseline row whose second or third field is not a number each fail the check…"
- Fixture list: "It has baselines that each number of a member's row and of the outside row must fail against, one whose first number `--update` must raise, one that …, a baseline row naming no member, and a tree …" → "It has baselines that each number of a member's row and of the outside row must fail against without `--update`, and that the member's second number must also fail against under it; one whose first number `--update` must raise; one that `--update` must rewrite to exactly those counts; a baseline row naming no member; and a tree in which git lists no .rs file."
- Counter: "It has a counter that exits 1, a baseline row listed twice, …" → "It has a counter that exits 1 without printing a count, a baseline row listed twice, …"
- Mutation paragraph replaced: "Each mutation of the script named here was run under bash 5.2.21 and 3.2.57, with the same result under both. These twenty-three fail the self-test: [list, plus 'letting a member's second number grow when `--update` is not given', minus the isolation item]. So do ten that keep a failure's message and drop only the failure: [same ten]. Two survive, each dropping one of the two counts from the number check: [explanation]."
- New paragraph: "The five that drop one of the four settings that isolate the fixture from the caller's git, or the line that exports them, each passed the self-test in an environment with no excludes file and git's default template. Each was also run with `target/` in ~/.config/git/ignore, in an excludes file ~/.gitconfig names, in one the system config names, and in the init template's info/exclude, one at a time. With each of those the unmutated script passes, and dropping XDG_CONFIG_HOME fails with the first, GIT_CONFIG_GLOBAL with the second, GIT_CONFIG_NOSYSTEM with the third, --template= with the fourth, and the export line with each of the first three."
- "Sixty-two", "5.3" and "only when" are gone.

Author asks the reviewer to look hardest at:
1. "Two survive" is scoped to the mutations the body names; L14, L16, U4 also survive and are not mentioned. Does the body need to say its set is closed?
2. The "without `--update`, and that … under it" sentence: unambiguous?
3. The isolation claim measured on this host only (HOME=/root), past tense, not universal.
4. The fixture-comment deviation.
5. Mapping from the body's descriptions to concrete mutants in fix-r1/mut.py, especially G1/G2 (comparison replaced by `false`) and K2 (`while` → `if`).

Files: /tmp/claude-0/-home-user/41f49fe0-9af3-5b27-a7a9-e4a4710cf8e0/scratchpad/fix-r1/ (mut.py, mut.log, reviewer-set.log, msg.old, msg.new, msg.readback, enforce.log, mut/).
