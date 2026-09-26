---
name: a-mutation-result-is-a-claim-about-a-suite
description: "2026-09-18 (gglib #1052 B1): a SURVIVED can mean the runner ran the wrong test binary, and a KILLED can be scored by a different test than the one named — read which tests failed, not just the verdict"
metadata: 
  node_type: memory
  type: project
  originSessionId: 79c9bf76-59a0-4045-954d-b4bef97a2e2a
  modified: 2026-09-18T10:10:29.976Z
---

Writing gglib #1052's B1, a thirteen-mutation run reported **two survivors**.
Both were real code changes that a named test should have caught. Applying one
by hand and running the test directly killed it immediately — so the tests were
fine and the *runner* was wrong: those two mutations were pointed at
`integration_loop_guard_note`, and their tests had moved to a new
`integration_loop_guard_counting` when that file was split for the 300-line
cap. The runner built and ran a real suite that simply did not contain the
tests, found nothing failing, and printed SURVIVED.

The same run scored a **KILLED** that was also wrong in the other direction.
Mutation 12 blanked the note's marker constant, and the test named for the
marker asserted `text.starts_with(MARKER)` — true of every string when
`MARKER` is `""`. A different test caught it. The runner only noticed because
it prints "expected but did not fail" beside every kill.

**Why:** a mutation's verdict is a claim about a *suite*, not about the code.
"No test failed" has at least three causes — the code is untested, the suite
does not contain the test, or the assertion cannot fail — and only the first is
the one being measured. Same family as
[[cargo-shared-target-stale-tests]] and
[[mutation-restore-must-refresh-mtime]]: a result read off something that was
not what you thought you were running.

**How to apply:**
- Name the expected test per mutation and have the runner **print when a kill
  did not include it**. A kill by the wrong test is a finding, not a pass.
- After splitting or renaming a test file, re-check every mutation's suite
  selection. `cargo test --test <name>` fails silently-useful: it runs, it
  passes, it proves nothing.
- Treat a survivor as a bug in the runner until disproved: apply it by hand and
  run the named test alone before writing "no test covers this".
- Never assert `starts_with`/`contains` against a **constant** the mutation can
  empty. Assert the literal, and pin the constant to the literal separately.
- Read the failure lists, not the summary column. The summary is what a
  contaminated or misdirected run looks like when it looks perfect.

**Two more shapes, 2026-09-18 (#1052 B2, PR #1095):**
- **A test must reach the line it guards.** `a_batch_of_zero_is_taken_as_one`
  passed with the `.max(1)` clamp removed. It recorded and stopped before the
  writer's loop ever asked the queue for a batch of zero, and the stop arm
  drained it. Only a mutation showed that. The fixed test waits until the
  loop is reached and asserts the queue is still open, and fails there when
  the clamp is removed. When you write a test for a guard, apply the mutation
  by hand once.
- **Some fixes no suite can pin.** Moving a check back outside a lock (a scan
  racing the writer's stop) passes all 97 tests. The evidence is a
  reviewer's out-of-tree concurrent experiment *with a positive control*:
  171/200 runs lost a scan with the old order, and 0/200, repeated, with the
  fix. Say that in the PR's not-tested section, both numbers, rather than
  implying a test holds it.

Related: [[mutation-restore-must-refresh-mtime]], [[cargo-shared-target-stale-tests]], [[gglib-adversarial-review-loop]].
