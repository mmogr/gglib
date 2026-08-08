# ADR 0001 — Compensation, Policy, Observation: classifying gglib against llama.cpp

- **Status:** Accepted
- **Date:** 2026-08-08
- **Supersedes:** nothing
- **Superseded by:** nothing

## Context

gglib sits between an OpenAI-compatible client and llama-server. Almost every
behaviour it applies exists for one of two very different reasons, and until
now nothing in the codebase distinguished them:

1. **llama.cpp cannot do this, or does it wrong.** The Qwen XML tool-call
   parser, non-streaming dialect normalization, decode-time grammar
   origination. These were written against a specific upstream limitation.
2. **llama.cpp will never do this.** The sampling hierarchy, admission control
   and model swapping, the model catalog, downloads, cross-turn loop
   detection. llama-server is one process serving one model with no catalog,
   no opinion about clients, and no state between requests. These are not
   gaps upstream is going to close; they are a different job.

The two look identical at a call site. Both are "gglib does something to the
request." The difference only shows up over time: category 1 has an expiry
date and category 2 does not.

Three things made that difference invisible:

- **Installs floated.** `download_prebuilt_binaries` resolved
  `releases/latest`, so the engine underneath gglib changed whenever upstream
  cut a release. Two users installing a day apart ran different engines, and
  a behaviour change could not be attributed to gglib or to llama.cpp.
- **The runtime was never modelled.** gglib had rich detection for what a
  *model* can do (`GgufCapabilities`, `format:*` tags, launch flags) and none
  for what the *binary* can do. `validate.rs` read `llama-server --version`
  and printed it to the console.
- **Compensation was unconditional.** With no way to ask *is this still
  needed?*, every workaround was hardcoded on. Upstream could ship the fix and
  nobody would find out.

This is not hypothetical. llama.cpp's `peg-native` chat parser now handles
delimited ("constructed") tool-call dialects — the XML-style envelopes
`DelimitedToolCallParser` was written for — and `json_schema_to_grammar` builds
argument grammars from tool schemas. Meanwhile upstream issues [#24807] (a
duplicate `</parameter>` dropping an entire tool call, ~1 in 128 requests) and
[#20260] (a thinking model emitting prose before `<tool_call>`) describe
failure modes gglib fixed in #690, a year earlier. Convergence is real, and so
is the fact that gglib is currently ahead on the specifics.

Without a classification, that situation resolves itself badly in both
directions: either gglib carries dead compensation forever, or it deletes
compensation that is still load-bearing.

## Decision

**Every gglib behaviour is classified into one of three tiers, and the tier
determines its lifecycle.**

### Tier A — Compensation

Exists only because llama.cpp does not do it, or does it wrong. **Designed to
be deleted.**

Rules:

- Gates on `RuntimeCapabilities`. A Tier A behaviour must be *skippable* when
  the runtime handles it natively, even if the decision today is "don't skip".
- Its module docs name a **deletion criterion**: the observable condition under
  which this code should be removed.
- Where gglib is ahead of upstream, the fix is contributed upstream. That is
  how Tier A shrinks — an accepted upstream patch converts maintenance burden
  into deletion.

Current members: `normalize::parsers::delimited`, `normalize::oneshot`,
`normalize` reasoning-tag handling (`format:think-tag`),
`request_pipeline::constrain`.

### Tier B — Policy

llama.cpp will never do this, structurally. **Permanently gglib's.**

Rules:

- Never gates on `RuntimeCapabilities`. A capability probe is irrelevant to a
  decision upstream is not in a position to make.
- Does not need a deletion criterion.

Current members: `request_pipeline::sampling` and the whole inference
hierarchy (`domain::inference`, `inference_profile`,
`sampling_provenance`); `process::admission` and `process::residency`; the
model catalog and its tag/capability detection; `gglib-download`;
`request_pipeline::tools` (a catalog-driven decision — llama-server has no
catalog); the cross-turn loop and stagnation detectors in `domain::agent`;
`token_calibration`; KV cache tiering configuration; the MCP gateway; the
`access` module's Host guard and bearer auth; the proxy dashboard.

`request_pipeline::truncation` is Tier B with a caveat: the *policy* — which
messages are eligible, what the budget is, whether to reject or compact — is
gglib's and stays. The *mechanism* may eventually be served by upstream
context-shift or compaction, at which point the mechanism (not the policy)
becomes a Tier A question.

### Tier C — Observation

Measures whether Tier A is still needed. **Always on, never gated.**

Current members: `normalize::residue` (the dialect drift alarm),
`metrics::ContextMetricsStore`, the dashboard's `dialect_residue_total` and
`grammar_enforced` counters.

Tier C is what makes the other two tiers honest. Without it, "is this
compensation still needed?" is answered by argument. With it, the answer is
evidence.

## Two rules that follow

### Capability presence is not permission to defer

`RuntimeFlags::PEG_NATIVE_TOOL_CALLS` answers *"does the runtime attempt this
itself?"*. It does not answer *"should gglib stop attempting it?"*. The second
question is settled by measurement — a tune run, an A/B eval, a week of Tier C
counters — and recorded as its own ADR.

Consequently, **this ADR's implementation gates nothing.** The probe is taken,
recorded, and surfaced; no Tier A behaviour changes. Deferral decisions are
separate, evidence-backed, and reversible.

### Arbitration is static, resolved once per run

The capability probe is taken when a server is launched and held for that
process's lifetime. Nothing re-probes mid-request. Nothing switches parsing or
constraint strategy mid-stream.

This is deliberate and it is the one place we consciously give up a smarter
design. A dynamic system could notice a residue hit and fall back to gglib's
parser for the rest of the response — but then a failure depends on *when*
within a stream the evidence arrived, and two identical requests produce
different behaviour. That is precisely the class of bug that cannot be
reproduced from a recording, and it would undermine the request recorder that
the rest of this architecture is built to feed.

Tier C's job is to **log** divergence between what a runtime claimed and what
it delivered. Acting on that log means changing a threshold in
`domain::runtime_capabilities` — deliberately, between runs, with the evidence
in hand.

## Consequences

**Good:**

- A behaviour's lifecycle is stated where the behaviour lives. "Can we delete
  this yet?" becomes a question with a documented answer.
- Upstream upgrades become deliberate, reviewable events: bump
  `PINNED_LLAMA_RELEASE`, run the suite, ship the bump with observed
  differences in the commit message.
- Behaviour changes become attributable. A stored request record carrying the
  llama.cpp build number distinguishes a gglib regression from an upstream one.
- Contributing upstream becomes strategically legible rather than altruistic:
  it is the mechanism by which Tier A shrinks.

**Costs, accepted:**

- The pin means gglib users do not automatically get upstream fixes. Mitigated
  by `GGLIB_LLAMA_RELEASE`, which takes a tag or `latest`.
- Someone must periodically bump the pin. That work is real, and it is the
  point: it is the work that was previously happening invisibly and without
  testing.
- Static arbitration leaves value on the table — a run against a flaky build
  compensates for the whole run rather than adapting. Accepted in exchange for
  reproducibility.
- The tier classification will be wrong at the edges (`truncation` is already
  a documented example). Boundary cases get a note rather than a forced
  answer.

## Implementation

- `domain::runtime_capabilities` — `RuntimeCapabilities`, `RuntimeFlags`, and
  the `MIN_BUILD_*` thresholds, each citing the upstream release or issue that
  establishes it. An unparseable version yields `unknown()`: no flags, every
  compensation on. Unknown means *nobody knows*, never *the feature is absent*
  — the same discipline `ModelContext::catalog_resolved` applies to models.
- `llama::runtime_probe` — runs `llama-server --version`, memoized per binary
  path. Never fails; an unprobeable binary is an unidentified one.
- `llama::download::PINNED_LLAMA_RELEASE` — the pinned build, overridable via
  `GGLIB_LLAMA_RELEASE`. Initialised to the release that was `latest` when the
  pin landed, so introducing it changed nothing on day one.
- Surfaced by the launch banner (`runtime` decision) and
  `gglib config llama status`.

[#24807]: https://github.com/ggml-org/llama.cpp/issues/24807
[#20260]: https://github.com/ggml-org/llama.cpp/issues/20260
