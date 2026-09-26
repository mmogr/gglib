---
name: an-exactly-when-claim-is-a-theorem
description: "prose that says \"X exactly when Y\" failed three reviews in a row on edge cases (gglib B1, 2026-09-25); state an equivalence only in the form a test checks, and repair a false sentence by narrowing or deleting it"
metadata:
  node_type: memory
  type: feedback
  originSessionId: d2f123ee-56ab-4c4c-a881-dbf2eef763a6
  modified: 2026-09-25T18:26:10.542Z
---

B1 (#1145) said "a page may change something exactly when it may read the answer". It failed three focused reviews on successive edge cases: the endpoint's own page, `Origin: null`, and extension origins. Each repair added a qualifier, and each qualifier was a new false claim.

**Why:** an "exactly when", "iff" or "only" sentence is a theorem about every input. A reviewer needs only one counterexample.

**How to apply:**
- State an equivalence only in the form a test checks, and name the test.
- When a reviewer shows a sentence false, delete it or narrow it to what was measured. Never add an "unless" or a mechanism you did not measure.
- A repair gets the same review as a first draft ([[a-repair-is-new-prose]]).
- Implementer briefs carry this rule. Brief each fix agent with the reviewer's exact wording when one exists.
- **Rewriting history into the present tense is new prose.** On 2026-09-26 the gglib comment cleanup PRs (I-1, I-3, I-4) all missed round 3 on sentences like this. For example, "every request path calls apply" is false for `/api/chat`, and "`enable` mints a code unconditionally" is false because it mints one only when an invite is requested. Brief the fixer to delete a claim before restating it, and to grep the callers before writing "every", "both" or "only".
