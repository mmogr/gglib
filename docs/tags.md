# Tags & capability detection

When you add a GGUF model, gglib reads its metadata — `tokenizer.chat_template`,
`general.architecture`, and architecture-specific keys — and derives two kinds
of information automatically:

- **Capability flags** stored on the model row and used by the proxy to
  preprocess requests (strict turn alternation, system-role support, …).
- **Tags** in an auto-generated namespace (`reasoning`, `agent`, `mtp`,
  `embedding`, …) that drive llama-server launch flags.

At import, the flags come from `gglib_core::domain::infer_from_chat_template`
and the architecture, and the tags from `gglib-gguf::capabilities::detect_all`,
whose `agent` tag is that same tool-call guess. Nothing needs re-downloading
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

# Or via the REST API, with the daemon's token from <data root>/data/daemon_token
curl -X PATCH http://localhost:9887/api/models/3/capabilities \
     -H "Authorization: Bearer $(cat "$DATA_ROOT/data/daemon_token")" \
     -H 'Content-Type: application/json' \
     -d '{"requiresStrictTurns": true}'
```

For details on how to add support for a new architecture, see
[`CONTRIBUTING.md`](../CONTRIBUTING.md#model-architecture-registry).

## Tool calls and reasoning

gglib does not parse a model's tool calls or reasoning; llama-server does.
Tool-call markup llama-server leaves in the text reaches the client as text. A
call that breaks the client's schema is re-issued by
[tool-call repair](tool-call-repair.md).

### The dialect drift alarm

The proxy watches client-visible output for known tool-call markers, on both
the streaming and non-streaming paths. A hit never alters the response: it is
logged (`dialect residue reached client-visible output`), counted as
`dialect_residue_total` on the dashboard snapshot, and flagged on the request's
entry in the recent-request list. A firing alarm means llama-server did not
parse that model's tool calls.

## Capability tags

gglib detects **capability tags** at import time from GGUF metadata, which
drive automatic flag selection at serve time:

| Tag | Detection trigger | Effect |
|-----|-------------------|--------|
| `agent` | Chat template contains tool-calling syntax | `--jinja` auto-enabled |
| `reasoning` | Chat template contains `<think>` / DeepSeek reasoning tokens | `--reasoning-format deepseek` auto-enabled; the `reasoning` sampling recipe applies (see [docs/sampling.md](sampling.md#2-the-reasoning-recipe-fallback)) |
| `mtp` | `{arch}.nextn_predict_layers > 0` in GGUF metadata | `--spec-type draft-mtp --spec-draft-n-max 2 --spec-draft-p-min 0.75` auto-enabled |
| `embedding` | Non-none `{arch}.pooling_type`, or an encoder-only `general.architecture` | `--embeddings` auto-enabled; the server refuses chat completions and serves `/v1/embeddings` |

The taxonomy also reserves `code` and `moe` as recognized capability tags in
the same auto-generated namespace; they carry no launch-flag effect today.

Image input is not a tag. A model in this library reads images exactly when
it is linked to a projector, the second GGUF llama-server loads with
`--mmproj`: the link is `models.projector_path`, set with `gglib model update
<model> --projector <path>` or the Images row of the model inspector, and
cleared with `--no-projector` or the row's "None (text only)". A download from a
repository that has projectors fetches one with the model and links it.
`gglib model list` shows it in its `Images` column and `gglib model inspect`
on its `Projector` line
([ADR 0015](adr/0015-a-model-reads-images-through-its-projector.md),
decision 1).

A model of the paired machine (`--remote`, or a far model on the chat page)
is judged by something other than this library's link: on that machine, by
its own link there, and its refusal comes back with the same code.

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

## Editing tags

There is no CLI subcommand for removing a tag — tags are edited through the
model inspector in the GUI and the HTTP API
(`DELETE /api/models/{id}/tags/{tag}`). User-curated tags outside the
auto-generated namespace are never touched by detection or retagging.

## Retagging an existing catalog

Models imported before a release added a tag can be retagged in place from
their persisted GGUF metadata — no re-download required:

```bash
# Additive: append missing tags, never remove anything
gglib model retag --all

# Full rebuild: drop and re-derive every auto-generated tag, preserving user
# tags
gglib model retag --all --full

# Single model
gglib model retag qwen3-30b
```

Re-registering a model also re-derives its tags, replacing the stored ones.
Re-downloading does this implicitly;
re-importing a file already in the library needs `--reimport`, because adding
a file that is already there is otherwise a conflict rather than a silent
overwrite:

```bash
# Re-derive the detected metadata: tags, plus the columns `retag` does not
# touch — capabilities, quantization, context length, expert counts
gglib model add --reimport ~/models/qwen3-30b.gguf
```

Without `--reimport` the command reports the model already present and changes
nothing.

`--reimport` refreshes what was *detected*, not the whole row, and the columns
it touches do not all behave alike:

| Column | On refresh |
|---|---|
| `tags`, `capabilities` | Replaced — including with nothing, so these **can** be cleared |
| `quantization`, `context_length`, expert counts | Replaced only when newly detected; a stored value is never emptied |
| `name`, parameter count, `architecture` | Untouched — a name you chose yourself survives |

This is a CLI-only workflow. The GUI's *Add a GGUF file…* and `POST /api/models` have
no equivalent — adding a file already in the library is a `409` there, with no
way to ask for the overwrite.
