# ADR 0003 — Defer sampler defaults to llama.cpp

- **Status:** Accepted
- **Date:** 2026-08-09
- **Depends on:** [ADR 0001](0001-runtime-capability-tiers.md)
- **Supersedes:** nothing
- **Superseded by:** nothing

## Context

[ADR 0001](0001-runtime-capability-tiers.md) lists `request_pipeline::sampling`
and the whole inference hierarchy as **Tier B — Policy**: "llama.cpp will never
do this, structurally. Permanently gglib's." Tier B never gates on
`RuntimeCapabilities` and needs no deletion criterion.

That classification is right about the *ladder*. llama-server is one process
serving one model with no catalog, no profiles, and no opinion about clients,
so it structurally cannot arbitrate between a `:coding` profile and a
per-model default. `resolve_layers`, `inference_profile`,
`sampling_provenance`, `trust_client_sampling` and the auto-detected-versus-
user-set distinction are all permanently gglib's, and nothing here changes
that.

It is not obviously right about the **floor beneath** the ladder.
`with_hardcoded_defaults()` pins seven sampler values, and `resolve_sampling`
force-writes all seven into every request — `to_openai_json_patch` drops only
`None`, so a `Some` in the floor is an assertion on the wire, not a fallback.
A floor value that happens to equal the runtime's own default is not policy. It
is a redundant assertion with an expiry date that nobody is watching.

**This is not hypothetical.** #739 found exactly that failure: `min_p: 0.0`
read like an absence and was in fact *disabling* min-p on every forwarded
request, because llama.cpp's own default is 0.05. The fix restated upstream's
value rather than deferring to it, explicitly so the value "stays visible as
`min_p=floor` in sampling provenance". So the redundancy was not removed, it
was made harmless-by-agreement — and the other six floor values were never
checked for the same shape.

Four of them (`top_p`, `top_k`, `repeat_penalty`, `presence_penalty`) carried
no documentation at all about their relationship to upstream. They sat in
precisely the configuration `min_p: 0.0` occupied before #739.

Two further facts make the question urgent rather than academic:

- **The same values are also emitted as launch flags.** `to_cli_args`
  (`inference.rs:709`) writes `--temp`, `--top-p`, `--top-k`, `--min-p` and the
  rest onto llama-server's command line via `command.rs:290-294`, fully
  resolved including the floor. `llama/args/` has a dedicated resolver for
  every other launch concern — `jinja.rs`, `reasoning.rs`, `kv_cache_type.rs`,
  `mtp.rs`, `cache_ram.rs`, `slot_restore.rs`, `embedding.rs` — and none for
  sampling; `launch_narration.rs` emits a `LaunchDecision` for every concern
  and none for sampling. So the second write is neither resolved through the
  established pattern nor stated at startup.

- **Nothing verifies any of it.** gglib never reads back what llama-server
  applied. Every defect in this subsystem — #621, #745, #743 — was found by a
  human hand-probing `GET /slots`. ADR 0001 says Tier C observation "is what
  makes the other two tiers honest. Without it, 'is this compensation still
  needed?' is answered by argument." Sampling has no Tier C and has been
  answered by argument for its entire life.

ADR 0001's own `truncation` caveat is the template for what follows: the
*policy* is gglib's and stays; the *mechanism* may be served by upstream, at
which point the mechanism becomes a Tier A question. Here the policy is the
ladder and the mechanism is the floor's wire assertion.

## Method

`scripts/experiments/sampler_wire_semantics.py`, run against **raw
llama-server with gglib out of the request path** — gglib's own force-write
would otherwise be the thing being measured.

| | |
|---|---|
| Build | `69bf643`, reported as `b1-69bf643` — the pin |
| Models | `Qwen3.5-4B-UD-Q6_K_XL`, `Llama-3.2-3B-Instruct-UD-Q6_K_XL` |
| Hardware | AMD 840M, Linux x86_64 |
| Launch | `--jinja -c 8192 -ngl 99`, **and no sampler flags** |
| Instrument | `GET /props` → `default_generation_settings.params` |

The floor is read out of `inference.rs` at runtime rather than copied into the
harness, following `tests/ts/contracts/settingsBounds.test.ts`, so the
comparison cannot silently go stale against a floor that has since moved.

**The classification rule was fixed before the data was seen:**

> A floor value equal to the pinned runtime's own default is **Tier A
> compensation** — it exists against the possibility that llama.cpp does the
> wrong thing, and llama.cpp does the right thing. It is deleted and gglib
> defers. A floor value that **diverges** is **Tier B policy**: it stays,
> force-written, and its documentation must state the upstream value it
> diverges from.

Stating it in advance is what stops the answer being chosen after seeing the
numbers. It cost nothing to write down and it is the only reason finding 1 can
be read as a result rather than as a rationalisation.

### Controls

A config read is not a generation, so no sampling parameter can influence it —
but that is an argument, and the point of this document is to prefer
measurements to arguments. Four controls, three of which could have overturned
the finding:

| Control | Result |
|---|---|
| Does `/props` drift after requests with wild params (temp 0.05→1.9, `top_k` 3, `min_p` 0.44)? | No drift |
| Does a bare launch (`-m` and `--port` only) differ from `--jinja -c -ngl`? | Identical |
| Does the table vary between the two models? | Identical, all 11 fields and the chain |
| **Positive control:** does anything move the table? | **Yes — `--temp 0.7` moves 0.8 → 0.7** |

The last row is load-bearing. Without it, "no difference" is indistinguishable
from a dead instrument, which is the failure ADR 0002's finding 3 records under
its own name.

## Findings

### 1. Six of the seven force-written values restate an upstream default

```
  parameter          gglib floor   upstream   verdict
  temperature                0.7        0.8   DIVERGES -> policy
  top_p                     0.95       0.95   EQUALS   -> compensation
  top_k                       40         40   EQUALS   -> compensation
  repeat_penalty             1.0        1.0   EQUALS   -> compensation
  presence_penalty           0.0        0.0   EQUALS   -> compensation
  min_p                     0.05       0.05   EQUALS   -> compensation
  dry_multiplier             0.0        0.0   EQUALS   -> compensation
```

Only `temperature` is a decision gglib is actually making. The other six are
gglib typing out an answer that was already the answer.

The four values that carried no documentation — `top_p`, `top_k`,
`repeat_penalty`, `presence_penalty` — are all in the redundant set. #739's
`min_p: 0.05` was a correct hand-copy of upstream's value, and
`dry_multiplier: 0.0` was already annotated as such.

### 2. This is a property of the build, not of a model or of the harness

Identical across both models, across all eleven parameters and the sampler
chain, and identical on a bare launch. So a single measurement per build is
sufficient, and the result does not need re-taking per GGUF.

This is a **stronger** claim than ADR 0002 was able to make, and the contrast
is worth recording. There, finding 1 was taken on one model and finding 4
overturned it on a second: tool-call conformance is model behaviour and did not
generalise. Sampler defaults are a server-level table and do. Different kind of
fact, different reach — and the reason to say so is that ADR 0002's caveat
about scope "was written as a formality and turned out to be the load-bearing
sentence in the document."

### 3. The launch flags are inert twice over

- Six of the seven set a value to what it already was, so they are invisible in
  `/props`. Only `--temp 0.7` moved anything.
- The remaining one loses anyway: launched with `--temp 0.7` and sent a body
  with `temperature: 1.5`, the slot reports **1.5**. The body wins, and gglib
  force-writes a body value on every request.

So the launch flags have no effect on any request that goes through the
pipeline. They affect exactly one population: someone bypassing gglib and
curling llama-server directly.

### 4. `dry_penalty_last_n` defaults to 64, and the tree says otherwise in three places

`docs/sampling.md:40` says 64. `docs/sampling.md:315` says −1. The same
document contradicts itself, and the −1 claim also reached
`src/constants/inferenceDefaults.ts:113-114`. The measured default is **64**.

The consequence is worse than a stale comment: the 64 figure fed
`domain/benchmark/tune/config.rs:76`, where it shapes a benchmark grid. A
number nobody had verified was steering measurement. (It happened to be the
correct one, which is luck, not process.)

### 5. The sampler chain order, previously unstated anywhere

```
penalties -> dry -> top_n_sigma -> top_k -> typ_p -> top_p -> min_p -> xtc -> temperature
```

gglib sends four truncation samplers on every request and never sets
`--samplers`, so the order in which they compose is load-bearing for the
resulting distribution and was, until now, a pure assumption. Recorded here
because the next person to reason about `top_k` and `min_p` interacting should
not have to re-derive it.

### 6. gglib is stricter than upstream on values upstream supports

| sent | llama.cpp |
|---|---|
| unknown key | 200, ignored |
| `max_tokens: -1` | **200, accepted** |
| `top_k: 40.0` | **200, accepted** |
| `temperature: "0.7"` | 400, `type must be number, but is string` |

`InferenceConfig::from_openai_json` currently discards the client's *entire*
sampling layer on any one of these, because it deserializes the whole body as a
unit and calls `.unwrap_or_default()`. Two of the four are values llama.cpp
handles without complaint.

This is a separate defect with its own fix, but it belongs in this document
because upstream's behaviour is the reference gglib's coercion policy should be
calibrated against rather than invented: accept what upstream accepts, reject
what upstream rejects, and never lose ten fields over one.

### 7. `/slots.params` is an echo of the request, not the applied chain

The `params` object carries the sampler settings for a request, and it is
exactly the wire evidence #621 and #745 were read by hand. Two limits, both
measured:

- **It appears only on an actively-processing slot.** Idle slots carry `id`,
  `n_ctx`, `speculative`, `is_processing` and nothing more. So it is a
  *sampling* instrument, not a census: it observes requests in flight when a
  poll lands.
- **It reports what was parsed, not what was applied.** Sent `mirostat: 2`
  alongside `top_k: 7`, and `params` still reports `top_k: 7` with a `samplers`
  array byte-identical to the non-mirostat run.

That is the weaker of the two possible readings and it must not be overstated.
It cannot answer "what did the model sample with". It can answer "did what
gglib resolved reach llama-server intact", which is precisely the class both
#621 and #745 belong to.

## Decision

**1. The six redundant floor values are deleted; gglib defers to llama.cpp.**
`top_p`, `top_k`, `repeat_penalty`, `presence_penalty`, `min_p` and
`dry_multiplier` leave `with_hardcoded_defaults()`. Nothing is sent for them
unless a layer names one.

**2. `temperature: 0.7` stays, and its documentation must state that upstream's
is 0.8.** It is the one genuine policy choice in the set, and an undocumented
divergence is how the other six became invisible.

**3. `reasoning_floor()`'s overrides stay.** `presence_penalty: 1.0` and
`min_p: 0.0` for reasoning-tagged models are class-aware policy — llama.cpp has
no notion of a model class — and remain force-written. Note the consequence:
after this change `min_p` is asserted for reasoning models and deferred for
everything else, which is the correct shape and needs saying out loud because
it makes the floor non-uniform for the first time.

**4. The launch-flag emission is deleted.** `to_cli_args` has one production
caller (`command.rs:291`); both go. A `llama/args/sampling.rs` resolver is
added to match its seven siblings, emitting no sampler flags and a
`LaunchDecision::new("sampling", "per-request", ...)` so the launch banner
states the absence rather than leaving it to be inferred.

Beyond removing a write that does nothing, this makes
`/props.default_generation_settings` a permanently clean read of llama.cpp's
own defaults — turning the one-off probe in this ADR into a continuously
available instrument.

**5. A `ParamSource::Deferred` variant carries the provenance.** #739 chose
force-write partly so the value "stays visible as `min_p=floor`". Deferring
answers that objection *better* than force-writing did:

```
top_k    —    <- deferred to llama.cpp (b1-69bf643 default: 40)
```

This distinguishes "gglib picked 40" from "llama.cpp picked 40", which the
current output cannot express at all. The build's default is read from
`/props` and **omitted rather than invented** when unavailable.

**6. Deferral ships only after the readback exists.** The observation organ of
finding 7 lands first, and its own decisions — liveness contract, refusal to
act mid-run, the echo caveat — are deferred to their own ADR rather than
settled here. Rationale in "Two rules that follow".

**7. Scope.** One build, two models, one machine. The controls raise this from
"one reading" to "a build-level property", and no further.

## Two rules that follow

### The pin is what makes deferral safe, and the readback is what keeps it safe

Deleting a floor value changes what the model sees only if upstream's default
differs — which finding 1 excludes for the deleted set *on this build*. The
change is therefore behaviour-preserving by construction here, the same
property #750 claimed when it introduced the pin ("initialised to the release
that was `latest` when the pin landed, so introducing it changed nothing on day
one"), except measured rather than asserted.

But it is only true while the build is pinned. So this ADR carries a
**deletion criterion that runs backwards** from ADR 0002's:

> If a pin bump moves an upstream default that gglib now defers to, the
> readback flags the divergence and this decision is re-taken for that
> parameter.

That closes ADR 0001's loop rather than opening a hole in it: the pin makes
deferral safe, and the observation organ makes the pin's movement visible. It
is also why decision 6 exists — deferring first and observing later would be
trading a known cost for an unmonitored risk, which is the exact trade ADR
0001's `unknown()` discipline refuses.

### A measurement of redundancy is not a measurement of quality

"This value is identical to upstream's default" and "this value is good" are
different claims, and nothing in this document supports the second. The floor's
*numbers* are not being changed by this ADR — only *who supplies them*. Whether
gglib's sampling hierarchy improves agentic outcomes at all remains unmeasured,
as does the temperature-coupling rule, and both need an instrument that does
not exist yet.

Recorded explicitly because the tempting misreading of finding 1 — "most of the
sampling floor is pointless, so the hierarchy is pointless" — does not follow
and would be expensive to act on.

## Consequences

**Good:**

- Six values stop being asserted, so six ways for gglib to silently override a
  future upstream default disappear. #739's failure mode cannot recur on them.
- The launch/body double-write goes, so the process command line stops being a
  misleading record of what a server samples with — which is the surface that
  made #739 hard to see in the first place.
- `/props` becomes a clean instrument, so "what does this build default to" is
  answerable at any time instead of requiring a bespoke raw-server run.
- Provenance gains a distinction it could not previously express, and the
  sampling modules gain the Tier annotation ADR 0001:75-76 has required all
  along and which neither of them carried.
- Three documentation claims that contradicted the code or each other are now
  settled by measurement, including one that was steering a benchmark grid.

**Costs, accepted:**

- A user who bypasses the proxy and curls llama-server directly now gets
  llama.cpp's defaults rather than gglib's. That is correct — they bypassed
  gglib — but it is a behaviour change and belongs in release notes.
- The floor becomes non-uniform: `min_p` is asserted for reasoning models and
  deferred otherwise. Legible, but it is one more thing to hold in mind when
  reading `resolve_layers`.
- Deferral is scoped to a build. The maintenance obligation moves from "keep
  six constants in sync with upstream forever, silently" to "bump the pin and
  read the divergence counter", which is smaller and visible but not zero.
- The readback samples rather than censuses, so a divergence on a short request
  between polls goes unseen. It bounds how fast a regression is caught, not
  whether it is caught.

## Follow-ups

- Build the readback and write its ADR. It is decision 6's precondition, and
  finding 7's two limits — processing-slot-only, echo-not-applied — are its
  starting constraints.
- Fix `from_openai_json` per finding 6, calibrated to upstream's tolerance
  rather than to invention.
- Correct `dry_penalty_last_n` in `docs/sampling.md:315` and
  `src/constants/inferenceDefaults.ts:113-114`; leave `tune/config.rs:76`,
  which was already right.
- Record the sampler chain order in `docs/sampling.md`. Finding 5 is the only
  statement of it anywhere in the tree.
- `frequency_penalty` is a standard OpenAI field that llama.cpp supports and
  `InferenceConfig` does not model, while its twin `presence_penalty` is
  modelled — so one is governed by the hierarchy and the other passes through
  ungoverned. Decide deliberately rather than by omission.
- The temperature-coupling rule remains unmeasured, and `min_p`'s membership in
  the coupled trio is argued nowhere. Separate ADR, separate instrument.
