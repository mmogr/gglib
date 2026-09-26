VERDICT: FAIL (build lens, round 1 of this session, reviewed 6e8e7966; line numbers are at 6e8e7966's script)

Both fixes from the last FAIL hold (fail-closed number check; the (outside-members) 0 1 fixture). The FAIL is for wording defects 1-3 (required) and 4 (low), plus a recommended fixture R1.

**1. (required) "a counter that exits 1" claims more than the check tests.**
Where: scripts/check_lint_inheritance.sh:37 (header), :516 (✓ line), :421/:426/:430 (fixture comment and messages); scripts/README.md:146; 7931a83b body ("It has a counter that exits 1, a baseline row listed twice").
What: the check does not look at the counter's exit status; what fails the row is that no count comes out. With the fixture's counter changed to `COUNTER='print "0 0\n"; exit 1'` the check passed with rc 0. The fixture's counter (`exit 1`) prints nothing. The rule sentence "A count that failed … fail[s] the check" is true, because the real counter prints only at the end and `die`s before that.
Fix: "a counter that exits 1" → "a counter that exits 1 without printing a count":
- header :37 "…; and a counter that exits 1 without printing a count, a row listed twice…"
- ✓ line "…, an empty listing, a counter that exits 1 without printing a count, a row listed twice…"
- README :146 "…; a counter that exits 1 without printing a count, a row listed twice…"
- 7931a83b body "It has a counter that exits 1 without printing a count, a baseline row listed twice, …"
- (optional, consistency) the :421 comment and :426/:430 messages.

**2. (required) "a baseline field that is not a number" is true only of the second and third fields.**
Where: scripts/check_lint_inheritance.sh:27-28; scripts/README.md:140; 7931a83b body paragraph 3 ("A count that failed, a baseline row listed twice and a baseline field that is not a number each fail the check").
What: only $2 and $3 are checked; field 1 is the name, fields after the third are ignored (`crates/gglib-db 9 0 x` gave ✅ rc 0 on the real tree).
Fix: in all three places "a baseline field that is not a number" → "a baseline row whose second or third field is not a number". Header :27-28 becomes: "A count that failed, a baseline row listed twice and a baseline row whose second or third field is not a number each fail the check, and `--update` then writes nothing." Optional: in the fixture lists (header :38, README :147, ✓ line) "a field that is not a number" → "a count field that is not a number".

**3. (required, low) the closing message states a cause that did not happen.**
Where: scripts/check_lint_inheritance.sh:553-556 (the MSG heredoc).
What: printed after every failed check; after "❌ …: a count or its baseline row is not one number each" it says "A member stopped inheriting the workspace lints, or a row allows more than its baseline", false there, and advises `--update`, which refuses a doubled row or an unreadable file. Same after stale-row and empty-listing failures.
Fix: replace the four lines with:
```
The ❌ lines above say what failed. If a row allows more than its
baseline, fix the lint rather than allowing it. If an allow is the right
answer, give it `reason = "…"` and raise the row's first number with
./scripts/check_lint_inheritance.sh --update, in the same change.
```

**4. (low, older) "whole-line comments not" is not true of block comments.**
Where: header :14 ("`cfg_attr` forms included, whole-line comments not"); README :127-128 ("`cfg_attr` forms included and whole-line comments not").
What: a line holding only `/* #[allow(clippy::x)] */` is counted; only lines starting with `//` are dropped.
Fix: "whole-line comments not" → "whole-line `//` comments not" in both places.

**R1 (recommended; the orchestrator asks for it).** The ✓ line's "growth in either number of either row … refused" is literally true, but the member row's second number is refused by a fixture only under `--update` (`good 15 6`); the plain-check path CI runs is covered only because it shares code with the outside row. Mutant P1 (member bare compared only under `--update`) SURVIVES the self-test.
Fix: after the :390 fixture add `awk '$1 == "good" { $3 = 6 } 1' <<<"$expected" >"$dir/baseline"`, run `check` without update, and assert a non-zero exit and "good: 7 lints allowed without a reason, baseline 6".

R2 (optional; the orchestrator does NOT ask for it): no fixture runs `--update` over a stale row (mutant U4 survives). No sentence claims it. Leave it.

Reviewer's mutation runner (copy it; do not edit the reviewer's files): /tmp/claude-0/-home-user/41f49fe0-9af3-5b27-a7a9-e4a4710cf8e0/scratchpad/review/c3-build-r1/w/mut.py (writes mutants under w/mut/, runs --self-test under both bashes; KILLED only when rc≠0 and the first line is "❌ self-test:"). Its log: w/mut.log. Equivalent survivors found: L2, L3 (drop "$2"/"$3" from the number check), L14 (base_lints awk first match only); L16 (number check moved above the :-0 defaults) survives but still fails closed and no sentence claims either behaviour; U4 (R2); P1 (R1).
