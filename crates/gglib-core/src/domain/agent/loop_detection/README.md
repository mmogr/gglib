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
3. Compute a **results hash** for the batch by pairing each call with its own
   answer, sorting the pairs and hashing them (see `results.rs`).
4. A [`LoopDetector`] counts how many times the **current** batch signature has
   repeated back to back **and got the same answer back**.  A batch with a
   different signature resets that run to one; so does an answer that differs
   from the previous occurrence's.  The threshold applied depends on whether
   the batch is classified as "observation-only" (see below).

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

**What breaks a run.** A batch with a different signature, or a user turn.
The detector is never called for a turn that produced no tool calls, so a prose
answer and a `role: "tool"` result pass without breaking a run.  That is
deliberate rather than incidental: every real tool call is answered by a result
message before the next one, so a run that either could break would never reach
two.

A user turn is different, and breaks a run explicitly through `break_run`.  On
the agent path that is structural — `AgentLoop::run` is invoked once per user
message and builds a fresh `Guards` — so only the proxy, which walks one
detector across a whole replayed conversation, has to be told.  Without it the
two paths refuse at different turns while claiming parity by construction, and
a person asking three times for the same failing build was refused on the
third.  It resets the read-only allowance along with the strike count, because
that is what a fresh `Guards` does.

**The accepted cost.** A *cycle* of tool batches is no longer caught, at any
period of two or more.  Not only strict alternation: `A, A, B` repeating also
escapes, because the run reaches the threshold on every pass and never crosses
it.  Separating a cycle from the scattered repeats above needs a window or a
decay rate, and there is no measurement behind either number.

Reading the answers did not close this, and it is worth saying so plainly
because it looks as though it should have.  The run breaks on **signature**,
before any answer is consulted, so a cycle escapes whatever came back.

Nothing backstops it in the general case.
[`crate::domain::agent::stagnation::StagnationDetector`] keeps its session-wide
counting and catches an oscillating session **only if the model also repeats its
prose** — a tool-call-only turn carries `content: null`, which that detector
ignores by design.  A model alternating two batches and narrating nothing is
therefore refused by neither guard.  What observes it is
`identical_result_repeats` in the proxy's ledger: a reading for a person, not a
verdict.

# The verdict reads what came back

A repeat is a strike only when its answers matched the previous occurrence's.
The same call with a different answer is progress that happens to look alike —
an agent polling a build for output issues an identical batch every time, and
run-length counting alone refused it exactly as a session-wide tally did.

The two call sites cannot ask this the same way at the same moment, which is why
[`LoopDetector::check`] and [`LoopDetector::record_results`] are separate calls.
The proxy reads a finished transcript and knows a batch's answers when it
counts it; the agent loop counts a batch *before* it runs, so its answers do not
exist yet.  Both call `check` where the verdict is needed and `record_results`
once the answers are in hand.  A [`BatchRecord`] links the two, so the answers
recorded cannot belong to a different batch than the one counted — getting that
backwards would invert the measurement silently.

**Unknown answers never rescue.**  A batch that went unanswered, or was
answered only in part, produces no hash, and no hash is not evidence of
progress.  That is also what makes a detector nobody records into behave exactly
as it did before it could read anything.

**The ceiling.**  A run carries two counts: occurrences since the answer last
changed, which the threshold is compared against, and occurrences in the run
overall, which a changed answer does *not* reset.  A changed answer restarts
the first, so on its own it would exempt any tool whose output carries a clock,
an elapsed time or a progress counter — `cargo test`'s `finished in 0.31s` is
enough.  Observation batches are exempt
anyway, on the same ground the tier already stands on — but only while the tier
is *configured*: the classification comes from `observation_tools` and the
exemption from `max_observation_steps`, so with the latter unset a read-only
batch is bounded like any other.  The exemption is also withheld from the
entries that change nothing *on this machine* and still cost something
elsewhere — `navigate` moves a browser session, `click` changes page state,
`fetch_webpage` spends someone else's rate limit.  They stay classified, so
they keep the elevated threshold; they simply do not get the ceiling waived.
See `is_costly_batch`.  Everything else
may be carried by changing answers only while the run itself stays inside the
read-only allowance, `max_observation_steps`.  That number is reused rather than
invented; there is no measurement behind a new one.

**What it still does not fix.**  A *quiet* poll.  Sixteen identical answers in a
row to a read-only batch is a loop by this detector's definition, so an agent
watching a compile that prints nothing for two minutes is still refused at the
observation ceiling.  Reading the answers helps only once the output moves.

See [ADR 0010].

[ADR 0010]: https://github.com/mmogr/gglib/blob/main/docs/adr/0010-the-loop-guard-reads-what-came-back.md

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
| [`observation.rs`](observation.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-observation-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-observation-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-observation-coverage.json) |
| [`results.rs`](results.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-results-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-results-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-results-coverage.json) |
| [`signature.rs`](signature.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-signature-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-signature-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-signature-coverage.json) |
| [`tests.rs`](tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-tests-coverage.json) |
| [`verdict_tests.rs`](verdict_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-verdict_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-verdict_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loop_detection-verdict_tests-coverage.json) |
<!-- module-table:end -->

</details>
