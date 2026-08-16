# ADR 0007 — Ask the server for template capabilities

- **Status:** Accepted
- **Date:** 2026-08-17
- **Depends on:** [ADR 0001](0001-runtime-capability-tiers.md),
  [ADR 0002](0002-defer-tool-call-constraint-to-llama-cpp.md),
  [ADR 0003](0003-defer-sampler-defaults-to-llama-cpp.md),
  [ADR 0004](0004-observe-the-sampling-boundary.md)
- **Amends:** [ADR 0001](0001-runtime-capability-tiers.md) — the verbatim note
  is the appendix of this document
- **Supersedes:** nothing
- **Superseded by:** nothing

## Context

The reasoning-controls arc gives the sampling hierarchy a resolved
`reasoning_effort`. Before that value can mean anything, one question has to be
answerable per model: **does this model's chat template read the variable at
all?** llama.cpp will not answer it at request time — it validates nothing
(finding 1), and a level sent to a template that never branches on it
disappears without a trace. An effort control that silently does nothing on
most models is the shape of defect this repository keeps re-purchasing:
`min_p: 0.0` read as an absence (#739), `presence_penalty: 1.5` resolved and
faithfully sent (#621). The difference here is that no readback can ever catch
it — see Consequences.

Two ways to answer the question existed:

1. **Build a detector in gglib.** `gglib-gguf/src/capabilities/` already has
   the machinery precedent: `template_probe.rs` executes the GGUF's own
   `tokenizer.chat_template` against probe conversations and diffs the renders
   to derive tool-call dialects. Extending it — or adding a sibling — to
   detect `reasoning_effort` reads is a straight line from existing code.
2. **Read llama-server's own self-report.** The pinned build computes, per
   loaded template, a `chat_template_caps` structure (`jinja::caps`,
   `common/jinja/caps.h`) and publishes it on `GET /props`.

This ADR takes the second path, records the evidence that forced several of
its details, and names a classification problem honestly: the observation this
ADR introduces is used to *gate* a behaviour, and [ADR
0001](0001-runtime-capability-tiers.md) has no tier for that. Its Tier C is
"always on, never gated", and it states flatly, under "Capability presence is
not permission to defer", that "this ADR's implementation gates nothing." Rather than quietly stretching a tier until it
covers the case, the appendix amends ADR 0001 in place with a dated note.

## Method

No live measurement was needed for most of this document; the evidence is the
pinned source itself, plus upstream's own executable evidence and one banner
from the installed binary. All llama.cpp citations are into the vendored pin
at `.llama/llama.cpp`, commit `10bf611` — re-verify against that tree, not
against upstream `master`, which has certainly moved.

The template census is two greps over the templates upstream ships:

```
grep -rln 'reasoning_effort'   models/templates/   # six files
grep -rln 'reasoning_strength' models/templates/   # + muse-glimmer
grep -rln 'enable_thinking'    models/templates/   # the overlap check
```

**Scope.** One pinned commit. The census covers the templates in upstream's
own tree — a reasonable proxy for the model families that matter, but not a
census of the GGUF population, whose templates are whatever their authors
embedded. That gap is precisely why detection is per-model at runtime rather
than a table derived from this census.

## Findings

### 1. `reasoning_effort` is a native top-level field, and nothing validates it

On the pinned build, `reasoning_effort` is parsed off the top-level OpenAI
request body (`tools/server/server-common.cpp:1295-1303`). Any non-empty value
except `"none"` is stored into
`chat_template_kwargs["reasoning_effort"]` as a JSON-quoted string and handed
to the template. There is no allowlist of levels and no check that the
template uses the variable: `"banana"` is accepted as readily as `"high"`,
and **a template ignores what it does not branch on**. Whatever governance
exists has to exist on gglib's side.

### 2. The server publishes what its template reads, computed by execution

`GET /props` carries `chat_template_caps` unconditionally
(`tools/server/server-context.cpp:4602`; documented at
`tools/server/README.md:863`). It is the map form of `jinja::caps` — nine
bools, **five of which default `true`** upstream
(`common/jinja/caps.h:11-14,23`), so the field must be read as a report, not
as a conservative baseline.

The bit that matters here, `supports_reasoning_effort`, is not derived from
metadata or from pattern-matching template text. It is computed by
**executing the template with instrumented variable access**
(`common/jinja/caps.cpp:504-529`): the probe binds `reasoning_effort`, runs a
render, and reports whether the variable's access stats show it was read
(`caps.cpp:526` — `stats.used`). Two sharp edges, both load-bearing later:

- `caps_apply_reasoning_effort` binds **both** `reasoning_effort` and
  `reasoning_strength` to the same value (`caps.cpp:29-32`), and the
  production render path applies the identical binding
  (`common/chat.cpp:925`), so the cap and the render agree about the alias.
- When a model ships a separate `tool_use` template, the published caps
  describe *that* template — "the more expressive template when available"
  (`common/chat.cpp:3855-3863`) — which is not necessarily the template this
  request renders.

And the semantics are strictly weaker than they look: `stats.used` means the
template **read** the variable, not that any particular level changes the
output (see finding 3).

### 3. Template census: seven read it, and levels are per-template folklore

On the pinned tree, seven shipped templates set the cap: six read
`reasoning_effort` literally — `openai-gpt-oss-120b`,
`deepseek-ai-DeepSeek-V4`, `deepseek-ai-DeepSeek-V4-Flash-0731`,
`Cohere2MoE`, `tencent-Hy3`, `upstage-Solar-Open-100B` — and `muse-glimmer`
reads the alias `reasoning_strength`, which finding 2's binding makes
equivalent.

**Five of those seven never read `enable_thinking`** (the exceptions are the
two DeepSeek-V4 templates), and the Qwen3 family reads `enable_thinking` but
not `reasoning_effort`. The two mechanisms are disjoint populations, not a
new control layered on an old one.

Levels do not generalise either. Upstream's own tests
(`tests/test-chat.cpp:6662-6690`) show DeepSeek-V4 rendering
"Reasoning Effort: Absolute maximum" for `"max"` and rendering nothing
special for `"high"` or `"low"` — the level vocabulary is per-template, and a
true cap licenses sending a level, not expecting it to do anything.

### 4. `"none"` does not mean off — retraction

~~`reasoning_effort: "none"` disables thinking on any model.~~

> **Retracted 2026-08-17.** An earlier design draft claimed this, generalising
> from the comment in `server-common.cpp` ("`none` disables reasoning"). The
> claim is false, and the mechanics are worth recording because they shaped
> two decisions below.

What `"none"` actually does (`tools/server/server-common.cpp:1296-1304`): it
sets `inputs.enable_thinking = false` and **erases** the `reasoning_effort`
kwarg. But `enable_thinking` is a plain Jinja variable that most templates
never read — finding 3's five-of-seven — so on those models the first half is
inert. And on gpt-oss the erasure is worse than inert: the template's own
fallback `{%- set reasoning_effort = "medium" %}`
(`models/templates/openai-gpt-oss-120b.jinja:203-206`) fills the hole, so
**`"none"` yields medium thinking**.

Upstream knows this is non-universal — it ships
`common_chat_templates_support_enable_thinking()` (`common/chat.cpp:357`)
precisely to answer "does *this* template honour the switch" — and that
predicate is **not** part of `jinja::caps`, so gglib cannot observe it
through `/props`.

Consequence, taken as a decision below: `"none"` is not offered as a level.
"Stop thinking" is expressed as `reasoning_budget_tokens: 0`, which is
sampler-enforced (`common/reasoning-budget.{h,cpp}`) rather than
template-dependent, and observable in slot params
(`tools/server/server-schema.cpp:383`).

### 5. A build-number gate fails closed on the machine in front of it

The obvious in-house shape — a `RuntimeFlags::REASONING_EFFORT` bit behind a
`MIN_BUILD_*` threshold — was checked against the installed binary and dies on
contact. It reports:

```
version: 0.1.0-dev (build 1, commit 10bf611)
```

`parse_build_number` (`crates/gglib-core/src/domain/runtime_capabilities.rs:232-248`)
reads the digits after `version:` and gets **0** from `0.1.0-dev`; `0` is
under `MIN_PLAUSIBLE_BUILD` (`runtime_capabilities.rs:113`), so the runtime
resolves to `unknown()`. `parse_commit` (`runtime_capabilities.rs:257-263`)
takes the first parenthesised run and rejects it —
`build 1, commit 10bf611` is not all hex — so not even the sha survives. An
unidentified runtime has no flags, by design, which means a build-number gate
would be **permanently off on the primary development machine** while the
capability sits right there in the binary.

That is the honest disqualifier, but not the deepest one: a build number
answers "what can this *binary* do", and the question is the intersection —
this binary *and* this model's template. `GET /props` answers the
intersection directly, because the caps are computed from the template the
server actually loaded.

### 6. The trust gate did not cover this field — the #779 shape, renamed

Before this arc, a client-sent `reasoning_effort` was neither modelled by
`InferenceConfig` nor listed in `UNMODELLED_SAMPLER_KEYS`
(`crates/gglib-core/src/request_pipeline/sampling.rs:286-297`), so it rode
through the untrusted path ungoverned — exactly the passthrough PR #779
("close the untrusted-sampler passthrough") shut for `frequency_penalty`,
with a new name. **Modelling the field is what creates governance**: an
unmodelled key is invisible to the trust gate, to provenance, and to every
discard record.

## Decision

**1. Detection is deferred to upstream wholesale.** gglib learns whether a
model's template supports `reasoning_effort` by reading llama-server's own
`GET /props` → `chat_template_caps`. No detector is built in
`gglib-gguf/src/capabilities/`, and `template_probe.rs` is not extended. The
caps are computed by executing the template on the exact Jinja engine that
renders production requests (finding 2); a gglib reimplementation could only
ever *disagree* with the renderer it is trying to predict, and every
disagreement would be a bug on gglib's side by construction.

**2. The observation is snapshotted once per launch and held for the process
lifetime**, per ADR 0001's static-arbitration rule — nothing re-probes
mid-request, nothing changes strategy mid-stream. It is stored on the model's
catalog row as a tri-state: **supported / not supported / never observed**,
never collapsed (see Consequences).

**3. The resolved `reasoning_effort` is suppressed when — and only when — the
observed caps positively say the template does not read the variable.** The
suppression is recorded in sampling provenance, never silent. **Unknown never
gates**: a never-observed model sends the resolved value. This is the same
discipline stated twice already in this tree — `ModelContext` treats an empty
capability set on an unresolved context as "nobody knows", not "the model
can't" (`crates/gglib-core/src/request_pipeline/model_context.rs:55-57`), and
an unidentified `RuntimeCapabilities` means every compensation stays on
(`runtime_capabilities.rs:149`). Unknown ≠ unsupported, and an observation
that fails to arrive must not masquerade as one that arrived negative.

**4. `"none"` is not a level gglib offers.** Finding 4 shows it delivering
medium thinking on gpt-oss and nothing at all on most other templates, and
the predicate that would make it honest is unobservable. "Stop thinking" is
`reasoning_budget_tokens: 0` — sampler-enforced, universal, verifiable.

**5. Both fields become modelled, on opposite sides of the trust gate.**
`reasoning_effort` is **taste**: it joins the sampling hierarchy and is
trust-gated like every other sampler preference. `reasoning_budget_tokens`
is a **budget**: it says what the request *is*, not how it should sample, and
joins `max_tokens` on the client-authoritative side (the "taste, not
function" line `request_pipeline::sampling` already draws). Finding 6 closes
behind them.

## Classification: the control is policy, the observation is a self-report

The *control* — resolving, gating, and suppressing `reasoning_effort` — is
**Tier B policy**, beside `request_pipeline::sampling` where it lives.
llama-server has no catalog, no profiles, and no opinion about clients; it
will never arbitrate whose effort level wins. Nothing new there.

The *observation* fits no tier, and saying so out loud is cheaper than
stretching one:

- Not **Tier C**: Tier C "measures whether Tier A is still needed. Always on,
  never gated" — and never *gating*. This observation is a policy input that
  suppresses a resolved value. Filing it under Tier C would falsify ADR
  0001's "this ADR's implementation gates nothing" by reclassification.
- Not **Tier A**: there is no compensation and no deletion criterion. Nothing
  here is waiting for upstream to catch up; upstream is the source.
- Not a **`RuntimeFlags` capability**: those are derived from a build number
  and describe the binary. This is a property of the binary–model pair,
  finding 5's intersection.

It is a fourth posture, named here: a **runtime self-report used as a policy
input**. The server states a fact about itself; gglib records the statement
and lets it steer one decision. The posture carries three rules, all
inherited rather than invented:

1. **Unknown never gates** (decision 3 — the `catalog_resolved` and
   `unknown()` discipline).
2. **Snapshotted once per launch, static for the process lifetime** (ADR
   0001's arbitration rule; a mid-stream cap flip is the unreproducible bug
   class that rule exists to exclude).
3. **Provenance whenever it changes an outcome** — a suppressed value is
   reported suppressed, with the observation that suppressed it.

The appendix amends ADR 0001 so the next reader of "this ADR's implementation
gates nothing" finds the exception where the rule is stated, not three
documents later.

## Consequences

**Good:**

- The effort control is governed: modelled, trust-gated, provenance-tracked.
  The #779 passthrough shape is closed for this field (finding 6).
- Detection cannot drift from the renderer. The answer comes from the code
  that renders the prompt, on the binary and model actually running — the
  intersection no build-number gate can see (finding 5).
- gglib carries no template-parsing code for this, so there is nothing to
  keep in sync as upstream's Jinja dialect grows.
- "Stop thinking" rests on a mechanism that is sampler-enforced and
  observable rather than a template variable most templates ignore
  (finding 4).

**Stated plainly, because they are permanent:**

- **The effort control is permanently unobservable at the sampling
  boundary.** It becomes a template kwarg, consumed at render time; it is not
  a sampler parameter, and no slot-params field echoes it
  (`server-schema.cpp` defines none). ADR 0004's readback can verify that
  `top_k` arrived; it can never verify that an effort level did anything.
  This is not a gap an instrument will close — the provenance record is the
  only account of the decision, and prompt-level effects are measurable only
  by outcome evaluation, the instrument [ADR
  0003](0003-defer-sampler-defaults-to-llama-cpp.md)'s closing rule already
  notes does not exist yet.
- **`reasoning_budget_tokens` is the universal, verifiable counterpart**, and
  the pairing is deliberate: budget (client-authoritative, joining
  `max_tokens`, slot-observable at `server-schema.cpp:383`) beside taste
  (trust-gated, template-dependent, unobservable). When only one of the two
  can be trusted to work everywhere, the API's shape should say which.
- **The caps live on the model row as a tri-state — supported / not
  supported / never observed — and the states are never collapsed.**
  Collapsing "never observed" into "not supported" is exactly how unknown
  starts to gate, one refactor after everyone stops looking.

**Costs, accepted:**

- `supports_reasoning_effort` means *read*, not *honoured* (finding 2's
  `stats.used`; finding 3's DeepSeek-V4 honouring only `"max"`). A true cap
  licenses sending the value; whether a given level changes anything remains
  per-template and unmeasured here.
- The published caps may describe the `tool_use` template rather than the one
  a non-tool request renders (`common/chat.cpp:3855-3863`). Accepted as
  upstream's definition of the fact being observed; a divergence between the
  two templates on this bit is possible and would be invisible.
- The `reasoning_strength` alias means the cap asserts "one of two names is
  read" (`caps.cpp:29-32`). Harmless on the pinned build, where the render
  path binds both names identically (`chat.cpp:925`), but it is one more
  place the observation is coarser than it looks.
- A `jinja::caps` bug upstream becomes a wrong gate in gglib. Accepted: the
  alternative is a second implementation whose disagreements with the
  renderer are all bugs anyway, and the pin (ADR 0001) makes the dependency a
  fixed, testable target rather than a moving one.
- The snapshot is per launch. A template never changes under a running
  server, so the staleness window is theoretical — but it is the same
  reproducibility-over-adaptivity trade ADR 0001 already priced, and it is
  restated here rather than re-litigated.

## Appendix — Amendment to ADR 0001

Apply verbatim to `docs/adr/0001-runtime-capability-tiers.md`.

**1.** In the header block, extend the date line:

```
- **Date:** 2026-08-08 (amended 2026-08-17 — see the note under "Capability
  presence is not permission to defer")
```

**2.** At the end of the section *"Capability presence is not permission to
defer"*, after the paragraph ending "Deferral decisions are separate,
evidence-backed, and reversible.", insert:

> **Amended 2026-08-17.** "This ADR's implementation gates nothing" stands,
> and Tier C remains never-gating. But
> [ADR 0007](0007-ask-the-server-for-template-capabilities.md) introduces an
> observation that *is* a gate: llama-server's `chat_template_caps`
> self-report, read from `GET /props` and used to suppress a resolved
> `reasoning_effort` that the observed template never reads. That observation
> fits no tier here — not Tier C (Tier C never gates), not a `RuntimeFlags`
> capability (it describes the binary–model pair, not the build), not Tier A
> (there is nothing to delete). ADR 0007 names the posture — a **runtime
> self-report used as a policy input** — and carries its rules: snapshotted
> once per launch, provenance whenever it changes an outcome, and unknown
> never gates.
