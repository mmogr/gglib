# ADR 0011 — Stagnation is about prose, and it forgets

- **Status:** Accepted
- **Date:** 2026-08-27
- **Depends on:** [ADR 0006](0006-recover-dont-predict.md),
  [ADR 0010](0010-the-loop-guard-reads-what-came-back.md)
- **Supersedes:** nothing
- **Superseded by:** nothing

## Context

`StagnationDetector` and `LoopDetector` sit behind one settings toggle, run on
the same two paths, and return the same HTTP 400. #926 rebuilt the second one
and left the first exactly as it was: a `HashMap<u64, usize>` of text hashes,
accumulated for the life of the session, never decremented.

Both of that detector's properties turned out to be defects, and each is a
separate failure.

**It counted narration.** Assistant text is read from every turn, including
turns that also carry `tool_calls`. Small models narrate almost every call, and
they narrate in the same words. "Let me look at the file." before each of six
`read_file` calls — six *different* files, which is ordinary work — was six
occurrences of one text, and the default threshold is 5. The sixth turn of a
Copilot session was a 400.

The text carries no information about whether the work is stuck. The same
sentence precedes a model reading six files and a model reading one file six
times. Only the tool calls distinguish those, and `LoopDetector` is already
reading them — including, since ADR 0010, the answers they got back.

Worse, it overruled a decision made deliberately next door. #923 raised the
read-only allowance to 16 so a coding agent could re-read a file without being
refused. A guard that cannot see a tool call at all was cutting that off at 6.

**It never forgot.** The proxy builds a fresh detector per request and replays
the whole transcript through it, so the verdict is a pure function of the
history. Once a text had occurred often enough *anywhere* in that history, every
later request was refused too. History only grows. A conversation that crossed
the threshold was over, and nothing the user or the model could do would clear
it.

## Decision

**A turn that called a tool is not recorded.** It is doing work, and the guard
that judges work is `LoopDetector`. This is a parameter of `record` rather than
a decision left to each call site, so the proxy and the agent loop cannot answer
it differently — the divergence `loop_guard`'s docs forbid.

**What remains is counted within a sliding window** of the last
`max_stagnation_steps × 4` recorded turns, rather than for the life of the
session.

The window is **derived from the threshold, not configured beside it**, because
the two are not independent: a window shorter than the repeats it takes to trip
makes the guard unable to fire at all. A fixed 20 would have silently disabled
stagnation for anyone who raised `max_stagnation_steps` to its ceiling of 100.

Four is the factor because oscillation is the binding constraint. Catching
A → B → A → B needs `max_steps + 1` occurrences of one text, which take
`2 × (max_steps + 1)` turns to arrive; the window must be at least that long or
the pair ages out before it can be counted. Four clears it at every threshold,
and at the default of 5 gives a 20-turn window against the 12 turns oscillation
actually needs — the same guarantee the session-wide tally gave, stated as a
number rather than as a side effect of never forgetting.

## Consequences

**Prose oscillation is still caught**, at every threshold, and a test pins it at
the ceiling as well as at the default.

**Tool-batch oscillation is now caught by nothing.** #926 gave cycles up in
`LoopDetector` deliberately — a run breaks on signature before any answer is
consulted — and this detector used to catch some of them by accident, whenever
the model happened to narrate its cycle repetitively.

That accident is not worth keeping, and the arithmetic says so. It fired on
exactly the narration described above, which is the *normal* behaviour of the
models gglib exists to serve, and caught a cycle only in the subset of cycling
sessions where the prose repeated too. It rejected far more ordinary work than
it caught loops. Recording the gap honestly is better than a net that is mostly
a hazard.

Nothing backstops cycles now. `max_iterations` bounds the agent path; the proxy
path is bounded only by the client's own loop, so a cycling small model against
Copilot burns tokens and GPU until the user stops it. Both candidate mechanisms
— a window over signatures, or decay — need a number nobody has measured, which
is why this ADR records the gap rather than closing it.

**On the agent path the detector is now nearly inert.** A turn with no tool
calls *is* the final answer there, so a run records at most one turn and cannot
reach a threshold above zero. That is acceptable rather than accidental: the
agent loop already has `max_iterations` and `LoopDetector`, and what stagnation
was contributing on that path was the narration false positive. It remains
meaningful on the proxy path, where prose turns are the ordinary shape of a
chat.

**A conversation can still die permanently, for one shape.** The window fixes
repeats that are *spread out*; it does not help repeats that were adjacent. Six
identical prose turns in a row remain in the transcript, and a replayed scan
trips on them at the same point every time, however much good work follows.

This is not specific to stagnation — `LoopDetector` has it too, and it is a
property of judging a whole transcript rather than the turn in front of you. The
remedy is that a user turn should break both detectors' state, which is a
separate change with its own argument to make.

## Kill criteria

- If `loop_guard_trips` shows stagnation rejections have effectively vanished on
  the proxy path, the guard is no longer paying for itself and the window should
  be widened or the detector retired. It should get *rarer*, not zero.
- If cycling sessions become a reported complaint, the gap above is the cause,
  and it wants a mechanism sized by a measurement rather than this ADR's
  reasoning.
- If `WINDOW_FACTOR` ever needs a value below 3, the oscillation guarantee it
  exists to preserve has been given up, and the ADR should say so instead.

## Notes

The two failing tests this change produced were both the change arriving, and
both are recorded rather than deleted. `test_stagnation_detected_integration`
asserted that identical narration alongside tool calls terminates a run; it is
now `test_narration_alongside_tool_calls_is_not_stagnation` and asserts the
opposite, because the opposite is the fix.

`a_stagnation_rejection_does_not_inherit_the_previous_turns_rescue` could only
be constructed with a turn carrying text *and* tool calls, which no longer
reaches the guard. Its invariant survived and got stronger: a stagnation
rejection can now only land on a prose turn, and a prose turn has already
cleared all three ledger bits by the time the guard runs.
