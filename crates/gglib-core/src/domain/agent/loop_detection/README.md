# Loop Detection

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-agent-loop_detection-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-domain-agent-loop_detection-complexity.json)

<!-- module-docs:start -->

Tool-call loop detection via FNV-1a batch signatures.

# Algorithm

1. Compute an **individual signature** for each [`ToolCall`] as
   `"{name}:{fnv1a_64(canonical_args_json):016x}"`.
2. Sort the individual signatures and join them with `"|"` to form a
   **batch signature** that is independent of tool-call ordering.
3. A [`LoopDetector`] counts how many times the **current** batch signature
   has repeated back to back.  A batch with a different signature resets that
   run to one.  The threshold applied depends on whether the batch is
   classified as "observation-only" (see below).

# Consecutive, not session-wide

Only the current unbroken run is held.  A session-wide tally made any long
conversation terminal: an agentic client replays the whole history every turn,
so once a signature had accumulated enough occurrences *anywhere* in the
session, every later request was rejected too, and the error body told a
non-technical user to run a CLI flag.

The case that forced the change is ordinary work rather than a loop.  Mutating
tools are held to `max_repeated_batch_steps`, which is 2, so an agent that runs
one command, edits a file, runs it again, edits again and runs it a third
time reaches three identical batches well inside a normal task — and was
refused on that third run.

**What breaks a run.** Only a batch with a different signature.  The detector
is never called for a turn that produced no tool calls, so a prose answer, a
`role: "tool"` result and a user interjection all pass without breaking a run.
That is deliberate rather than incidental: every real tool call is answered by
a result message before the next one, so a run that anything else could break
would never reach two.

**The accepted cost.** A *cycle* of tool batches is no longer caught, at any
period of two or more.  Not only strict alternation: `A, A, B` repeating also
escapes, because the run reaches the threshold on every pass and never crosses
it.  Separating a cycle from the scattered repeats above needs a window or a
decay rate, and there is no measurement behind either number.

Nothing backstops it in the general case.
[`crate::domain::agent::stagnation::StagnationDetector`] keeps its session-wide
counting and catches an oscillating session **only if the model also repeats its
prose** — a tool-call-only turn carries `content: null`, which that detector
ignores by design.  A model alternating two batches and narrating nothing is
therefore refused by neither guard.  What observes it is
`identical_result_repeats` in the proxy's ledger: a reading for a person, not a
verdict.

What this does **not** fix is a batch repeated back to back whose result
changes every time — an agent polling a build for output.  Those repeats are
consecutive, so run-length counting refuses them exactly as a session-wide
tally did.  Telling them apart needs the verdict to read what came back, which
it cannot do today; see ADR 0006's postscript and the `identical_result_repeats`
counter, which measures how often it happens.

# Dual-threshold detection

Observation-only tools are the read-only half of an agent's toolkit: browser
snapshots and page screenshots, and the file reads, directory listings and
searches a coding agent runs.  Repeating one is ordinary work, not a loop, and
a strict threshold causes false positives on legitimate `ReAct`
*observe → act → observe* cycles.

Two distinct reasons put a tool in this tier, and both matter:

1. **The signature cannot tell the calls apart.**  A browser snapshot takes no
   meaningful arguments, so every call hashes identically regardless of the
   page content returned.  The repeat is an artifact of the signature scheme.
2. **The repeat is real but benign.**  `read_file{"path":"a.rs"}` hashes
   distinctly and genuinely did happen twice — the agent read a file, edited
   it, and read it back to check.  Nothing is stuck; verifying your own work
   is the correct behaviour, and rejecting it ends the conversation.

The second reason is why membership is not limited to argument-free tools.
What the tier actually selects for is **read-only** — a call that changes
nothing can be repeated without consequence, so the cost of tolerating it is a
few wasted generations, while the cost of refusing it is the session.

The detector therefore applies **two thresholds**:

| Batch type | Threshold used |
|------------|---------------|
| Every call matches an observation pattern | `max_observation_steps` |
| At least one call does **not** match | `max_repeated_batch_steps` |

A batch is observation-only when [`is_observation_batch`] returns `true`:
every call's lowercased name satisfies
`name.ends_with(pattern) || name.contains(pattern)` for at least one
pattern in the configured list.  Substring/suffix matching is used
intentionally so that namespaced MCP tool names such as
`playwright_mcp_browser_snapshot` are matched by the short pattern
`"snapshot"` without requiring users to enumerate every vendor variant.

**Mixed batches** (≥ 1 non-observation call) always fall back to the
stricter `max_repeated_batch_steps` — the conservative choice.

# Hash algorithm

FNV-1a 64-bit with:
- Offset basis: `14_695_981_039_346_656_037`
- Prime: `1_099_511_628_211`
- Wrapping 64-bit multiplication (`wrapping_mul`)

Argument JSON objects are **canonicalised** (keys sorted recursively)
before hashing so that `{"a":1,"b":2}` and `{"b":2,"a":1}` produce the
same signature, preventing a non-deterministically ordered model from
bypassing the loop guard.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`results.rs`](results.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-results-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-results-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-results-coverage.json) |
| [`tests.rs`](tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-tests-coverage.json) |
<!-- module-table:end -->

</details>
