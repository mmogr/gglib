# Sampling resolution

gglib treats sampling as server-side configuration, not something every client
gets to improvise. Every request that reaches llama-server — from the proxy,
`gglib serve`, `gglib chat`, or `gglib q` — resolves its sampling parameters
through the same hierarchy, and `gglib model explain` shows exactly how any
given model resolves.

## The 5-level merge hierarchy

Each level fills in only the fields left unset by the previous level:

```
Request override  →  Inference profile  →  Per-model defaults  →  Global settings  →  Floor
```

The "Request override" level is gated by `trust_client_sampling` (see
[Client sampling authority](#client-sampling-authority)) — untrusted by
default, it drops out of the hierarchy entirely except for `max_tokens`.

"Per-model defaults" isn't always one rung: it sits *above* global settings when a person set
it, and *below* global settings when gglib auto-detected it — see [Reasoning model
auto-defaults](#reasoning-model-auto-defaults) and [Where a model's defaults came
from](#where-a-models-defaults-came-from) below.

The full set of configurable parameters:

| Parameter | CLI flag | Range | Floor | Notes |
|-----------|----------|-------|-------|-------|
| `temperature` | `--temperature` | 0.0 – 2.0 | **0.7** | The only value gglib asserts; upstream's is 0.8 |
| `top_p` | `--top-p` | 0.0 – 1.0 | *(deferred)* | llama.cpp default 0.95 |
| `top_k` | `--top-k` | int | *(deferred)* | llama.cpp default 40 |
| `max_tokens` | `--max-tokens` | int | *(none)* | Deliberately unset — see below |
| `repeat_penalty` | `--repeat-penalty` | > 0.0 | *(deferred)* | llama.cpp default 1.0 |
| `presence_penalty` | `--presence-penalty` | 0.0 – 2.0 | *(deferred)*, or **1.0** on a `reasoning`-tagged model | llama.cpp default 0.0; see below |
| `min_p` | `--min-p` | 0.0 – 1.0 | *(deferred)*, or **0.0** on a `reasoning`-tagged model | llama.cpp default 0.05; see below |
| `frequency_penalty` | `--frequency-penalty` | -2.0 – 2.0 | *(none)* | llama.cpp default 0.0; scales with how often a token already appeared |
| `dynatemp_range` | `--dynatemp-range` | ≥ 0.0 | *(none)* | llama.cpp default 0.0, i.e. dynatemp off |
| `dynatemp_exponent` | `--dynatemp-exponent` | > 0.0 | *(none)* | llama.cpp default 1.0; inert without a range |
| `top_n_sigma` | `--top-n-sigma` | ≥ -1.0 | *(none)* | llama.cpp default -1.0, i.e. off |
| `dry_multiplier` | `--dry-multiplier` | 0.0 – 5.0 | *(deferred)* | llama.cpp default 0.0, i.e. DRY off |
| `dry_base` | `--dry-base` | > 1.0 | *(none)* | llama.cpp default 1.75 |
| `dry_allowed_length` | `--dry-allowed-length` | int ≥ 0 | *(none)* | llama.cpp default 2 |
| `dry_penalty_last_n` | `--dry-penalty-last-n` | -1 or ≥ 0 | *(none)* | llama.cpp default 64; 0 disables |

**These are the *build's* defaults, and a model can move five of them.** Since
llama.cpp PR #17120 a GGUF can carry `general.sampling.*` keys, which llama.cpp
applies over its own defaults for every field no launch flag sets — and gglib
sets none. So for `temperature` (`general.sampling.temp`), `top_p`, `top_k`,
`min_p` and `repeat_penalty` (`general.sampling.penalty_repeat`) the effective
default is per-model, not per-build. `presence_penalty` and `dry_multiplier`
have no GGUF key and remain the build's.

The proxy dashboard's Sampling Readback panel names which of the two supplied
each value on the running model — and, separately, whether gglib is *overriding*
what the model published. Those are different questions: `/props` reporting a
value as model-supplied says the build's own default is unobservable for that
field, not that the model's number is what the sampler uses. If gglib names the
field, gglib's number wins.

`gglib model explain` answers the same question per field, from stored
configuration, with a note under any row the model published a value for:

```
temperature      1.0     ← per-model defaults (auto-detected: reasoning tag)
      ! general.sampling.temp = 0.33; gglib is sending 1
top_k            —       ← unset by design
      · general.sampling.top_k = 17; gglib defers to it
```

`gglib model inspect` lists the raw `general.sampling.*` keys a model carries,
including the seven llama.cpp reads that gglib does not model.

**"Deferred" means gglib sends nothing and llama.cpp's own default applies.**
[ADR 0003](adr/0003-defer-sampler-defaults-to-llama-cpp.md) measured each of
those six against a bare `llama-server` on the pinned build and found gglib's
floor value was *exactly* the upstream default in every case. Restating a value
that is already the answer is not a decision — it is a redundant assertion that
silently overrides whatever upstream chooses next, which is precisely how #739
shipped a `min_p` floor that disabled the tail cut on every untuned request.

The numbers a model actually decodes at are unchanged. What changed is who
supplies them, and therefore what happens when llama.cpp changes its mind:
gglib now follows rather than overrides — and llama.cpp may in turn follow the
model author, where the GGUF declares a preference. The pinned build plus the `/props`
baseline check in
[ADR 0004](adr/0004-observe-the-sampling-boundary.md) are what make that safe —
a pin bump that moves one of these defaults is flagged rather than absorbed.

gglib also no longer passes *any* sampler flags on the llama-server command
line. They were inert for request behaviour (the request body wins) and
actively harmful to observation, because a launch flag overwrites the `/props`
table the baseline check reads.

## The order llama.cpp applies them in

gglib sends four truncation samplers on every request and never sets
`--samplers`, so the order they compose in is llama.cpp's and it is
load-bearing — `top_k` running before `min_p` is a different distribution from
the reverse. Measured on the pinned build:

```
penalties → dry → top_n_sigma → top_k → typ_p → top_p → min_p → xtc → temperature
```

Note where `temperature` sits: **last**. The truncation samplers cut the
candidate set from the *unscaled* distribution, and temperature reshapes
whatever survives. This is worth knowing before reasoning about the coupling
rule below, which is justified on `presence_penalty` and `repeat_penalty`
competing with "how sharp the temperature makes the distribution".

Re-measure with `scripts/experiments/sampler_wire_semantics.py` after a pin
bump; the chain is a property of the build.

## Temperature coupling

**`temperature`, `presence_penalty`, `repeat_penalty` and `min_p` are coupled.**
The last three are only meaningful relative to how sharp `temperature` makes the
sampling distribution, so they always come from whichever layer supplied the
`temperature` — never a lower layer, which would apply a penalty tuned for a
distribution nothing above it asked for. If that layer left one of them
unset (or if no layer names a temperature at all, in which case the pair-up
doesn't apply), it falls to the floor rather than to a lower layer's value.

**The DRY parameters are not coupled**, and gap-fill independently like
`top_p` and `top_k`. They were coupled briefly, on the argument that a
repetition penalty is a repetition penalty. That symmetry is false:
`presence_penalty` and `repeat_penalty` are flat logit offsets competing
directly with temperature's sharpening, whereas DRY's strength is set by its
own `dry_base` and `dry_allowed_length` and it targets verbatim sequence
repetition — which gets *worse* at low temperature, not milder. Coupling it
meant `gglib config profile set coding --dry-multiplier 0.8` silently resolved
to `0.0` on any model whose defaults name a temperature, which is every
`reasoning`-tagged model.

The floor itself is class-aware for two fields, `presence_penalty` and
`min_p`. Most models get neither — both are deferred to llama.cpp — but a
`reasoning`-tagged model (Qwen3.6, DeepSeek-R1, QwQ, …) degrades into
repetitive reasoning loops under greedy or near-greedy decoding, so gglib
asserts `presence_penalty: 1.0` for those: a real guard, without asserting the
model's full tuned recipe onto a temperature it wasn't chosen for.

**This makes the floor non-uniform in what it names, not just in what it names
it as.** A reasoning model is sent `min_p: 0.0` on the wire; every other model
is sent no `min_p` key at all and decodes at llama.cpp's 0.05. That asymmetry
is deliberate — one is a measured divergence from upstream, the other is
agreement with it — but it will look like a bug to anyone diffing two requests
without this paragraph.

`top_p`, `top_k` and `max_tokens` are uncoupled and fill from any layer
independently regardless of temperature.

**`max_tokens` has no fallback, by design.** Resolution force-writes every set
parameter into the outgoing request, so a fallback here would cap *every* request
that did not name its own — silently truncating long answers. Left unset, no
`max_tokens` key is sent and llama-server applies its own `n_predict` default of
`-1`, generating until a stop token or the context limit. Explicit per-request,
per-profile and per-model values are unaffected.

**`min_p` is deferred rather than restated, and that took two attempts.**
The floor was once `0.0`, which reads like an absence but was not one:
resolution force-writes every set parameter, so it explicitly turned off the
tail cut on every request that did not set its own. #739 fixed the behaviour by
restating llama.cpp's `0.05` — correct in its effect, and it kept the value
"visible as `min_p=floor` in provenance" at the cost of a permanent silent
override. Deferral answers the same objection better: provenance reports it as
unset *because it is unset*, and the readback names llama.cpp's own number
instead of gglib restating it.

`reasoning`-tagged models still assert `0.0`, matching Qwen3.6's guidance to
disable min-p. That is a measured divergence from upstream, so it stays
force-written.

## Per-model defaults written at import

A model arrives with one of two recipes stored as its per-model defaults, and
gglib prefers the author's own wherever it can get it.

### 1. The author's published recipe (preferred)

On a `HuggingFace` download gglib fetches `generation_config.json` — the file
every `transformers` user gets by default — and stores the sampling values it
names. It looks in the base repo rather than the quant repo the GGUF came from,
following the repo's own `base_model:` tags, because that is where the file
lives.

This is best-effort and every failure is the same failure: a repo publishing no
such file, a gated base repo (Llama and Gemma, routinely), no network, or a
file naming nothing gglib models all fall back to the guess below. Nothing
fails an import over a sampling recipe.

Three values are deliberately not adopted from it:

| in the file | why gglib ignores it |
|---|---|
| `max_new_tokens` / `max_length` | `max_tokens` is unset by design, so nothing but the client bounds a response |
| `do_sample: false` | gglib has no greedy mode, and forcing `temperature: 0` is the near-greedy setting [ADR 0004](adr/0004-observe-the-sampling-boundary.md)'s addendum bans for reasoning models. Logged, never applied |
| anything out of range | dropped rather than clamped — clamping invents a number the author did not choose and attributes it to them |

### 2. The `reasoning` tag guess (fallback)

Models tagged `reasoning` at import time (Qwen3.6, DeepSeek R1, QwQ, etc. — see
[docs/tags.md](tags.md)) receive a pre-tuned `InferenceConfig` profile instead,
when no published recipe could be fetched:

```
temperature=1.0  top_p=0.95  top_k=20  max_tokens=8192
presence_penalty=1.5  min_p=0.0  repeat_penalty=1.0
```

A published recipe **replaces** this rather than merging with it. Merging would
produce a recipe no author published and gglib cannot defend, labelled as
though somebody had — and it would defeat the temperature-coupling rule, which
exists so a layer naming a temperature is not paired with penalties tuned for a
different one. A model whose published recipe names a temperature but no
`presence_penalty` therefore falls to the reasoning floor's `1.0` for it, not
to this table's `1.5`.

These are baked into the database at download time and are fully user-overridable:

```bash
# Inspect all stored details for a model (arch, quant, capabilities, inference defaults)
gglib model inspect <id>

# Show every resolved parameter and which layer supplied it
gglib model explain <id>

# Override individual params
gglib model update <id> --presence-penalty 0.8 --max-tokens 32768

# Clear all inference defaults (revert to global/hardcoded chain)
gglib model update <id> --clear-inference-defaults
```

All the same flags are available on `gglib serve`, `gglib chat`, and `gglib q` as
per-invocation overrides that sit at the top of the hierarchy.

## Where a model's defaults came from

gglib tracks whether a model's stored `inference_defaults` were set by a person
or written automatically at import, and the two rank differently:

```
Request override → Inference profile → Per-model defaults (user-set) → Global settings
  → Per-model defaults (auto-detected) → Floor
```

Inside `gglib proxy` there is one more distinction. "Request override" is two
separate rungs there — an operator's own `gglib proxy --temperature …` flags
sit **above** the client's request parameters, because the person running the
server stating what the server does cannot be true if any client silently
outranks it. So the pipeline folds **six** rungs:

```
cli → client → profile → model (user-set) → global → model (auto-detected) → Floor
```

Six is the number to hold onto. Three separate doc comments in the code said
five, six and seven at various points, and the provenance test helper was built
five-wide against a six-wide ladder, so nothing caught the drift.

A deliberate per-model choice (`gglib model update --presence-penalty …`, or an edit in
the WebUI) keeps outranking global settings — that's what "per-model" is supposed to
mean. An unreviewed recipe does not: it ranks *below* global
settings instead of silently shadowing them.

**Both unreviewed origins rank identically**, and the rung is labelled with
which one it was. A `published` recipe read from the model author and an
`auto_detected` guess from the `reasoning` tag are both things nobody in this
installation chose, and rank is about exactly that. What differs is the
evidence, and that decides which one gets *written* at import — not where it
sits once it is. Without this, a model tagged `reasoning`
would always resolve its full auto-written recipe over anything configured globally, with
no way to tell the two apart in the resolved output.

```bash
# Shows whether the current defaults are user-set, published or auto-detected
gglib model inspect <id>

# Shows the rung each parameter actually resolved from, for this model
gglib model explain <id>

# ...and how selecting a profile changes that resolution
gglib model explain <id> --profile coding

# Any explicit edit marks the defaults user-set from then on, even if the
# values happen to match what gglib would have guessed
gglib model update <id> --presence-penalty 0.8
```

Rows written before this distinction existed have nothing stored for it — there's no
migration that goes back and stamps every existing row, since the two backend service
processes (`gglib-axum`, standalone `gglib`) don't share a single startup hook to run one
in. Instead, every read derives the answer on the spot: a stored value that matches the
reasoning recipe byte-for-byte is treated as auto-detected, and anything else as user-set.

## `gglib model explain`

`gglib model explain` is the direct answer to "why is this parameter this
value?". It resolves through the same code the proxy does and labels each
result with the layer it came from — including the cases that surprise people:
a parameter sitting on the floor because a higher layer claimed the
temperature, and a `reasoning` model's auto-written recipe losing to global
settings.

```
temperature      0.2     ← profile 'coding'
top_k            20      ← global settings
presence_penalty 1.0     ← reasoning floor (coupled to temperature layer)
top_p            —       ← unset by design (llama.cpp's own default applies)
max_tokens       —       ← unset by design
```

A `—` is not a gap. Since ADR 0003 it is the normal answer for six of the seven
samplers on a model that is not `reasoning`-tagged: gglib names no value and
llama.cpp supplies its own. The row is still worth printing, because "gglib
chose 0.95" and "llama.cpp chose 0.95" are different facts and only one of them
changes when the pinned build moves.

## Client sampling authority

By default, an external client's own sampling parameters (`temperature`, `top_p`,
`top_k`, `presence_penalty`, `frequency_penalty`, `repeat_penalty`, `min_p`, and
the DRY and entropy-adaptive fields) are **not honoured** — they are read off the
incoming request and then dropped, so the request resolves exactly as if the client
had sent none of them. `max_tokens` is the one exception: it is a budget, not a
taste, and ignoring it would silently truncate the client's own turns, so it is
always forwarded regardless of this setting.

Sampler keys gglib does not model at all (`mirostat` and its parameters,
`typical_p`, `xtc_probability`/`xtc_threshold`, `dry_sequence_breakers`,
`repeat_last_n`, `min_keep`, and the `samplers` chain-order array) are **stripped
from the untrusted body** rather than gated: they have no place in the hierarchy
to be dropped from, and before the strip they rode the body straight to
llama-server — an untrusted client could replace the entire configured sampling
chain with `mirostat: 2` and nothing in the stack would notice. Functional keys
(`stop`, `grammar`, `json_schema`, `response_format`, `logit_bias`, `n_probs`)
are never stripped: they say what the request *is*, not how it should sample.
Everything dropped — gated and stripped alike — is recorded on the request's
sampling decision, so a client repeatedly trying to steer sampling is visible
to the operator rather than silently overruled.

This is the default because many OpenAI-compatible clients send fixed sampling
values with no user-facing control behind them. VS Code Copilot's LLM Gateway, for
one, hardcodes `temperature: 0` on every agent request — that is boilerplate the
extension always sends, not a deliberate choice by whoever is using it. Trusting it
by default would let that boilerplate silently outrank a model's own tuned defaults
and this server's global settings, defeating the point of configuring either.

```bash
# Inspect the current setting
gglib config settings show

# Trust clients that expose real sampling controls to their user (e.g. OpenWebUI)
gglib config settings set --trust-client-sampling true

# Back to the default
gglib config settings set --trust-client-sampling false
```

`{model}:{profile}` selection is unaffected either way — that is not a client
sampling parameter, it is part of the requested model name, and profiles remain the
sanctioned way for a client to express a sampling preference without needing to be
trusted (see below). In-process callers (`gglib chat`, `gglib q`) are also
unaffected: their sampling parameters are gglib's own typed configuration, not an
external client's request body, so they are always honoured.

## Inference profiles (`<model>:<profile>`)

One proxy often serves clients that want incompatible sampling: a coding agent
wants low-variance output while a chat UI wants something warmer. Both hit the
same model name, so per-model defaults alone cannot tell them apart.

**Inference profiles** are named sampling overrides selected per request by
suffixing the model:

```bash
# Install the starter profiles (coding, chat, creative), then edit to taste
gglib config profile install-templates
gglib config profile list

# Create or adjust one — only the flags you pass are set
gglib config profile set coding --temperature 0.15 --top-p 0.9
gglib config profile set coding --unset top-p        # back to the model default
gglib config profile show coding
```

A client then selects it as part of the model name:

```jsonc
{ "model": "qwen3.6:coding", "messages": [...] }
```

Profiles are **global** — one `coding` profile applies to every model — and
deliberately **sparse**: only the parameters you set override anything, and the
rest fall through to that model's own defaults. That is what makes a single
profile safe across models with different architectures; a `coding` profile
setting only `top_k` still lets a thinking model contribute its own `top_p`
and `max_tokens`.

One exception: a `coding` profile that sets `temperature` does **not** also
inherit the model's `presence_penalty` — see the coupling rule above. A
profile is presumed to be choosing its own distribution sharpness, so it gets
the floor for the coupled parameters it doesn't name, not the model's recipe
tuned for a different temperature. Set `presence_penalty` explicitly on the
profile if it needs one.

Key behaviours:

- **Bare model names are unchanged.** `qwen3.6` resolves exactly as it always
  did. Nothing is applied unless a profile is named.
- **A real model always wins.** If a model is literally named `qwen3.6:27b`, it
  resolves as that model — adding a profile can never shadow an existing one.
- **An unknown profile is a 404, not a fallback.** If `coding` is renamed or
  deleted, requests naming it fail loudly rather than quietly sampling at the
  wrong temperature.
- **No model reload.** A profile changes only the request body, so switching
  between `qwen3.6:coding` and `qwen3.6` does not restart llama-server or
  invalidate the KV cache.

Set `--list-in-models` on a profile to advertise `<model>:<profile>` in
`/v1/models`, which makes it directly selectable in clients like OpenWebUI:

```bash
gglib config profile set chat --list-in-models
```

Listing is opt-in per profile because the full cross product of models and
profiles would swamp a client's model picker. Unlisted profiles remain fully
usable by name. Profiles can also be managed from the GUI under
**Settings → Inference Profiles**.

## Server launch defaults

In addition to inference parameters, each model can store per-model **server launch
defaults** (e.g., `context_length`) in the `server_defaults` DB column. These are
resolved through a 4-level fallback chain:

```
Runtime request / CLI flag  →  Per-model server_defaults  →  Global settings  →  Hardcoded default (4096)
```

Per-model server defaults can be set via the GUI or API (`PATCH /api/models/:id` with
`serverDefaults: { contextLength: 8192 }`), cleared with `serverDefaults: null`, or
left unchanged by omitting the field. The CLI `serve`, `chat`, and `q` commands
automatically consume these defaults, so models with large context windows don't need
manual `--ctx-size` flags on every invocation.

## DRY

`repeat_penalty` is a flat per-token penalty: it cannot see that a model is
cycling through the same three-line block, only that individual tokens recur.
DRY penalises a token in proportion to the length of the repeated *sequence* it
would extend, which is the shape degenerate loops actually take in long agentic
sessions.

The floor names nothing, so llama.cpp's own default of `0.0` applies and DRY
stays off — by silence now rather than by gglib restating the zero.
Turning it on for every untuned model is a tuning decision, not a default —
enable it per model or per profile:

```bash
gglib model update qwen3.6 --dry-multiplier 0.8
gglib config profile set coding --dry-multiplier 0.8 --dry-allowed-length 3
```

To pick a value from measurement rather than guesswork, sweep it. `0.0` is a
real candidate meaning "off", so one run compares disabled against two
strengths on your own model and task suite:

```bash
gglib benchmark tune <model> --sweep dry_multiplier=0,0.4,0.8
```

`dry_base`, `dry_allowed_length` and `dry_penalty_last_n` have no floor value.
Left unset they are omitted from the request entirely and llama.cpp applies its
own defaults (1.75, 2, and 64), which are reasonable starting points.

Those three numbers are measured against the pinned build, not read from
release notes — `scripts/experiments/sampler_wire_semantics.py`. This paragraph
previously claimed `dry_penalty_last_n` defaults to `-1`, contradicting the
table at the top of this document; the measured value is 64. `-1` is a legal
*value* meaning "scan the whole context", which is probably how the two got
confused, but it is not the default.

## The agentic-turn temperature ceiling

A request carrying a non-empty `tools` array may emit structured output, so its
temperature is **capped** — at `0.3`, and only on models *not* tagged
`reasoning`. Reasoning models have no ceiling; that is a measured decision, not
an omission, and the measurement is below.

### It is a ceiling, and it only overrules a guess

The cap applies *after* resolution, and only when the temperature that won came
from a source nobody deliberately chose: an auto-detected recipe, or the floor.
Anything set by a person — request, profile, per-model, global, CLI — stands
untouched. It never raises a temperature, only lowers one.

That gate is the point. This shipped first as a *floor*, and a floor could never
reach the models that needed it: every `reasoning`-tagged model carries an
auto-detected recipe naming `temperature: 1.0`, and any layer outranks a floor,
so the adjustment was inert on exactly the models used for agentic coding. An
auto-detected recipe is an unreviewed guess written at import time — it already
ranks below global settings for that reason, and a task-aware cap overruling it
is consistent with that.

### Why reasoning models have no ceiling

A reasoning model does not decode its tool call in isolation. The `<think>`
block and the call are one completion under one sampler configuration, so a cap
imposed for structured output lands on the reasoning phase too. This section
used to justify a `0.6` cap — inside the Qwen3 / DeepSeek-R1 recommended band —
as the least-bad compromise. [ADR 0004]'s addendum named the evidence that
would change it, and on 2026-08-10 that experiment ran (tune runs #12–#32:
Qwen3.5-4B Q8_0, 20 paired runs of the full agentic suite per arm, plus a
broken-sampling positive control):

- The uncapped recipe temperature (`1.0`) beat the `0.6` cap on the paired
  composite: 11W–4L–5T, mean +0.067, Wilcoxon one-sided p = 0.0099, bootstrap
  95% CI [+0.017, +0.116].
- The failure the cap existed to prevent never happened: tool-call formatting
  tasks passed **100%** at `1.0` against 98.6% under the cap. The sampler
  chain explains why — llama.cpp truncates (`top_k`, `top_p`, `min_p`) on the
  *unscaled* distribution and applies temperature last, so a tight `top_k 20`
  keeps structured tokens stable regardless of heat.
- The failure the cap was *risking* did happen, under the cap: loop/stagnation
  triggers were more frequent at `0.6` (29/126) than at `1.0` (22/117) —
  cooling a thinking model manufactures the repetition its vendors warn about,
  which the proxy's own loop guard then rejects as a 400.

So the reasoning-class cap is gone: a reasoning model's resolved temperature
stands on agentic turns. The non-reasoning `0.3` cap is untouched — no
non-reasoning model has been measured, and it keeps its old rationale until it
gets the same treatment.

[ADR 0004]: adr/0004-observe-the-sampling-boundary.md

### DRY is not touched

An earlier version forced `dry_multiplier` to `0` on these turns, on the grounds
that structured output legitimately repeats tokens. That was wrong twice over.
llama.cpp's DRY already ships sequence breakers defaulting to `\n`, `:`, `"`,
`*` — two of which are pervasive in JSON — so the case is mitigated upstream.
And because agentic clients send `tools` on *every* request, disabling DRY for
them would disable it for an entire session, which is exactly the workload it
was added for.

If DRY is ever observed mangling tool calls, the lever is
`--dry-sequence-breaker`, not switching the sampler off.

### Scope

It engages on tools being *present*, not on `tool_choice: "required"`: agentic
clients send `"auto"` almost universally, so a `required`-only trigger would
describe nearly no traffic. The corollary is that this is an *agentic-turn*
adjustment, not a tool-emission one — it applies to prose turns in an agentic
session too, which is why the cap is mild rather than near-greedy.

Disable it with `gglib config settings set --agentic-sampling false`, or
per-process with `GGLIB_DISABLE_AGENTIC_SAMPLING=1`.

Two surfaces cannot report it, both by construction: `gglib model explain` and
the GUI's sampling provenance explain *stored configuration* with no request in
hand. The proxy's `sampling resolved` debug line carries `agentic_turn` and
`agentic_ceiling` for the turns where it applied.
