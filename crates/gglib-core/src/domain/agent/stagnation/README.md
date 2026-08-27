# stagnation

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-agent-stagnation-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-agent-stagnation-complexity.json)

<!-- module-docs:start -->

Text stagnation detection for the agentic loop.

# Algorithm

After each LLM response the assistant's text is hashed with
[`crate::domain::agent::fnv1a::fnv1a_64`] and pushed onto a sliding window of
recent turns.  When a hash occurs more than `max_stagnation_steps` times
**within that window**, the loop is aborted with
[`AgentError::StagnationDetected`].

The window is `max_stagnation_steps × 4` turns — derived rather than configured,
because the two numbers are not independent.  See `WINDOW_FACTOR`.

## A turn that called a tool is not recorded

Stagnation is about a model that is saying the same thing and *getting
nowhere*.  A turn that issued a tool-call batch is getting somewhere by
definition, and
[`crate::domain::agent::loop_detection::LoopDetector`] is what judges whether
that work is going in circles.

This is load bearing, and it is what the detector used to get wrong.  Assistant
text is read even from turns that also carry `tool_calls`, so a model's
*narration* counted.  Small models narrate almost every tool call, and they
narrate repetitively — "Let me look at the file." before each of six `read_file`
calls, on six different files, was six occurrences of one text.  At the default
of 5 that was a refusal in the middle of ordinary work, and the text carried no
information about whether the work was stuck: the same sentence precedes a model
reading six files and a model reading one file six times.  Only the tool calls
distinguish those, and the other detector is already reading them.

## Why a window, and not a session-wide tally

Counts used to accumulate for the life of the session and never decay.  The
proxy replays the whole conversation through a fresh detector on every request,
so the verdict is a pure function of the transcript — which meant that once a
text had occurred often enough *anywhere* in the history, every subsequent
request was refused too.  History only grows, so nothing the user or the model
could do would clear it.  A conversation that crossed the threshold was over.

A window keeps the property worth having — repeats close together are evidence
of being stuck — and drops the one that killed sessions, that a repeat *ever*
is permanent.

## Oscillation detection

Because the window spans many turns rather than only the previous one, the
detector still catches A → B → A → B oscillation in the text as well as strictly
consecutive repetition.  A model that alternates between two responses exhausts
its budget for each independently; with the default `max_stagnation_steps = 5`,
stagnation fires within at most 12 iterations (two responses × 6 occurrences
each), comfortably inside the 20-turn window that default implies.  This is the
same guarantee the session-wide tally gave, and `WINDOW_FACTOR` is sized to
preserve it at every threshold.

What is **not** caught is oscillation between tool-call *batches*.  That was
never this detector's job — a tool-call-only turn has no text — but it used to
be caught by accident whenever the model happened to narrate its cycle
repetitively, and excluding tool-calling turns ends that.  The accident is not
worth keeping: it fired on exactly the narration described above, so it rejected
far more ordinary work than it caught cycles.  The loop detector gave cycles up
deliberately, and nothing backstops them now.  See ADR 0011.

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
