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
property of judging a whole transcript rather than the turn in front of you.

> **Amended 2026-08-27.** A user turn now clears both detectors, which stops
> repeats *either side* of an interjection accumulating into one span. It does
> **not** rescue a transcript that already tripped, and the paragraph above
> originally implied it would. `scan_history` returns on the first trip it
> finds, so it never reaches a later user turn to be cleared by it.
>
> Recovering that means judging the end of the transcript rather than its worst
> moment — and a verdict that a trailing user message can clear is one most
> clients could clear by accident, since a trailing user message is the ordinary
> shape of a chat request. That needs its own argument and a measurement, and is
> deliberately not made.

## Kill criteria

- ~~If `loop_guard_trips` shows stagnation rejections have effectively vanished
  on the proxy path, the guard is no longer paying for itself and the window
  should be widened or the detector retired. It should get *rarer*, not zero.~~

  > **Restated 2026-08-28 — `loop_guard_trips` cannot show that.** It is one
  > tally over both detectors ~~and both paths~~. `ModelDefectCounts` documents it as
  > "requests the loop/stagnation guard rejected before dispatch", and the proxy
  > records `LoopDetected` and `StagnationDetected` through the same
  > `loop_guard_tripped` flag before they diverge into two error bodies;
  > `record_loop_guard_trip` takes no discriminator. A stagnation rejection is
  > not separable from a loop one in that number~~, and the proxy path is not
  > separable from the agent path~~.
  >
  > Read as written, the criterion is answerable only where the combined tally is
  > zero — which is the reading taken below, and which says nothing about whether
  > stagnation *specifically* has become rare. A criterion nobody can read is a
  > promise that the decision will be revisited, and it will not be.
  >
  > **What stands in its place, and it is deliberately weaker: if
  > `loop_guard_trips` reaches zero across a denominator large enough that a
  > rejection would have been expected, then neither detector is paying for
  > itself**, and the window should be widened or the detector retired. That is
  > the most this counter supports. It cannot single out stagnation, so it cannot
  > retire this detector alone.
  >
  > Separating the two needs a counter that does not exist. Queued as
  > [#947](https://github.com/mmogr/gglib/issues/947) rather than assumed, and
  > the original criterion above is struck rather than deleted so that what was
  > wanted stays legible.

  > **Amended 2026-09-17 — the tally now says which detector, and it never
  > covered two paths.** [#947](https://github.com/mmogr/gglib/issues/947) has
  > landed its first half. `record_loop_guard_trip` takes the detector that
  > raised the trip, `ModelDefectCounts` carries `loop_guard_loops` and
  > `loop_guard_stagnations` beside `loop_guard_trips`, which stays as their sum,
  > the snapshot's `loop_guard_tripped` flag is now `loop_guard_trip` and names
  > the detector, and `gglib proxy dashboard` prints the parts under the sum. The
  > sentences above that say there is no discriminator, that the two are not
  > separable, and that the counter does not exist were true when written and
  > are not now.
  >
  > "Both paths" is struck above, here and in the first reading, because it was
  > wrong when written. The agent loop runs the same two detectors
  > (`gglib-agent`'s `agent_loop.rs`), but a trip there becomes an error event and
  > reaches no counter, so this tally has only ever seen the proxy's pre-dispatch
  > scan, which is also all the struck criterion asked about. #947's second
  > half, separating the paths, is therefore not a split of this number but an
  > instrument that does not exist yet:
  > [#1091](https://github.com/mmogr/gglib/issues/1091).
  >
  > **What this does not change is the criterion.** The struck one can now be
  > *asked* — within one proxy run. These counters still reset with the process,
  > which is what the first reading below calls "cannot be re-read", and a
  > criterion about stagnation becoming rare needs a denominator no single run
  > will supply. So the weaker restatement stands, and the original is not
  > reinstated here; that waits for a reading that survives a restart
  > ([#1052](https://github.com/mmogr/gglib/issues/1052)).

  > **Amended 2026-09-18 — the number has changed its subject, and the reading
  > still does not survive a restart.** [#1052](https://github.com/mmogr/gglib/issues/1052)'s
  > first half has landed. The guard's answer is now a setting,
  > `loop_guard_mode`, with three values, and its default is `note`: a tripped
  > request is **forwarded**, with a fixed note appended to the last message
  > saying what repeated. `refuse` is the old HTTP 400, and `off` is the old
  > off.
  >
  > So `loop_guard_trips` no longer counts *rejections*. It counts requests the
  > guard **acted on** — noted or refused — and the same is true of
  > `loop_guard_loops` and `loop_guard_stagnations`. Every criterion on this
  > page that reads those numbers now reads a count of interventions.
  >
  > That matters more than it sounds. Read against the old subject, the struck
  > criterion — "stagnation rejections have effectively vanished" — would be
  > satisfied the moment the default stopped rejecting, by a change in what the
  > proxy does rather than by anything about the models. The restatement above
  > is not reinstated and is not weakened further: it asks whether the counter
  > reaches zero across a large enough denominator, and under `note` that is
  > still the right question, now about interventions.
  >
  > The reason the original criterion is *still* not reinstated is unchanged
  > and is the one below: these counters reset with the process. #1052's second
  > half is the event log that outlives it.
  >
  > **The restatement above quotes words that have since changed.** It cites
  > `ModelDefectCounts` documenting the counter as "requests the loop/stagnation
  > guard rejected before dispatch"; that doc now reads "requests the
  > loop/stagnation guard **acted on**", corrected in the same change as this
  > note. The quotation is left as it was — it was accurate when the
  > restatement was written, and it is the evidence for the argument that
  > paragraph makes — but a reader following it to the source will find the new
  > wording, not the old. (The correction ships in the same pull request as
  > this note, a commit later.)
  >
  > **One limit on what an intervention is worth, because it bears on what this
  > number counts.** The note is delivered inside the last message's content, so
  > a chat template with no branch for the `tool` role drops the whole last
  > message on an agentic tail and the note with it. Such a request is still
  > counted here as an intervention, and the model saw nothing. Two of the 65
  > llama.cpp templates that render at all behave that way (Phi-3.5-mini,
  > rwkv-world) — but what a deployment actually renders is the template inside
  > its own GGUF, which is usually not one of those 69, so the share of real
  > traffic affected is **unmeasured**, not two in sixty-five. A
  > model behind either never sees a tool *result* either, so it cannot run a
  > tool loop meaningfully in the first place; on a chat tail, where stagnation
  > trips, both render the note in place. The event log in #1052's second half
  > records the action taken, not whether the model read it, and no counter
  > here can close that gap.
  >
  > The same bound by a second route: a tripped conversation that also exceeds
  > the context budget is refused as `context_length_exceeded` before the note
  > is rendered, so it too is counted as an intervention that delivered
  > nothing — and that is by construction the shape most likely to trip the
  > guard. Stating the template case and not this one would leave the bound
  > half-drawn.
  >
  > One consequence worth stating because it is a cost, not a benefit: under
  > `note` a genuinely runaway client burns a full generation per stuck turn
  > instead of being stopped at threshold + 1. That is the trade the issue
  > asks for — a refusal is terminal for a client with no recovery path, and
  > ADR 0011's own context records one ending a Copilot session on turn six —
  > and the event log is what makes the cost auditable.
- If cycling sessions become a reported complaint, the gap above is the cause,
  and it wants a mechanism sized by a measurement rather than this ADR's
  reasoning.
- If `WINDOW_FACTOR` ever needs a value below 3, the oscillation guarantee it
  exists to preserve has been given up, and the ADR should say so instead.

### First reading, 2026-08-28

The first evaluation of these three criteria. Same session as
[ADR 0009](0009-fit-the-context-to-the-machine.md)'s first reading, which
carries the provenance: ten requests, one model, Qwen3.8-27B via VS Code
Copilot, read from `gglib proxy dashboard`, per-process counters that reset on
restart and cannot be re-read.

Mapping the three criteria to their instruments is what produced the
restatement above. Two of them turn out not to be readings at all, which is
worth saying plainly rather than leaving as three lines that look alike.

- **The guard is no longer paying for itself** — **0 loop-guard trips across 10
  requests, 2026-08-28**, covering both detectors ~~and both paths~~, read against
  the restated criterion rather than the struck one. Ten requests is nowhere
  near a denominator at which a rejection would have been expected, so this is a
  zero with nothing yet behind it. **OPEN.**
- **If cycling sessions become a reported complaint** — **0 complaints, 1
  session, 2026-08-28.** This one is a report and not a counter, and it says so:
  nothing in the ledger observes a cycle, which is the gap the Consequences
  section above records as backstopped by nothing. **OPEN.**
- **If `WINDOW_FACTOR` ever needs a value below 3** — **not applicable.** A
  tripwire consulted when the constant is changed, not a reading taken from
  traffic. It has not been changed. Alone among the nine criteria in this arc it
  needs no denominator, and it is the only one that was readable the day it was
  written.

**Both live criteria remain OPEN**, and one of them has been narrowed by the
restatement: until [#947](https://github.com/mmogr/gglib/issues/947) lands,
nothing can retire `StagnationDetector` on its own evidence.

> **Amended 2026-09-17.** #947's detector split has landed (the note under the
> first criterion says what it did and did not change). What still stands
> between this detector and its own evidence is a count that outlives the
> process (#1052) and the path that is not counted at all (#1091).

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
