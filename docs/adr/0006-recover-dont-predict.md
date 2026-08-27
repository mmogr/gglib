# ADR 0006 — Recover, don't predict: the scheduler is removed and defaults come from the model

- **Status:** Accepted
- **Date:** 2026-08-12 (amended 2026-08-26 and 2026-08-27 — see the two notes
  following the 2026-08-26 postscript)
- **Depends on:** [ADR 0001](0001-runtime-capability-tiers.md),
  [ADR 0002](0002-defer-tool-call-constraint-to-llama-cpp.md),
  [ADR 0003](0003-defer-sampler-defaults-to-llama-cpp.md),
  [ADR 0004](0004-observe-the-sampling-boundary.md)
- **Supersedes:** [ADR 0005](0005-autonomous-closed-loop-and-reactive-grammar.md)
- **Superseded by:** nothing

## Context

ADR 0005 recorded a closed measurement loop in three organs: the ledger
observes, the **scheduler decides when to spend the GPU**, and the gate decides
whether anything changes. It worked. It was verified live. It is being removed
anyway, and this ADR records why, because "it worked" is not the same as "it
earned its cost".

A day of real use produced four findings, none of which the loop was built to
survive:

**The tuner kept rediscovering the published preset.** Run #38 applied
`qwen-coding`; a later sabotage run scored that same preset **0.836** against
**0.190** for the incumbent. Two campaigns, one conclusion: the author's numbers
were already the answer.

**The cost profile is inverted.** A 27B tune is 2–3 hours. A *broken* model is
slower still, because every turn runs to the token ceiling — so the models most
in need of correction are the most expensive to correct, and on a machine
someone is using, preemption can mean never finishing at all.

**Felt failures were never repaired.** A real chat died mid-stream on a parser
abort. gglib had the failure in hand and forwarded it. The only remedy on offer
was "accumulate fifty requests, wait for idle, run a two-hour benchmark."

**The literature does not support the effect being chased.** Renze & Guven
(arXiv 2402.05201) measured temperature 0.0→1.0 across nine models and found
no significant effect on problem solving (H(10)=10.439, p=0.403). That is
consistent with our own gate's refusals — the gate was working; the signal was
small.

## Decision

**Spend effort recovering from bad output, not predicting the configuration
that avoids it.**

Concretely:

1. **The idle-time scheduler is deleted.** No timer, no idle watch, no signal
   sweep, no `Settings.auto_tune`. Tuning becomes a thing a person asks for.
2. **Sampling defaults come from the model.** The GGUF's `general.sampling.*`
   keys, the publisher's `generation_config.json`, and a curated task-regime
   table — a claim about the model, not a claim about last week's traffic.
   This path already exists and already works; it is what produced the 27B's
   `published` origin.
3. **The measurement suite stays, demoted from a service to an instrument.**
   The A/B harness, the gate, the positive control and the A/A noise floor are
   all retained. They are how a proposed change is judged; they are no longer
   something that runs on its own initiative.
4. **The defect ledger stays, as diagnosis.** Per-model counters keep recording
   what actually fails — now including stream errors, truncations, empty and
   reasoning-only turns, dialect residue, unvalidatable schemas and
   normalization errors. Nothing acts on them automatically — they are read by
   a person, in `gglib proxy dashboard`, which lists only the models with
   something to report.

## What ADR 0005 got right and keeps

The **gate** is untouched and remains the only path by which a measured change
reaches a model's defaults. ADR 0005's central discipline — that a sampling
change which has not been observed firing has not been shown to fire — is
strengthened here, not weakened: with the scheduler gone, *every* change is
deliberate.

Its reading of **reactive repair as the permanent mechanism** was correct, and
is now the whole strategy rather than one half of it.

## Consequences

**Defect counters are per-process and reset on restart.** Persistence was built
(a `defect_windows` table, exponential decay by age, discard of evidence from a
foreign llama.cpp release) and then removed. Decay and build-scoping were never
features in their own right — they were the price of persisting at all,
invented to answer the staleness objection ADR 0005 itself raised. With no
automatic reader left, nobody needed yesterday's numbers, and the apparatus
would have sat dormant. Gathering evidence from real traffic is now a
deliberate sitting rather than something that accrues across restarts.

**A day's autonomy is gone, deliberately.** gglib will no longer improve a
model's defaults while nobody is looking. In exchange it no longer spends hours
of GPU rediscovering values its publisher already documented.

**The reactive path must now carry the weight.** Repair already pulls the
correct hard lever — `tool_choice: "required"`, which makes llama.cpp's own
schema-derived grammar non-lazy from the first token. Any escalation beyond
that should be chosen from measured failure rates, not from a candidate list;
the counters exist for exactly that, and the plan's own analysis killed three
of four originally-proposed rungs before any data was collected.

## Postscript, 2026-08-13 — decision 2 names three channels, and they are not peers

The decision above lists the GGUF's `general.sampling.*` keys, the publisher's
`generation_config.json` and the curated task-regime table as one mechanism.
Read against the code they are three different things, and only one of them
writes a model's stored defaults:

- **`generation_config.json` is the one that does.** It produces the
  `Published` origin, and only for HuggingFace imports —
  `services/model_import.rs` returns `None` for `ModelOrigin::LocalFile`, so a
  local GGUF never gets one however good its metadata.
- **The GGUF keys feed observability, not defaults.**
  `ModelSamplingDefaults::from_metadata` is read by `props.rs`, the sampling
  audit and `model explain`. llama.cpp applies those keys server-side, which
  the `/props` probe confirmed — so "the defaults come from the GGUF" is a true
  claim about llama.cpp and not about gglib.
- **The task-regime table seeds tune candidates.** It reaches a model's
  defaults only if a person runs a tune and the gate approves the winner. With
  the scheduler gone that is a manual, hours-long path.

None of this changes the decision — sending explicit values from a claim about
the model still beats predicting them from traffic. It corrects the sentence,
which reads as though three automatic channels are all writing defaults when
one is.

## Postscript, 2026-08-26 — the ledger records two things that are not failures

Decision 4 above scopes the ledger precisely: *"Per-model counters keep
recording **what actually fails**"*, and every counter it names is either a
gglib organ firing or a defect in the shape of the model's own output. A new
counter, `identical_result_repeats`, does not fit that sentence, so the sentence
is widened here rather than quietly outgrown.

It counts turns whose newest tool-call batch repeated the batch before it
**and got an equal result back**. Nothing failed. The model asked for the same
thing twice and the environment answered the same way twice — a fact about the
conversation, not a fault.

It is here because the ledger had no way to say whether a repeat was productive.
Every existing counter measures gglib's own reflexes; none measures whether a
turn accomplished anything, and the loop guard's verdict cannot supply it — that
verdict keys on `batch_signature`, which is blind to what came back. So a model
varying one argument each time escapes the guard forever, while a model
verifying its own edit is refused. Reading the `role: "tool"` half of the
transcript is the only available evidence, and it arrives free in a body the
proxy already parses.

This does not reopen prediction. The counter is per-process, non-persisted, read
by a person ~~, and acts on nothing — the same discipline as every counter
beside it~~. It is the measurement decision 4's own consequence section calls for:
*"Any escalation beyond that should be chosen from measured failure rates, not
from a candidate list; the counters exist for exactly that."* A corrective arm on
the input plane is the candidate; this is the rate that decides whether it is
built. If it stays near zero in real use, the arm is cancelled rather than
written.

That criterion carries a precondition, so a second counter carries it.
`repeats_not_evaluated` counts turns that repeated a batch whose results could
not be compared — a client omitting `id` on replayed calls, results that are not
contiguous, a parallel batch answered in part. A repeat gglib could not evaluate
is not a repeat that did not happen, and without the distinction a near-zero
reading is equally consistent with a rare phenomenon and with a join that never
once matched. Only the first of those licenses cancelling the arm. This is
ADR 0004's own standard applied to an instrument rather than to policy: an
observation that cannot report its own failure to observe is not yet an
observation.

The scope change is therefore: **the ledger records what a person needs in order
to decide what to build next, which is usually but not always a failure.** A
counter that is not a defect must say so where it is read — `gglib proxy
dashboard` prints both under an `observed` heading below the defect rows, in a
section titled *Per-model signals* rather than *Defects*.

> **Amended 2026-08-26 — readings either side of the loop-guard change are not
> comparable.** The criterion above stakes a build-or-cancel decision on
> `identical_result_repeats` reading near zero in real use. `LoopDetector` has
> since stopped counting a batch's occurrences session-wide and counts only
> back-to-back runs, so conversations that were rejected before — and therefore
> contributed nothing further — now continue and can go on repeating. The
> population feeding the counter is larger, and the rate reads higher for a
> reason that has nothing to do with models getting worse.
>
> The direction is conservative: a higher reading makes cancelling the arm less
> likely, not more, so the criterion cannot be tripped into a wrong *cancel* by
> this. But two weeks of data spanning the change is two populations, and a
> decision taken on it should say which side it was gathered from.

> **Amended 2026-08-27 — the arm was built, and not from this rate.**
> [ADR 0010](0010-the-loop-guard-reads-what-came-back.md) makes the loop
> guard's verdict read the same `role: "tool"` join these counters are computed
> from. Three things above stop being true, and one of them is the criterion
> this postscript exists to state.
>
> **"Acts on nothing" is struck.** The counters themselves still act on
> nothing — they are still read by a person and still decide nothing on their
> own. What has changed is that the *observation* they are computed from is no
> longer inert: a repeat whose answer changed is no longer a strike. The
> discipline the sentence claimed — "the same as every counter beside it" — is
> the part that fails, because no counter beside it shares an input with a
> verdict.
>
> **The counter does not count what this postscript says it counts.** "Turns
> whose newest tool-call batch repeated the batch before it" describes a
> comparison against the immediately preceding batch. The implementation keys a
> session-wide map by signature, so it also fires on a repeat with other turns
> in between — `A`, prose, `A` is reported. That was true when it was written.
> It matters more now, because the verdict uses a **run-scoped** comparison
> instead: the two read different populations, and are not two views of one
> instrument. ADR 0010 adds a third counter taken from the detector's own
> outcome for the reading its kill criteria depend on.
>
> **The cancel criterion is spent, not satisfied.** This postscript said the
> rate "decides whether it is built" and that a near-zero reading would cancel
> the arm rather than write it. The arm was written without waiting for the
> rate. What forced it was two verified consumer bugs — the guard refusing a
> file read, and refusing an agent that runs a command, edits and runs it again
> — and a third, an agent polling a build for output, which no rate could have
> answered because the failure was a false *rejection* rather than a missed
> detection. A criterion quietly outgrown is worse than one recorded as spent,
> so it is recorded as spent.
>
> Readings are again not comparable across the change, and in the opposite
> direction from the note above: a verdict that stops rejecting productive
> repeats lets more sessions continue, which enlarges the population again, and
> a verdict that reads the join changes which turns are counted at all.

## Notes

The scheduler's removal was mechanical but wide: 45 references across the Rust
core, CLI, daemon, HTTP DTOs, TypeScript types, the setup wizard, the settings
modal and the Makefile's `setup` target. The `auto_tune` settings row is
reclaimed at database setup — `Settings` is `#[serde(default)]` and would have
dropped it silently at load, but `save()` only writes the serialised struct, so
an orphan key would have sat in every existing database indefinitely, reading
like a setting that still did something.
