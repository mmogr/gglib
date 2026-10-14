# ADR 0021 — The loop guard ends the loop: under `note` it judges the newest turn, notes a run once, and ends the run if it trips again

- **Status:** Accepted
- **Date:** 2026-10-14
- **Depends on:** [ADR 0010](0010-the-loop-guard-reads-what-came-back.md),
  [ADR 0011](0011-stagnation-is-about-prose.md)
- **Supersedes:** nothing
- **Superseded by:** nothing
- **Log:** none

## Context

The proxy's loop guard rescans the replayed history on every request. Under
`note`, the default since #1052, a trip forwards the request with a fixed
`[gglib loop guard]` sentence appended to the last message. Two faults, read
from the code before this change:

1. **A trip was permanent.** `scan_history` returned at the first trip it
   found, so a transcript that once held three identical batches tripped on
   every later request — after the model changed approach, and after a person
   spoke. The test `six_adjacent_prose_repeats_still_stagnate_whatever_follows`
   pinned this and deferred the fix, because ADR 0011's 2026-08-27 amendment
   feared that judging the end of the transcript would let a trailing user
   message clear a *refusal*. Under `note` the costs are the other way round:
   a trip wrongly cleared costs one sentence not sent; a trip wrongly kept
   costs a misleading note on every later turn.
2. **Nagging is not ending.** A model that ignored the note was noted again
   on every later turn, and each turn was a full generation.

One measurement bears on both, and on what this change does not reach. One
task, Qwen3.8-27B with thinking on, Copilot CLI in plan mode, **one run**: the
client re-asked for the plan over and over, and each lap — announce "writing
the plan", re-read a few lines, emit the plan again — re-thought 3k to 18k
characters. That run spent 54 of its 82 minutes in 14 laps, and the guard did
not trip: the laps carried tool calls and their prose varied. The run itself is
not in this repository.

## Decision

1. **Under `note`, a verdict is about the newest turn.** Each detector's trip
   stands only while the newest turn that detector judged belongs to the run
   that tripped, so the scan reads the whole transcript instead of stopping at
   the first trip. A different tool-call batch clears a loop trip; a prose turn
   that does not trip clears a stagnation trip; prose stays transparent to the
   loop detector and tool turns to the stagnation detector, as they were. A
   user turn clears both once an assistant turn follows it; a user turn with
   nothing after it clears nothing, so a client that appends one to every
   request cannot clear a trip that way. When both detectors have a trip
   standing, the later one is reported.
2. **Under `refuse`, the first trip anywhere still stands.** `refuse` exists
   to cap a runaway session at threshold + 1, and ADR 0011's 2026-08-27
   amendment is the argument for not letting anything at the end of the
   transcript reopen it. A refused request never gets a model turn after its
   trip, so the two rules differ only on a transcript that crossed a trip
   under another mode. No existing test required this; the amendment and the
   cap did.
3. **Under `note`, a run that trips again is ended.** The proxy keeps no
   state: a run whose newest judged turn trips, and whose previous judged turn
   in the same run tripped too, was already noted by the request that carried
   that earlier turn (`ScanOutcome::tripped_before`). That request is not
   forwarded. The proxy answers it with a finished assistant message — status
   200, `finish_reason: "stop"`, no tool call, streamed when the client asked
   for a stream — saying in one sentence that the model repeated the same step
   after being told to change approach and that gglib's loop guard stopped the
   run. Every agentic client seen so far ends its loop on a turn with no tool
   call. The trip is logged and counted as any trip is, and the request's
   dashboard snapshot sets `loop_guard_ended`. A request whose history ends
   with a user turn after the trip is noted instead: a person spoke, and
   ending it would answer them without the model. No setting or mode is
   added; `refuse` and `off` are unchanged.

## Consequences

- Under `note` a looping run whose requests end with the tool result is
  capped at threshold + 2: one generation with the note, then the ending. A
  stagnation trip followed by tool turns is not ended: tool turns are
  transparent to the stagnation detector, so such a run is noted on each
  later request while the trip stands. Whether a tool turn should end a
  stagnation run is open.
- ADR 0010's 2026-09-18 note says a mistaken trip "adds a sentence instead of
  ending a session". A mistaken trip that repeats now ends the run after one
  note.
- That a run was noted is inferred, not recorded. A run whose first trip was
  never delivered — it was scanned under another mode, or its noted request
  failed on the way (the routes ADR 0011 lists) — is ended without the model
  having read the note.
- After a loop ending, a client that sends another request with no user turn
  is ended again at once: the message replays as a prose turn, which the loop
  detector reads through. After a stagnation ending the replayed message is a
  new prose turn that clears the trip, so that request is forwarded.
- A client that ends every request with a user message is noted on every
  trip and never ended, since the guard cannot tell that message from a
  person speaking.
- The loop guard's log cannot tell an ended request from a noted one: both are
  rows under `note`. Only the dashboard's `loop_guard_ended` can, per request
  and per process. Under `note` a stuck run that stops at the message logs
  two rows, not one per later turn.

## Kill criteria

- If a client is seen to go on after the message — a `loop_guard_ended`
  request followed by another in the same conversation with no user turn
  between — then that client does not stop at a turn with no tool call, and
  ending has to take another form for it. No counter reads this; it is seen in
  the request log or the dashboard's `recent_requests`.
- If a run is reported ended on a trip that was a false positive, the ending
  costs more than ADR 0010 priced a mistaken trip at, and the second trip
  should go back to a note. This is a report, not a counter.

## Out of scope

- **The lap.** The measured pattern — varying prose around small tool calls,
  re-asking for the same result — trips neither detector, and nothing here
  detects it. What would: a repeated-announcement detector (the same
  announcement across turns that call tools, which the stagnation detector
  ignores by design) or a repeated-small-read detector (the same few lines
  re-read lap after lap). It gets built only if a real client shows the
  pattern with a write tool available; the harness that produced it had the
  write tool denied.
- A column in the loop guard's log for an ended run.
- The agent path, whose guard already ends its run on the first trip.
