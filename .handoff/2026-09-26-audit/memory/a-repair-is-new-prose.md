---
name: a-repair-is-new-prose
description: "2026-09-20 (#1091): fixing a reviewer's finding introduced two fresh false claims — a new absolute, and a commit message true at the tip but not at its own commit; re-review what you wrote to repair, not just what you wrote first"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 6cf7bfa9-4b9f-430f-a80d-1cc07a675653
  modified: 2026-09-20T08:45:23.109Z
---

Four review rounds on gglib #1091 left the code almost untouched and found
eleven bad sentences. Two of them **did not exist before the repair**:

- Correcting "records nothing at all — no sink of any kind" (false: the
  tuning benchmark reads the loop's `AgentError` as `loop_detected`), I wrote
  "the one place an agent trip is read at all is the tuning benchmark".
  Also false — a trip leaves as `AgentEvent::Error` to the SSE stream and the
  CLI. A new absolute, introduced while removing an absolute.
- Correcting a quantifier in a **commit message**, I made that message
  describe the field doc as the *branch tip* leaves it. Its own commit
  documents one difference; the second arrives four commits later. The same
  reword made commit 5 assert a reason that commit 6 then silently replaced.

**Why:** a repair is written under the belief that the sentence has just been
checked, so it gets less scrutiny than the original — while being exactly the
kind of sentence (new, confident, corrective) that carries the most risk.
Session 17's number said this already: must-fix findings concentrate in newly
written paragraphs, and corrected prose stays correct — but only if the
correction is itself put through the pass.

**How to apply:**
- Put every repair through the same test as the original: name the one
  observation that would make it false, and go look for that. Especially a
  repair that *narrows* a claim, where the temptation is to narrow to the
  first true-sounding thing rather than to the evidence.
- Read a commit message against `git show <that hash>:<the file it
  describes>`, never against the tip. A message edited during a rebase is a
  claim about a tree you are not currently standing in.
- When a docs commit corrects an earlier commit's stated reasoning, move the
  correction into the commit that made the claim instead. A later silent fix
  hides it from anyone reading the history forward. Restructuring beats
  rewording here.
- A phrase search cannot find the sites that do not share your wording.
  Sweep by *subject* — every document that enumerates the struct, names the
  type, or describes the behaviour — and use `git log -L` on a list to see
  whether it has been maintained as exhaustive.

Related: [[a-design-memo-ages-like-an-issue-body]],
[[a-grep-finds-callers-not-behaviour]], [[a-name-based-gate-is-never-complete]],
[[a-dev-check-log-names-its-tree]].

**Measured again, much harder, 2026-09-22 (ggchat, modelpipe-ffi 0.4.0 uptake).**
Five adversarial rounds. Blockers by round: **10 → 6 → 1 → 2**. In round two,
**four of the five blockers were written by round one's repair**; round three's
single blocker was written by round two's repair; both of round four's were
written by round three's. The original change was never the problem — the code
was right from round one and no round found a behavioural defect in it.

Every one of those repair-introduced defects had the same shape: **a closed
claim written after verifying something narrower than the sentence asserted.**
Examples, all mine:

- "Every caller hands it a ticket modelpipe has already read" — one caller
  digests a ticket straight from the Keychain.
- "every call site … at every commit that ever reached `main`" — I had checked
  `Sources/` only; test call sites passed `.uppercased()`.
- "had two tests for it; both went down" — unsubstantiable under any reading.
- "the two things Rust cannot do portably" over a list of three.
- "the only layer that knows the file's name" — two layers see it.
- Twice, a claim about what canonicalisation "newly folds" (case, then address
  order) — both false because the parse never ran on the Swift side at all.

**How to apply.** After repairing a finding, put the repair through the *same*
pass as original prose, and specifically re-derive the *reason* clause, not
just the conclusion — twice the conclusion was right and the "because" was
false, once contradicting a sentence 17 lines up in the same file. Before
writing "every", "only", "both", "two", "nothing", "no … at all": name the
exact command that would falsify it and run that, not a narrower one.
