# loop_detection

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-loop_detection-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-loop_detection-complexity.json)

<!-- module-docs:start -->

Tool-call loop detection via FNV-1a batch signatures.

# Algorithm

1. Compute an **individual signature** for each [`ToolCall`] as
   `"{name}:{fnv1a_64(canonical_args_json):016x}"`.
2. Sort the individual signatures and join them with `"|"` to form a
   **batch signature** that is independent of tool-call ordering.
3. A [`LoopDetector`] counts how many times each batch signature has been
   seen.  The threshold applied depends on whether the batch is classified
   as "observation-only" (see below).

# Dual-threshold detection

Observation-only tools (e.g. browser snapshots, page screenshots) take no
meaningful arguments, so every call hashes to the same signature regardless
of the page content returned.  With a strict threshold this causes false
positives on legitimate `ReAct` *observe → act → observe* cycles.

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
| [`tests.rs`](tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-loop_detection-tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-loop_detection-tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-loop_detection-tests-coverage.json) |
<!-- module-table:end -->

</details>
