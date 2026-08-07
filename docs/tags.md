# Tags & capability detection

When you add a GGUF model, gglib reads its metadata — `tokenizer.chat_template`,
`general.architecture`, and architecture-specific keys — and derives two kinds
of information automatically:

- **Capability flags** stored on the model row and used by the proxy to
  preprocess requests (strict turn alternation, system-role support, …).
- **Tags** in a protected auto-generated namespace (`reasoning`, `agent`,
  `mtp`, `embedding`, `format:*`, …) that drive parser selection and
  llama-server launch flags.

Detection happens at import time in `gglib-gguf::capabilities::detect_all` and
is persisted by `ModelService::import_from_file`. Nothing needs re-downloading
to re-derive it — see [Retagging](#retagging-an-existing-catalog).

## Capability flags

The proxy uses per-model capability flags to preprocess requests before they
reach llama-server: whether the model requires strict user/assistant turn
alternation, supports a system role, can handle tool calls, and so on.

For models whose quantized builds ship without a chat template, the
architecture name (`general.architecture`) acts as a backstop — for example,
`"mistral"` architecture always implies `REQUIRES_STRICT_TURNS`.

You can inspect or override any model's flags at any time:

```bash
# Show current capabilities
gglib model capabilities 3

# Force strict-turn coalescing on
gglib model capabilities 3 --set requires-strict-turns

# Or via the REST API
curl -X PATCH http://localhost:9887/api/models/3/capabilities \
     -H 'Content-Type: application/json' \
     -d '{"requiresStrictTurns": true}'
```

For details on how to add support for a new architecture, see
[`CONTRIBUTING.md`](../CONTRIBUTING.md#model-architecture-registry).

## Dialect specs

The proxy normalizes every stream into strict OpenAI-compatible events
regardless of the dialect llama-server emits (Qwen XML tool calls, bare
`<think>` tags, …). Which dialect parser runs is driven by the model's
**dialect spec** — a structured description of the dialect (envelope
markers, body codecs, emission layout) persisted on the model row in the
`dialect_spec` column and resolved once per request into the
`ModelContext`. One spec is read by all three dialect consumers — the
stream parser, the decode-time GBNF grammar, and detection — so they
cannot drift apart.

### How a spec is detected

At import (and on `gglib model retag`), detection derives the spec in
precedence order:

1. **Template probe** (`gglib-gguf::capabilities::template_probe`): the
   model's own `tokenizer.chat_template` is *executed* against two probe
   conversations differing only in the final assistant turn — plain
   sentinel text vs. a sentinel tool call — and the two renders are
   diffed to extract the tool-call envelope markers. Any model family
   whose template renders tool calls as `MARKERS{json}MARKERS` derives a
   working spec with **zero per-family code and no model-name matching**.
   A spec is only emitted when the rendered payload round-trips as
   `{"name", "arguments"}` JSON — a wrong spec would corrupt client
   output, so non-JSON body dialects (DeepSeek's fenced blocks, Llama 3's
   `"parameters"` key) are refused here and fall through.
2. **Pattern tables**: when the probe cannot derive (template missing —
   common in quantized community GGUFs — or unexecutable, or a non-JSON
   body), the legacy substring heuristics still run, and the
   `qwen-xml`/`hermes` formats they detect map to the built-in Qwen spec.
3. **Nothing**: no spec is stored; the model streams through the identity
   passthrough parser.

### `format:*` dialect tags

The `format:*` system-tag namespace remains as the human-visible label
and the **fallback** for rows persisted before specs existed (no
re-import needed — resolution maps the tag to the built-in spec at
request time, and `gglib model retag` persists a real spec from stored
metadata):

| Tag | Resolves to | When auto-applied |
|-----|-------------|-------------------|
| `format:qwen-xml` | built-in Qwen spec → `DelimitedToolCallParser` | Model name contains `qwen` and the chat template emits `<tool_call>` markup |
| `format:hermes` | built-in Qwen spec → `DelimitedToolCallParser` | Hermes/ChatML-style tool-calling templates |

Models without a spec or dialect tag use the identity
`StandardJsonParser`. A spec/tag is only emitted when tool calling is
actually detected — a stray format hint on a non-tool-calling model would
wire a parser with nothing to parse.

### Decode-time enforcement for dialect models

Parsing is the rescue path; enforcement is stronger. When a client *demands*
a tool call from a model with a JSON-codec dialect spec — `tool_choice:
"required"` or a named function — the proxy also originates a GBNF grammar,
generated from the same spec the parser reads, that llama-server applies at
decode time, so a malformed envelope, truncated JSON, or invented tool name
cannot be generated at all. This covers qwen-tagged, hermes-tagged, and
template-derived models alike. `tool_choice: "auto"` stays unconstrained (a
grammar would forbid plain-text answers), a client-supplied
`grammar`/`json_schema`/`response_format` is always respected, and requests
where enforcement engaged are flagged in the proxy log and the dashboard's
recent-request list. Kill switch:

```bash
GGLIB_DISABLE_GRAMMAR=1 gglib proxy
```

## Capability tags

Alongside `format:*` tags, gglib detects **capability tags** at import time
from GGUF metadata, which drive automatic flag selection at serve time:

| Tag | Detection trigger | Effect |
|-----|-------------------|--------|
| `agent` | Chat template contains tool-calling syntax | `--jinja` auto-enabled |
| `reasoning` | Chat template contains `<think>` / DeepSeek reasoning tokens | `--reasoning-format deepseek` auto-enabled; pre-tuned sampling defaults written (see [docs/sampling.md](sampling.md#reasoning-model-auto-defaults)) |
| `mtp` | `{arch}.nextn_predict_layers > 0` in GGUF metadata | `--spec-type draft-mtp --spec-draft-n-max 2 --spec-draft-p-min 0.75` auto-enabled |
| `embedding` | Non-none `{arch}.pooling_type`, or an encoder-only `general.architecture` | `--embeddings` auto-enabled; the server refuses chat completions and serves `/v1/embeddings` |

The taxonomy also reserves `vision`, `code`, and `moe` as recognized capability
tags in the same auto-generated namespace; they carry no launch-flag effect
today.

### Overriding MTP

```bash
# Disable MTP on a tagged model
gglib serve <id> --mtp-draft-n-max 0

# Explicit settings (4 draft tokens, 80% p-min)
gglib serve <id> --mtp-draft-n-max 4 --mtp-draft-p-min 0.8
```

Disable MTP globally on **every** launch path (including proxy auto-start,
where no per-model flag is reachable) with an environment variable — useful
for A/B testing speculative decoding as a suspect for long-context issues:

```bash
# Truthy values: 1, true, yes, on (case-insensitive)
GGLIB_DISABLE_MTP=1 gglib proxy
```

## System-tag protection

Auto-detected tags are protected as system tags: `gglib model remove-tag` will
reject any attempt to remove a `format:*` tag (use the `_force` service path
for admin operations). User-curated tags outside the auto-generated namespace
are never touched by detection or retagging.

## Retagging an existing catalog

Models imported before format-tag or dialect-spec detection landed can be
retagged in place from their persisted GGUF metadata — no re-download
required. Retagging re-runs the full detection stack, template probe
included, and persists the derived dialect spec alongside the tags:

```bash
# Additive: append missing format:* tags and fill a missing dialect spec,
# never remove or overwrite anything
gglib model retag --all

# Full rebuild: drop and re-derive every auto-generated tag AND the dialect
# spec (including clearing one that is no longer derivable), preserving
# user tags
gglib model retag --all --full

# Single model
gglib model retag qwen3-30b
```

Re-registering a model (re-import or re-download) also re-derives the spec,
overwriting the stored one — the same semantics as `--full`.

End-to-end round-trip coverage lives in
[`crates/gglib-proxy/tests/integration_proxy_pipeline.rs`](../crates/gglib-proxy/tests/integration_proxy_pipeline.rs).
