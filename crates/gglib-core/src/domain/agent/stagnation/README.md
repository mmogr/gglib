# stagnation

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-agent-stagnation-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-agent-stagnation-complexity.json)

<!-- module-docs:start -->

Text stagnation detection for the agentic loop.

# Algorithm

After each LLM response the assistant's text is hashed with
[`crate::domain::agent::fnv1a::fnv1a_64`].  The hash is looked up in a session-wide
occurrence map.  When a hash has already been seen at least
`max_stagnation_steps` times **before** the current call, the loop is
aborted with [`AgentError::StagnationDetected`].

Stagnation detection runs on **every** iteration — both tool-calling and
text-only — so it catches models that repeat the same text regardless of
whether tools are invoked.  Tool-call loops are handled separately by
[`crate::domain::agent::loop_detection::LoopDetector`], **and the two count
differently on purpose**: that detector counts only back-to-back repeats of a
batch, because a session-wide tally rejected ordinary work — an agent that runs
a command, edits, and runs it again.  This detector stays session-wide.

That does **not** make it a backstop for what the loop detector gave up.  The
oscillation this detector catches (below) is oscillation *in the text*, and a
tool-call-only turn has none — empty text is ignored, as stated below.  A model
cycling through tool batches while narrating nothing is caught by neither
guard — at any cycle period of two or more, not only strict alternation — and
is visible only as `identical_result_repeats` in the proxy's ledger.

## Oscillation detection

Because occurrence counts are accumulated across the **whole session**, the
detector catches A → B → A → B oscillations as well as strictly consecutive
repetitions.  A model that alternates between two responses will exhaust its
budget for each response independently; with the default `max_stagnation_steps
= 5`, stagnation fires within at most 12 iterations (two responses × 6
occurrences each).

The first occurrence of any hash is always treated as a baseline and never
triggers an error.  Empty text is silently ignored so that tool-call-only
iterations do not accumulate spurious counts.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`tests.rs`](tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-stagnation-tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-stagnation-tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-stagnation-tests-coverage.json) |
<!-- module-table:end -->

</details>
