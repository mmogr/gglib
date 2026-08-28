# ADR 0010 — The loop guard reads what came back

- **Status:** Accepted
- **Date:** 2026-08-27
- **Depends on:** [ADR 0001](0001-runtime-capability-tiers.md),
  [ADR 0006](0006-recover-dont-predict.md)
- **Supersedes:** nothing
- **Superseded by:** nothing

## Context

`LoopDetector` decides whether a tool-call batch is a loop by comparing
`batch_signature` — tool names plus canonicalised arguments — against the
current unbroken run. It cannot see what the batch got back. #923's commit
message named that as the cause and #926 fixed only the other half of it,
turning session-wide tallies into run-length counting and recording that this
half was still open.

Two symmetric failures follow. An agent polling for output — `get_terminal_output`
on a running build, a test runner, any "is it done yet" call — issues the *same*
batch back to back, so run-length counting refuses it exactly as a session-wide
tally did. And a model that varies one argument each time escapes the guard
forever. The verdict was measuring the wrong thing in both directions.

This is a **Tier B — Policy** behaviour. ADR 0001's Tier B list already names
"the cross-turn loop and stagnation detectors in `domain::agent`", so nothing
about the classification changes: it never gates on `RuntimeCapabilities`, and
it owes no deletion criterion. What changes is what it reads.

## Decision

**A repeat is a strike only when its answers matched the previous occurrence's,
in the same run.**

- Same call, same answer, is stuck. Same call, different answer, is progress
  that happens to look alike, and restarts the run.
- **Unknown answers never rescue.** A batch that went unanswered, or answered
  only in part, produces no hash. An answer nobody can read is not evidence of
  progress, and treating it as such would let any client that omits `id` on
  replayed calls switch the guard off. It is also what makes a detector nobody
  records into behave exactly as it did before it could read anything, which is
  how every test written against the old verdict still stands.
- **A ceiling, for anything that changes something.** A changed answer restarts
  the run, so on its own it would exempt any tool whose output carries a clock,
  an elapsed time, a progress counter or a random id. Observation-tier batches
  are exempt anyway — that tier exists on the ground that repeating a call which
  changes nothing is free. Everything else may be carried by changing answers
  only while the run itself stays inside the read-only allowance,
  `max_observation_steps`. That number is **reused, not invented**: there is no
  measurement behind a new one, and inventing one is what this codebase has
  repeatedly refused to do.

### The split, and why it is two calls

The two call sites cannot ask the question at the same moment. The proxy scans a
finished transcript and knows a batch's answers when it counts it. The agent
loop counts a batch *before* it executes, so the answers do not exist yet. Any
design where `check` needs this batch's answers works in one path and cannot
work in the other — which is the divergence `loop_guard`'s module docs forbid
when they promise parity "by construction — there is one detector
implementation, not two".

So the verdict stays in `check` and the reset moves to `record_results`. Both
paths call `check` where the verdict is needed and `record_results` once the
answers are in hand; the proxy simply calls them back to back. A `BatchRecord`
returned by `check` and required by `record_results` is what stops the answers
being attributed to a different batch than the one counted. Getting that
backwards inverts the measurement silently, and this codebase has made that
class of mistake once already, with a global id map, and caught it only in
review.

That promise is now checkable rather than argued, within a limit worth stating.
A test drives the same turns through `scan_history` and through a bare
`LoopDetector` exercised in the agent's order — check, then record — and asserts
both refuse at the same turn. It holds the *detector's* half of the protocol on
both sides, and it was written against a sequence that passed under a
deliberately broken proxy, then rewritten until it failed; an agreement test
that agrees for the wrong reason is worse than none. What it does not hold is
`gglib-agent`'s own wiring, because it does not run the agent loop. That is held
separately, by `test_changing_tool_results_do_not_trip_the_loop_guard`, which
fails if `run` stops calling `record_results`.

## Consequences

**This is the corrective arm ADR 0006's postscript said would be decided by a
measured rate, and it was not.** That is worth stating rather than glossing:
what forced it was three verified consumer bugs about the guard refusing
ordinary work — a file read, an agent running a command between edits, and an
agent polling a build for output — none of which a rate could answer, because
the failure was a false *rejection* rather than a missed detection. The criterion is spent, not
satisfied. See the amendment on that ADR.

**What this gives up, in order of how much it costs:**

1. **A drifting byte buys tolerance.** Any tool whose output carries a clock or
   a counter rescues its run on every occurrence. For a mutating batch that
   raises the ceiling from 3 to 16; for a read-only batch it removes the ceiling.
   `cargo test`'s `finished in 0.31s` is enough. This is the largest cost and the
   reason the ceiling exists at all.
2. **A quiet poll still trips.** Sixteen identical answers in a row is a loop by
   this definition, so an agent watching a compile that prints nothing for two
   minutes is still refused at the observation ceiling. Result-awareness helps
   only once the output moves — so "polling a build is fixed" is not the claim.
3. **A rescue arrives one turn late.** `record_results` runs after `check`, so a
   run already at its threshold is refused before the occurrence that would have
   changed the answer can report it.
4. **Cycles are untouched.** A → B → A → B, and equally A → A → B repeating,
   break the run on *signature* before any answer is consulted. #926's accepted
   cost stands exactly as it was. It looks as though reading the answers should
   have helped here and it does not.

**A rejected alternative**, recorded because it is the obvious one: count
*distinct* answers within a run, so a two-state spinner reads as stuck. Rejected
— the set grows with the run, and a two-element cycle is the cycle problem
again, with no measurement behind any threshold on it.

**What still bounds a rescued run.** `max_iterations` bounds the agent path
outright. The proxy path is bounded only by the client's own loop, with
`StagnationDetector` still catching a model that repeats its *prose*
session-wide — and a tool-call-only turn carries `content: null`, which that
detector ignores by design.

> **Amended 2026-08-27 — that bound is narrower than this sentence claims, and
> narrower still now.** `StagnationDetector` read assistant text from turns that
> *also* carried tool calls, so what it mostly caught on the proxy path was
> narration, not prose. At the default threshold of 5 that refused a Copilot
> session on its sixth narrated call, and it overruled the read-only allowance
> #923 had just raised to 16.
>
> [ADR 0011](0011-stagnation-is-about-prose.md) stops it recording any turn that
> called a tool, and windows what remains. So the residual bound on a rescued
> run is smaller than stated above: it is prose only, and a rescued run consists
> of tool-calling turns. Read this paragraph as naming the gap rather than
> filling it.

**What the counters mean now.** `identical_result_repeats` and
`repeats_not_evaluated` keep their session-wide scope and remain facts about the
conversation: they key a map by signature and fire on a repeat with other turns
in between. The verdict uses a **run-scoped** comparison instead. The two read
different populations and are not two views of one instrument, which is why the
reading the kill criteria below depend on is a third counter, `repeats_rescued`,
taken from the detector's own outcome. That one is a fact about gglib's reflex
rather than about the conversation — which is what the ledger was chartered for
before ADR 0006 had to widen it.

## Kill criteria

- If `repeats_rescued` dwarfs `identical_result_repeats` in real use, the join is
  being defeated by drifting output rather than measuring progress. Narrow the
  rescue to tools with stable output, or remove it.
- If sessions are still refused while polling a build that is quiet, the
  observation ceiling is the wrong instrument for polling and wants its own
  change rather than another adjustment to this one.
- If `repeats_rescued` stays at zero in real use, the ceiling is complexity
  without a customer and should be deleted along with `Run::total` — the
  ceiling can only fire on a run that a changed answer reset, so no rescues
  means no ceiling trips. The trip itself is deliberately *not* distinguishable
  from a strike trip: `AgentError::LoopDetected` carries only the signature and
  the remedy is identical, so a second error variant would be a distinction
  with no reader. The rescue counter is the readable proxy for it.

## Notes

The results join moved from `gglib-proxy` into `gglib-core` ahead of the verdict
change, as a commit with no behaviour in it, so that the diff which changes what
a user sees is small enough to argue with. The pairing rule — pair each call
with its own answer, sort the pairs, hash — is the part worth stating once:
sorting bare answer hashes would meet the ordering goal, since `batch_signature`
sorts too, but it severs which call produced which result and a two-call batch
whose answers swapped would compare equal.

The windowing stayed in the proxy, because it is a wire-format concern. gglib
mints synthetic tool-call ids for dialect models and `DelimitedToolCallParser`
restarts at zero on every response, so `call_qwen_0` recurs on every turn of a
replayed conversation and a global index would resolve every occurrence of a
batch to the same result. The agent loop has no window to find: its results
arrive one per call, in order.

### First reading, 2026-08-28

The first evaluation of these three criteria. Same session as
[ADR 0009](0009-fit-the-context-to-the-machine.md)'s first reading, which
carries the provenance and the arc's defect count: ten requests, one model,
Qwen3.8-27B via VS Code Copilot, read from `gglib proxy dashboard`, per-process
counters that reset on restart and cannot be re-read.

**All three read from one line, and the note says so rather than presenting four
zeros as four observations.** `Per-model signals (this proxy run)` printed
`none across 10 request(s)`. The dashboard emits that line only when `is_clean`
holds for every model, and `is_clean` requires `repeats_rescued`,
`identical_result_repeats`, `repeats_not_evaluated` and `loop_guard_trips` to be
zero together — so one printed word is the source of every number below.

- **If `repeats_rescued` dwarfs `identical_result_repeats`** — **0 and 0 across
  10 requests, 2026-08-28.** A ratio between two zeros is not a reading, in
  either direction. **OPEN.**
- **If sessions are still refused while polling a build that is quiet** — **0
  loop-guard trips across 10 requests, 2026-08-28**, and no build-polling
  occurred in the session, so the shape this criterion is about was never
  exercised. A zero here is the absence of the test, not its result. **OPEN.**
- **If `repeats_rescued` stays at zero in real use** — **0 across 10 requests,
  2026-08-28.** This is the criterion a small denominator most easily misleads,
  because it is *satisfied* by zeros rather than by a value: at n=10, "the
  ceiling has no customer" and "nobody exercised the ceiling" produce the same
  number, and only the first licenses deleting the ceiling and `Run::total`.
  It should not be acted on until the denominator is large enough to exclude the
  second reading. **OPEN.**

**All three remain OPEN**, and the third is the one to be most careful with: it
is the only criterion in this arc where the reading taken so far points, on its
face, at deletion.
