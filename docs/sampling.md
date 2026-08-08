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
| `temperature` | `--temperature` | 0.0 – 2.0 | 0.7 | |
| `top_p` | `--top-p` | 0.0 – 1.0 | 0.95 | |
| `top_k` | `--top-k` | int | 40 | |
| `max_tokens` | `--max-tokens` | int | *(none)* | Deliberately unset — see below |
| `repeat_penalty` | `--repeat-penalty` | > 0.0 | 1.0 | |
| `presence_penalty` | `--presence-penalty` | 0.0 – 2.0 | 0.0, or 1.0 on a `reasoning`-tagged model | See below |
| `min_p` | `--min-p` | 0.0 – 1.0 | 0.05, or 0.0 on a `reasoning`-tagged model | Matches llama.cpp's own default — see below |
| `dry_multiplier` | `--dry-multiplier` | 0.0 – 5.0 | 0.0 (disabled) | DRY repetition penalty; see below |
| `dry_base` | `--dry-base` | > 1.0 | *(none)* | llama.cpp default 1.75 |
| `dry_allowed_length` | `--dry-allowed-length` | int ≥ 0 | *(none)* | llama.cpp default 2 |
| `dry_penalty_last_n` | `--dry-penalty-last-n` | -1 or ≥ 0 | *(none)* | llama.cpp default 64; 0 disables |

## Temperature coupling

**`temperature`, `presence_penalty`, `repeat_penalty`, `min_p` and the four
DRY parameters are coupled.**
The rest are only meaningful relative to how sharp `temperature` makes the
sampling distribution, so they always come from whichever layer supplied the
`temperature` — never a lower layer, which would apply a penalty tuned for a
distribution nothing above it asked for. If that layer left one of them
unset (or if no layer names a temperature at all, in which case the pair-up
doesn't apply), it falls to the floor rather than to a lower layer's value.

The floor itself is class-aware for two fields, `presence_penalty` and
`min_p`: a plain `0.0` presence penalty is fine for most models, but a
`reasoning`-tagged model (Qwen3.6, DeepSeek-R1, QwQ, …) degrades into
repetitive reasoning loops under greedy or near-greedy decoding, so its floor
is `1.0` — a real guard, without asserting the model's full tuned recipe onto
a temperature it wasn't chosen for. `top_p`, `top_k` and `max_tokens` are
uncoupled and fill from any layer independently regardless of temperature.

**`max_tokens` has no fallback, by design.** Resolution force-writes every set
parameter into the outgoing request, so a fallback here would cap *every* request
that did not name its own — silently truncating long answers. Left unset, no
`max_tokens` key is sent and llama-server applies its own `n_predict` default of
`-1`, generating until a stop token or the context limit. Explicit per-request,
per-profile and per-model values are unaffected.

**`min_p` restates llama.cpp's default rather than disabling itself.** The floor
is `0.05`, the same value llama.cpp applies when the flag is omitted. It is
stated here rather than left out because resolution force-writes every set
parameter, so llama-server receives a fully specified request and the value
stays visible as `min_p=floor` in provenance. A `0.0` floor would not read as
"unset" — it would be written too, explicitly turning off the tail cut on every
request that did not set its own. `reasoning`-tagged models floor at `0.0`
instead, matching Qwen3.6's guidance to disable min-p; that is the only other
parameter besides `presence_penalty` whose floor depends on the model.

## Reasoning model auto-defaults

Models tagged `reasoning` at import time (Qwen3.6, DeepSeek R1, QwQ, etc. — see
[docs/tags.md](tags.md)) automatically receive a pre-tuned `InferenceConfig`
profile as their per-model defaults:

```
temperature=1.0  top_p=0.95  top_k=20  max_tokens=8192
presence_penalty=1.5  min_p=0.0  repeat_penalty=1.0
```

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
or written automatically by the auto-default behaviour above, and the two rank
differently:

```
Request override → Inference profile → Per-model defaults (user-set) → Global settings
  → Per-model defaults (auto-detected) → Floor
```

A deliberate per-model choice (`gglib model update --presence-penalty …`, or an edit in
the WebUI) keeps outranking global settings — that's what "per-model" is supposed to
mean. An unreviewed guess from the `reasoning` tag does not: it ranks *below* global
settings instead of silently shadowing them. Without this, a model tagged `reasoning`
would always resolve its full auto-written recipe over anything configured globally, with
no way to tell the two apart in the resolved output.

```bash
# Shows whether the current defaults are user-set or auto-detected
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
max_tokens       —       ← unset by design
```

## Client sampling authority

By default, an external client's own sampling parameters (`temperature`, `top_p`,
`top_k`, `presence_penalty`, `repeat_penalty`, `min_p`) are **not honoured** — they
are read off the incoming request and then dropped, so the request resolves exactly
as if the client had sent none of them. `max_tokens` is the one exception: it is a
budget, not a taste, and ignoring it would silently truncate the client's own turns,
so it is always forwarded regardless of this setting.

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

The floor leaves it off (`dry_multiplier: 0.0`, llama.cpp's own default).
Turning it on for every untuned model is a tuning decision, not a default —
enable it per model or per profile:

```bash
gglib model update qwen3.6 --dry-multiplier 0.8
gglib config profile set coding --dry-multiplier 0.8 --dry-allowed-length 3
```

`dry_base`, `dry_allowed_length` and `dry_penalty_last_n` have no floor value.
Left unset they are omitted from the request entirely and llama.cpp applies its
own defaults (1.75, 2, and -1), which are reasonable starting points.

## The tool-call floor

A request carrying a non-empty `tools` array is asking for structured output,
and resolves against a tighter floor than a chat turn: `temperature 0.15`,
`top_p 1.0`, and **DRY forced off**.

DRY is disabled there deliberately. Structured output legitimately repeats
tokens — braces, quoted keys, the same argument names across a batch of calls —
so a repetition penalty attacks the very structure that makes the call
parseable, and a malformed tool call is the failure the pipeline is least able
to recover from.

It engages on tools being *present*, not on `tool_choice: "required"`. Real
agentic clients send `"auto"` almost universally, so a `required`-only trigger
would describe nearly no traffic.

It composes onto whichever class floor applies rather than replacing it, so a
`reasoning`-tagged model calling tools keeps both of that floor's carve-outs:
its anti-repetition guard, and its deliberately disabled min-p.

Being a floor, every value is still outranked by any layer that names one — the
way to override it for one model is that model's own defaults, not the global
switch. Disable it entirely with
`gglib config settings set --tool-call-floor false`, or per-process with
`GGLIB_DISABLE_TOOL_FLOOR=1`.

Two surfaces do not apply it, both by construction: `gglib model explain` and
the GUI's sampling provenance explain *stored configuration* with no request in
hand, so they always report the reasoning or neutral floor. The proxy's
`sampling resolved` debug line names the floor that actually ran.
