# ADR 0006 — Recover, don't predict: the scheduler is removed and defaults come from the model

- **Status:** Accepted
- **Date:** 2026-08-12
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

## Notes

The scheduler's removal was mechanical but wide: 45 references across the Rust
core, CLI, daemon, HTTP DTOs, TypeScript types, the setup wizard, the settings
modal and the Makefile's `setup` target. The `auto_tune` settings row is
reclaimed at database setup — `Settings` is `#[serde(default)]` and would have
dropped it silently at load, but `save()` only writes the serialised struct, so
an orphan key would have sat in every existing database indefinitely, reading
like a setting that still did something.
