# defects

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-defects-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-defects-complexity.json)

<!-- module-docs:start -->

Per-model defect counters — the Tier C signals the closed loop steers by.

The proxy records defect *events* (a loop-guard trip, a tool-call repair)
as they happen; the auto-tune scheduler reads *rates* over the traffic
since its last look and decides whether a model has earned a targeted
sweep. Writers never interpret and the reader never guesses: a trip is a
fact about one request, a rate is a claim about a model, and the split
keeps both honest.

Counters are cumulative and process-lifetime (they live on the proxy
supervisor, like the agent cache metrics, so a proxy restart does not
zero them). Windowing is the *reader's* job: the scheduler keeps its own
per-model baselines and rates the delta, so two readers can window
differently without fighting over a reset button.

A restart no longer zeroes the evidence. The *unacted window* per model
is persisted and re-seeded at boot via [`ModelDefectLedger::seed`] —
decayed by wall-clock age, and discarded outright when it was recorded
against a different llama.cpp release.

Decay and build scoping are the two answers to the staleness objection
that originally kept this unpersisted (ADR 0001 via ADR 0005):
yesterday's rate must not answer today's question at full weight, and
another build's rate must not answer it at all. Interpretation still
belongs to readers — this module stores and adds counts; nothing here
judges. The policy that decides what a restored row is worth lives next
door in [`decay`], kept pure and separately testable.

The accepted residual: a window held by a long-lived process does not
decay in place, because decay covers only the recorded gap between runs.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`decay.rs`](decay.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-defects-decay-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-defects-decay-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-defects-decay-coverage.json) |
<!-- module-table:end -->

</details>
