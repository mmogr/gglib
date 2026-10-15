# ADR 0022 — The proxy does less: no `cache_prompt` pin, no sampling floor, and the dialect normaliser left ungated

- **Status:** Accepted
- **Date:** 2026-10-15
- **Depends on:** [ADR 0003](0003-defer-sampler-defaults-to-llama-cpp.md),
  [ADR 0004](0004-observe-the-sampling-boundary.md)
- **Supersedes:** nothing
- **Superseded by:** nothing
- **Log:** [log-0022](log-0022.md)

> **Amended 2026-10-15.** The line numbers here are at `972b5fc`, not at a
> pin. Rechecked at `b11429`, the claims hold, but kill criterion 1's `/slots`
> reading is 0 after every finished request, so it cannot be taken as written
> ([log-0022](log-0022.md)).

## Context

A day of agentic runs on 2026-10-14 compared requests sent through gglib with
the same requests sent to llama-server directly. The proxy's own per-request
overhead measured nil. The body diff showed what the proxy adds to a request:
a `cache_prompt` key, sampling keys, a token budget, a reordering of keys, and
the stripping of prior reasoning. Two of those were unconditional rules that
cost time or changed a measurement nobody asked them to change: the strip,
and the sampling floor. The runs and their harness are on the owner's machine,
not in the repository; the numbers below are as reported from them.

- **`cache_prompt` is already `true` upstream.** At the pinned checkout
  (`972b5fc`) `task_params::cache_prompt` defaults to `true`
  (`tools/server/server-task.h:54`) and so does the server-wide default
  (`common/common.h:635`). Stage 5 of the request pipeline force-inserted the
  same `true` on every request.
- **The floor broke clearing a model's defaults (#1428).** The import wrote
  the `reasoning` recipe into a `reasoning`-tagged model's
  `inference_defaults`; any `gglib model update` replaced it and marked the row
  user-set; `--clear-inference-defaults` then left no recipe at all, and the
  floor filled the hole with `temperature 0.7`, `presence_penalty 1.0` and
  `min_p 0.0`, and no `top_k` or `top_p`. On the wire, a model forwarded with
  `temperature 1.0, top_k 20, top_p 0.95` before the edit was forwarded with
  `temperature 0.7` and no `top_k`/`top_p` after it. It confounded a four-run
  comparison the same day: both arms ran at `0.7`.
- **The recipe's `max_tokens: 8192` is close to real turns.** No run through
  the proxy under the recipe hit it, but a plan-writing turn on an arm run
  without the cap produced 9,274 completion tokens, and a recipe run's plan
  turn came within 1,918 of the cap. A turn that hits the cap ends
  mid-thought.
- **llama-server parses Qwen's XML tool calls itself.** Every direct run that
  day returned clean `tool_calls`. Not for every model: llama.cpp issue #30118
  (Qwen3.6 tool calls leak into content when the `<tool_call>` tag is left
  out) is a case gglib's normaliser still catches.

## Decision

### 1. The `cache_prompt` pin is deleted

The pipeline adds no `cache_prompt` key. llama-server's own default applies,
and a client's `cache_prompt` passes through as any key gglib does not model
does, `false` included. Stage 5 leaves the stage table; stage 5b keeps its
name.

### 2. There is no sampling floor, and the `reasoning` recipe is never stored

`InferenceConfig::with_hardcoded_defaults` and `reasoning_floor` are deleted,
and with them `temperature 0.7` and the reasoning class's
`presence_penalty 1.0` and `min_p 0.0`: a field no rung names is not sent.
Provenance loses `Floor`; the coupling rule's `FloorCoupled` becomes `Coupled`,
and the explanation's `isReasoning` goes.

The `reasoning` recipe is resolved from the tag, below global settings,
whenever the row stores no published or measured recipe
(`ModelSamplingContext::model_rungs`, which `gglib model explain` and the
request pipeline both read). The import no longer writes it, and the
`auto_detected` origin is deleted. A one-time migration (`user_version` 3,
schema generation 2) clears every stored copy: rows labelled `auto_detected`,
and unlabelled rows equal to the recipe as it was stored. A row with defaults
and no label reads as user-set.

So a person's edit is stored sparse: a field set stands, and every other field
falls through to the next rung exactly as for an untouched model. Clearing a
`reasoning` model's defaults, all at once or a field at a time, gives back the
same resolution and the same `gglib model explain` output as a fresh import.
This closes #1428.

That needs one change to the coupling rule. It passed over every layer but
the one that claimed `temperature`, above it as well as beneath, so a person
who set only `presence_penalty` on a `reasoning` model would have lost it to
the recipe's claim. The rule now passes over only layers *beneath* the claim:
a layer above it keeps what it named. `sampling_discards.rs` and its tests
documented the old direction as correct while a person's first edit copied the
whole recipe into their own defaults; sparse storage made the above-claim case
the common one. The downward half, the one #621 is behind, is unchanged.

Two things existed only for the old direction and go with it: the completion
adapter's erasing of a value its caller named and the fold passed over, and
the tune sweep's switch to keep such a value. A caller's own parameters sit
above every layer the adapter is handed, so the fold no longer passes one
over.

The `reasoning` recipe's `max_tokens` is 32768.

### 3. The dialect normaliser is not gated, because the reading the gate needs does not exist

The gate considered was: skip gglib's dialect normalisation, and the dialect
grammar of stage 6, when llama-server reports at launch that it parses the
template's tool calls itself, read from `GET /props`
`default_generation_settings.params.chat_format` and recorded on the catalog
row like `template_caps`. It is not built.

gglib's own import already classifies a template as a dialect and persists the
spec, and the normaliser runs only for such a model; what has no reading is
the llama-server side.

- **`/props` reports `Content-only` for every template.** `get_res_props`
  (`tools/server/server-context.cpp:5117`) builds a fresh `task_params`,
  copies only `sampling` into it and serialises it, so `chat_format` is the
  struct default, `COMMON_CHAT_FORMAT_CONTENT_ONLY`
  (`tools/server/server-task.h:93`). The format is set per request by
  `apply_chat_session` (`server-task.h:267`), and shows only in `/slots`, for
  a slot's current or previous task (`server-context.cpp:739-745`), after a
  request has run.
- **Even the per-request format does not tell templates apart.** The names at
  this checkout are `Content-only`, `peg-simple`, `peg-native`, `peg-gemma4`
  and `peg-minimax-m3` (`common/chat.cpp:829`). With `--jinja`, every template
  either gets a specialised PEG parser or the autoparser's `peg-native`, or the
  request fails (`common/chat.cpp:1371-1413`); `Content-only` is the legacy
  route taken without jinja. A gate on the format would be a gate on
  `--jinja`, and would skip normalisation for the Qwen3.6 case of #30118,
  which is the case the normaliser is kept for.
- **Stage 6 depends on the normaliser.** It rewrites `tool_choice` to `"none"`
  and installs gglib's own grammar, and the normaliser turns the markup that
  grammar produces back into `tool_calls`; gating one without the other would
  break `tool_choice: "required"` on dialect models.

The normaliser's own deletion criterion stands as written in
`normalize/parsers/delimited.rs`: zero residue from the drift alarm across a
release cycle with the parser bypassed, not the presence of a PEG parser.

## Consequences

- No request carries a `cache_prompt` key of gglib's. A client that sends
  `false` now gets `false`, and llama-server skips prompt reuse for that
  request.
- An untuned model that is not `reasoning`-tagged is sent no temperature and
  decodes at llama.cpp's `0.8`, where it was sent `0.7`. On a turn with tools
  it was capped from `0.7` to `0.3`; nothing is capped now, because the
  ceiling only lowers a value a rung supplied and adds none. The ceiling still
  caps a published recipe above `0.3` on an ordinary model, and nothing else
  in the default configuration.
- A `reasoning` model under a profile that claims a temperature and names no
  penalty (the shipped `coding` profile, `0.2`) is sent no `presence_penalty`
  and no `min_p`, so it decodes at llama.cpp's `0.0` and `0.05`, where the
  floor sent `1.0` and `0.0`. The floor was added for this shape after the
  2026-07-31 incident (repetitive reasoning at a low temperature).
- A layer above the temperature claim keeps its trio members: a client's,
  an operator's flag, a profile's over a model's claim. `gglib chat`'s
  passed-over-flag warning now has only a float JSON cannot carry left to
  report, and says so without the coupling sentence.
- A trusted client's trio value beneath an operator-flag temperature claim
  stays in the body while provenance reports it `Coupled`. This was so on main
  for `repeat_penalty`, and now also covers `presence_penalty` and `min_p`,
  which the reasoning floor overwrote on a tagged model. It is filed as a
  follow-up.
- A published recipe is still stored, and the first edit still replaces it.
  Clearing that model's defaults gives back the `reasoning` recipe for a
  `reasoning`-tagged model and nothing for any other, not the published one.
- `gglib model inspect` and the inspector's stored-defaults view list nothing
  for an untouched `reasoning` model; `gglib model explain` shows the recipe.
- The global settings form states llama.cpp's own number under every empty
  field, `temperature` included, and no longer names values for reasoning
  models. Its Max Tokens field allows up to 32768.
- Wire types change: `DefaultsOrigin` loses `auto_detected`,
  `ProvenanceKindDto` loses `floor` and renames `floorCoupled` to `coupled`,
  `SamplingExplanationDto` loses `isReasoning`, and `contracts/settings/bounds.json`
  loses both floor sections.

## Kill criteria

- **`cache_prompt`.** A pin bump that changes either default named in the
  context (`server-task.h:54`, `common/common.h:635`) away from `true`, or a
  `/slots` reading of `n_prompt_tokens_cache` at 0 on a restored session whose
  request did not send `cache_prompt: false`, reverses decision 1.
- **The floor.** Loop-guard trips (`loop_guard_trips`) on a `reasoning` model
  under a profile that claims a temperature and names no penalty, where the
  same model and profile tripped none before this change, reverse the
  `reasoning` half of decision 2, as a profile value or a recipe rung rather
  than a floor. A measured comparison in which llama.cpp's `0.8` loses to
  `0.7` on an ordinary model reverses the temperature half.
- **`max_tokens`.** A turn ending with `finish_reason: "length"` at 32768 on
  the recipe reopens the number.
- **The gate.** A llama.cpp build whose `/props` names the parser llama-server
  chose for the loaded template, and which distinguishes a template it parses
  from one it does not, reopens decision 3. Until then the residue scanner
  counts what the native parser leaves in content, and the normaliser repairs
  it.

## Out of scope

- The reasoning strip (stage 1), the other unconditional rule the day's runs
  named. It is decided elsewhere.
- Whether the agentic temperature ceiling should exist at all. Its `0.3` is
  unmeasured, as `agentic_temperature_ceiling` says; this ADR only removes the
  floor it mostly capped.
- Any change to the normaliser, the residue scanner or stage 6 beyond leaving
  them as they are.
