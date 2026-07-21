# GGLib

[![CI](https://github.com/mmogr/gglib/actions/workflows/ci.yml/badge.svg)](https://github.com/mmogr/gglib/actions/workflows/ci.yml)
[![Coverage](https://github.com/mmogr/gglib/actions/workflows/coverage.yml/badge.svg)](https://github.com/mmogr/gglib/actions/workflows/coverage.yml)
[![Release](https://github.com/mmogr/gglib/actions/workflows/release.yml/badge.svg)](https://github.com/mmogr/gglib/actions/workflows/release.yml)
![Version](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/version.json)
[![License](https://img.shields.io/badge/license-AGPL--3.0-blue)](LICENSE)

<!-- crate-docs:start -->

Manage your local GGUF models without remembering file paths or llama.cpp commands.

GGLib keeps a catalog of your GGUFs, handles downloading from HuggingFace, and starts llama-server for you. Use it from the terminal, a desktop app, a web UI, or as an OpenAI-compatible API — they all share the same database and model directory.

## Quick look

```bash
# Download a model from HuggingFace (interactive queue — press [a] to add more, [q] to cancel)
gglib model download bartowski/Qwen2.5-7B-Instruct-GGUF

# List what you have
gglib model list

# Start chatting (launches llama-server automatically)
gglib chat qwen2.5

# Serve a model and use it from any OpenAI client
gglib serve qwen2.5

# Pipe anything into a question
cat error.log | gglib question "what went wrong?"

# Or skip the CLI — open the desktop app or web UI
gglib gui
gglib web
```

## Pipe anything, ask anything

GGLib treats your local model like a Unix tool. Pipe in any text and ask a question — no API keys, no cloud, no context window gymnastics. Use `gglib question` (or `gglib q` for short).

```bash
# Code review a PR diff
git diff main | gglib q "review this for bugs and suggest improvements"

# Understand an error log
journalctl -u myapp --since "1 hour ago" | gglib q "what caused this crash?"

# Summarize a man page
man rsync | gglib q "how do I sync only .rs files, excluding target/?"

# Explain unfamiliar config
cat nginx.conf | gglib q "explain the proxy_pass rules"

# Quick code explanation
cat src/main.rs | gglib q "what does this program do?"

# Get a commit message from staged changes
git diff --cached | gglib q "write a concise commit message for these changes"

# Translate a file
cat README_ja.md | gglib q "translate this to English"

# Use {} as a placeholder to control where input goes
echo "segfault at 0x0" | gglib q "I got this error: {}. What does it mean?"

# Read context from a file instead of stdin
gglib q --file Cargo.toml "what dependencies does this project use?"

# Agentic mode: let the model explore the codebase with filesystem tools
gglib q --agent "How is error handling structured in this project?"
```

Works with any command that produces text. If you can `cat` it, you can ask a local model about it.

## Benchmarking & inference tuning

`gglib benchmark` answers three questions about a model, from the same CLI/API/GUI surface:

```bash
# How fast is it? (llama-bench throughput)
gglib benchmark perf --model qwen2.5

# Side-by-side response quality across models
gglib benchmark compare --model qwen2.5 --model llama3 --prompt "Explain gradient descent"

# What sampling settings make it a good tool-calling agent?
gglib benchmark tune --model qwen2.5 --sweep temperature=0.2,0.5,0.8 --apply-best
```

`tune` sweeps sampling parameters (temperature, top-p, top-k, min-p, repeat-penalty)
against an agentic tool-calling task suite, scoring each candidate on tool-call
accuracy (AST-style structural matching, not string diffing) and resistance to
the agent loop's loop/stagnation guards — the failure mode that matters most
when a local model is powering an agentic coding IDE via `gglib proxy`, where
losing attention over a long session can send it into an infinite tool-call
loop. `--apply-best` writes the winning settings straight to the model's
`inference_defaults`, the same effect as `gglib model update --temperature ...`.

The built-in task suite covers five categories (modeled on the Berkeley
Function Calling Leaderboard, plus one gglib-specific category):

| Category | Tests |
|----------|-------|
| `single_call` | Exactly one tool call is expected |
| `parallel_call` | Multiple independent tool calls in the same turn |
| `multi_turn` | Sequential tool calls that build on prior results |
| `irrelevance` | Correctly abstaining from a tool call when none applies |
| `long_context` | Loop/stagnation resistance after a long simulated prior session |

**Custom task suites** — write your own scenarios (e.g. targeting your real MCP
tools) as a plain JSON array and pass it with `--task-suite path.json`. The CLI
and the GUI (file upload, parsed client-side) accept the identical schema —
see [`gglib-core/assets/tune_default_suite.json`](crates/gglib-core/assets/tune_default_suite.json)
for a full worked example. Each element is:

```jsonc
{
  "id": "single_call_weather",          // stable identifier
  "category": "single_call",            // single_call | parallel_call | multi_turn | irrelevance | long_context
  "system_prompt": null,                // optional
  "history": null,                      // optional: simulated prior turns (long_context only),
                                         // an array of ChatMessage-shaped objects, e.g.
                                         // [{"role":"user","content":"..."},
                                         //  {"role":"assistant","tool_calls":[...]}]
  "user_prompt": "What's the weather in Boston, MA?",
  "tools": [
    { "name": "get_weather", "description": "...", "input_schema": { "type": "object", "properties": { "location": { "type": "string" } }, "required": ["location"] } }
  ],
  "expected": {
    "kind": "tool_calls",                // "tool_calls" | "no_tool_call"
    "calls": [
      { "name": "get_weather", "required_args": { "location": "Boston, MA" }, "ordered": false }
    ]
  }
}
```

Scoring is partial-credit: extra arguments the model supplies are never
penalized, missing required arguments reduce the score proportionally
(`matching / total`), and values are compared structurally (a JSON `1` matches
`1.0` — never a string diff).

## Architecture

Cargo workspace with compile-time enforced boundaries. Adapters → infrastructure → core — never the reverse.

### Model capability detection

When you add a GGUF model, gglib reads its `tokenizer.chat_template` and `general.architecture` fields to automatically detect capability flags: whether the model requires strict user/assistant turn alternation, supports a system role, can handle tool calls, and so on. These flags are stored in the database and used by the OpenAI-compatible proxy to preprocess requests before they reach llama-server.

For models whose quantized builds ship without a chat template, the architecture name (`general.architecture`) acts as a backstop — for example, `"mistral"` architecture always implies `REQUIRES_STRICT_TURNS`.

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

For details on how to add support for a new architecture, see [`CONTRIBUTING.md`](CONTRIBUTING.md#model-architecture-registry).

### History Truncation & Proxy Status

When gglib acts as an OpenAI-compatible proxy (e.g. `gglib web --api-only`), it
defends against a problem with some AI clients: client-side context compaction
can be broken for custom endpoints, causing tool call responses to be permanently
embedded in chat history. After several tool-heavy turns the prompt balloons past
the local model's context window and the model falls into logic loops.

**The proxy intercepts and compacts history automatically.** Before every request
is forwarded to llama-server, a stateless truncation pass runs:

- Any unprotected `role: "tool"` or `role: "assistant"` message whose content
  exceeds **2,000 characters** has its content replaced with a short placeholder.
- The last **4 messages** and all `role: "system"` messages are always preserved.
- If the total payload still exceeds **240,000 characters** (≈ 60,000 tokens)
  after truncation, the request is rejected with HTTP 400
  (`context_length_exceeded`) rather than forwarding a prompt that would cause
  the model to fail.

**Proxy dashboard** — a unified `DashboardSnapshot` of active connections,
per-slot context usage, and recent request history — is available at
`GET /v1/proxy/status` (JSON) and `GET /v1/proxy/status/stream` (live SSE
updates). It's consumed by both `gglib proxy dashboard` (a live terminal
view) and the web GUI's Proxy Dashboard modal. See
[`gglib-proxy`'s docs](crates/gglib-proxy/README.md#proxy-dashboard) for the
full data contract.

```bash
curl http://localhost:9887/v1/proxy/status | jq .
gglib proxy dashboard --port 9887
```

### KV Cache Session Persistence

For sequential multi-agent workflows, enable `--cache --slot-dir <path>` to persist
KV cache state between requests. The proxy automatically saves and restores per-session
slot files (saved atomically via a temp-then-rename), gated by a semaphore to prevent
concurrent access. Stale caches are detected via mtime comparison and skipped
(fail-open). A background sweep evicts the least-recently-used slot files once the
on-disk cache exceeds a byte budget — by default auto-sized from free disk space,
override with `--cache-disk-gb`. Use `gglib proxy cache-clear` to manually clear
cached state.

Disk persistence is skipped automatically for models whose attention keeps only
part of the token history — sliding-window, hybrid (e.g. a GGUF declaring
`full_attention_interval`), and recurrent/SSM architectures. llama-server's slot
files carry KV state and tokens but not the context checkpoints those models need
to resume, so a restore cannot pick up where it left off and instead re-prefills
the whole prompt, while also crowding out the host-RAM prompt cache that *would*
have resumed cheaply. Such models rely on the RAM cache alone; the proxy logs the
decision at startup. Set `GGLIB_FORCE_HYBRID_DISK_CACHE=1` to re-enable the disk
layer anyway (intended for testing an upstream llama.cpp fix).

Independently of disk persistence, every launch auto-sizes llama-server's own
host-RAM prompt cache (`--cache-ram`) from system RAM, the model's weights, and its
KV footprint — override with `--cache-ram-mb`. The KV cache itself defaults to
`q8_0` quantization on both K and V (`--cache-type-k`/`--cache-type-v`), roughly
halving KV memory versus llama-server's own `f16` default; override per-axis or set
`GGLIB_DISABLE_KV_QUANT=1` to fall back to `f16`.

The proxy dashboard reports how the cache resolved and how much it is actually
doing: the RAM budget, whether either tier is degraded, and measured reuse
(prompt tokens served from cache vs. re-processed, per request and in total).
These are raw counts taken from the upstream's own `usage` reporting — there is
no estimated "time saved", since reuse is measured exactly but what it saved
depends on a prefill that never ran.

![Rust Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/tests.json)
![Rust Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/coverage.json)
![TS Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-tests.json)
![TS Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-coverage.json)
![Boundaries](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/boundary.json)

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                    Core Layer                                       │
│                                                                                     │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                              gglib-core                                     │   │
│   │              Pure domain types, ports & traits (no infra deps)              │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                          │
        ┌─────────────┬─────────────┬─────┴─────┬─────────────┬─────────────┐
        ▼             ▼             ▼           ▼             ▼             ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              Infrastructure Layer                                   │
│                                                                                     │
│  ┌────────────┐ ┌────────────┐ ┌────────────┐ ┌────────────┐                      │
│  │  gglib-db  │ │ gglib-gguf │ │ gglib-mcp  │ │ gglib-proxy│                      │
│  │   SQLite   │ │ GGUF file  │ │    MCP     │ │  OpenAI-   │                      │
│  │   repos    │ │   parser   │ │  servers   │ │  compat    │                      │
│  └────────────┘ └────────────┘ └────────────┘ └────────────┘                      │
│                                                                                     │
│  ╔═══════════════════════════════════════════════════════════════════════════════╗  │
│  ║                          External Gateways                                    ║  │
│  ║                                                                               ║  │
│  ║  ┌────────────────────────────────────┐  ┌────────────────────────────────┐   ║  │
│  ║  │      gglib-runtime                 │  │      gglib-download            │   ║  │
│  ║  │  Process lifecycle manager         │  │  Download orchestrator         │   ║  │
│  ║  │  ONLY component that spawns        │  │  ONLY component that contacts  │   ║  │
│  ║  │  & manages llama-server            │  │  HuggingFace Hub               │   ║  │
│  ║  │                                    │  │  (via gglib-hf + optional      │   ║  │
│  ║  │                                    │  │   hf_xet subprocess)           │   ║  │
│  ║  └────────────────────────────────────┘  └────────────────────────────────┘   ║  │
│  ╚═══════════════════════════════════════════════════════════════════════════════╝  │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                          │
                                          ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                   Facade Layer                                      │
│                                                                                     │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                              gglib-app-services                             │   │
│   │         Shared service ops (ensures feature parity across adapters)         │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
│   ┌─────────────────────────────────────────────────────────────────────────────┐   │
│   │                              gglib-bootstrap                                │   │
│   │         Shared composition root (infra wiring for all adapters)             │   │
│   └─────────────────────────────────────────────────────────────────────────────┘   │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                          │
                      ┌───────────────────┼───────────────────┐
                      ▼                   ▼                   ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                   Adapter Layer                                     │
│                                                                                     │
│   ┌─────────────────────┐  ┌──────────────────────┐  ┌──────────────────────────┐   │
│   │    gglib-cli        │  │    gglib-axum        │  │     gglib-tauri          │   │
│   │  CLI interface      │  │  HTTP server         │  │  Desktop application     │   │
│   │  (terminal UI)      │  │  ┌────────────────┐  │  │  ┌────────────────────┐  │   │
│   │                     │  │  │ Serves React   │  │  │  │ Embeds React UI    │  │   │
│   │                     │  │  │ UI (static)    │  │  │  │ (WebView assets)   │  │   │
│   │                     │  │  └────────────────┘  │  │  ├────────────────────┤  │   │
│   │                     │  │                      │  │  │ Embedded Axum      │  │   │
│   │                     │  │                      │  │  │ (HTTP endpoints)   │  │   │
│   │                     │  │                      │  │  └────────────────────┘  │   │
│   └─────────┬───────────┘  └──────────┬───────────┘  └───────────┬──────────────┘   │
│             │                         │                          │                  │
│             └─────────────────────────┼──────────────────────────┘                  │
│                                       │                                             │
│                  All adapters call infrastructure layer via:                        │
│                  • External Gateways (runtime, download)                            │
│                  • Other infrastructure services (db, gguf, mcp, proxy)             │
│                                       │                                             │
└───────────────────────────────────────┼─────────────────────────────────────────────┘
                                        │
                                        ▼
                            ╔═══════════════════════╗
                            ║  External Gateways    ║
                            ║  (from infra layer)   ║
                            ╚═══════════════════════╝
                                        │
                    ┌───────────────────┴────────────────────┐
                    ▼                                        ▼
          ┌──────────────────────┐              ┌──────────────────────┐
          │   gglib-runtime      │              │   gglib-download     │
          │   spawns/manages     │              │   calls HF Hub API   │
          └──────────┬───────────┘              └──────────┬───────────┘
                     │                                     │
                     ▼                                     ▼
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                  External Systems                                   │
│                                                                                     │
│               ┌──────────────────────────────┐                                      │
│               │   llama-server instances     │                                      │
│               │   (child processes)          │                                      │
│               └──────────────────────────────┘                                      │
│                                                                                     │
│               ┌──────────────────────────────┐                                      │
│               │   HuggingFace Hub API        │                                      │
│               │   (HTTPS endpoints)          │                                      │
│               └──────────────────────────────┘                                      │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

Only `gglib-runtime` spawns llama-server processes; only `gglib-download` talks to HuggingFace. Everything else goes through the infrastructure layer.

### Universal Consistency Layer

`gglib` exposes a **strict OpenAI-compatible** chat-completions surface from every entry point — the `gglib-proxy` HTTP server, the `gglib-axum` web API, the `gglib-cli` agent loop, and the `gglib-tauri` desktop app (which routes through the same axum handlers) — regardless of which dialect the upstream `llama-server` happens to emit (Qwen XML tool calls, bare `<think>` reasoning tags, etc.). External clients — OpenWebUI, the OpenAI SDKs, custom scripts — only ever see canonical `chat.completion.chunk` events.

This is achieved by a three-stage pipeline that runs on every streaming request, wired identically into every surface by `gglib-runtime::compose`:

```text
upstream bytes ─► SseStreamDecoder ─► NormalizingStream ─► SseEncoder ─► wire bytes
                  (gglib-core::sse)   (gglib-core::normalize)   (gglib-core::sse)
                       parse              dialect rewrite             re-emit
```

1. **Parse** — `SseStreamDecoder` reassembles SSE frames across arbitrary TCP chunk boundaries and produces typed `LlmStreamEvent`s.
2. **Normalize** — A per-request `ToolCallParser` is selected by model tags via `normalize::registry::get_parser`. Dialect parsers (`QwenXmlParser`) rewrite model-specific markup into strict tool-call / reasoning events. Models without a dialect tag use the identity `StandardJsonParser`. Recoverable parser issues become `NormalizationError` events that are logged and suppressed from the wire; unrecoverable upstream errors surface as a structured `error` data frame followed by `data: [DONE]`.
3. **Re-encode** — `SseEncoder` wraps every event in the canonical envelope (`id: "chatcmpl-…"`, `object: "chat.completion.chunk"`, stable `model` / `created`) so clients see identical framing on every request.

#### `format:*` tag taxonomy

Dialect selection is driven entirely by **system tags** in the `format:*` namespace, persisted on each model row:

| Tag | Parser | When auto-applied |
|-----|--------|-------------------|
| `format:qwen-xml` | `QwenXmlParser` | Model name contains `qwen` and the chat template emits `<tool_call>` markup |
| `format:hermes` | `StandardJsonParser` | Hermes/ChatML-style tool-calling templates |

In addition to `format:*` tags, gglib detects the following **capability tags** at import
time from GGUF metadata, which drive automatic flag selection at serve time:

| Tag | Detection trigger | Effect |
|-----|-------------------|--------|
| `agent` | Chat template contains tool-calling syntax | `--jinja` auto-enabled |
| `reasoning` | Chat template contains `<think>` / DeepSeek reasoning tokens | `--reasoning-format deepseek` auto-enabled |
| `mtp` | `{arch}.nextn_predict_layers > 0` in GGUF metadata | `--spec-type draft-mtp --spec-draft-n-max 2 --spec-draft-p-min 0.75` auto-enabled |

Override MTP from the CLI:
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

These tags are **auto-detected at import time** by `gglib-gguf::capabilities::detect_all` and persisted by `ModelService::import_from_file`. They are protected as system tags: `gglib model remove-tag` will reject any attempt to remove a `format:*` tag (use the `_force` service path for admin operations).

#### Inference parameter defaults

Sampling parameters are resolved through a **5-level merge hierarchy** on every request. Each level fills in only the fields left unset by the previous level:

```
Request override  →  Inference profile  →  Per-model defaults  →  Global settings  →  Hardcoded fallback
```

The full set of configurable parameters:

| Parameter | CLI flag | Range | Hardcoded fallback | Notes |
|-----------|----------|-------|--------------------|-------|
| `temperature` | `--temperature` | 0.0 – 2.0 | 0.7 | |
| `top_p` | `--top-p` | 0.0 – 1.0 | 0.95 | |
| `top_k` | `--top-k` | int | 40 | |
| `max_tokens` | `--max-tokens` | int | *(none)* | Deliberately unset — see below |
| `repeat_penalty` | `--repeat-penalty` | > 0.0 | 1.0 | |
| `presence_penalty` | `--presence-penalty` | 0.0 – 2.0 | 0.0 (disabled) | Recommended 1.5 for reasoning models |
| `min_p` | `--min-p` | 0.0 – 1.0 | 0.0 (disabled) | 0.05 is llama.cpp built-in default when omitted |

**`max_tokens` has no fallback, by design.** Resolution force-writes every set
parameter into the outgoing request, so a fallback here would cap *every* request
that did not name its own — silently truncating long answers. Left unset, no
`max_tokens` key is sent and llama-server applies its own `n_predict` default of
`-1`, generating until a stop token or the context limit. Explicit per-request,
per-profile and per-model values are unaffected.

**Reasoning model auto-defaults.** Models tagged `reasoning` at import time (Qwen3.6,
DeepSeek R1, QwQ, etc.) automatically receive a pre-tuned `InferenceConfig` profile as
their per-model defaults:

```
temperature=1.0  top_p=0.95  top_k=20  max_tokens=8192
presence_penalty=1.5  min_p=0.0  repeat_penalty=1.0
```

These are baked into the database at download time and are fully user-overridable:

```bash
# Inspect all stored details for a model (arch, quant, capabilities, inference defaults)
gglib model inspect <id>

# Override individual params
gglib model update <id> --presence-penalty 0.8 --max-tokens 32768

# Clear all inference defaults (revert to global/hardcoded chain)
gglib model update <id> --clear-inference-defaults
```

All the same flags are available on `gglib serve`, `gglib chat`, and `gglib q` as
per-invocation overrides that sit at the top of the hierarchy.

#### Inference profiles (`<model>:<profile>`)

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
setting only `temperature` still lets a thinking model contribute its own
`presence_penalty`.

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

#### Server launch defaults

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

#### Backfilling tags on existing catalogs

Models imported before format-tag detection landed can be retagged in place from their persisted GGUF metadata — no re-download required:

```bash
# Additive: only append missing format:* tags, never remove anything
gglib model retag --all

# Full rebuild: drop and re-derive every auto-generated tag, preserving user tags
gglib model retag --all --full

# Single model
gglib model retag qwen3-30b
```

End-to-end round-trip coverage lives in [`crates/gglib-proxy/tests/integration_proxy_pipeline.rs`](crates/gglib-proxy/tests/integration_proxy_pipeline.rs).

### Crate Metrics

#### Core Layer
| Crate | Tests | Coverage | LOC | Complexity |
|-------|-------|----------|-----|------------|
| [gglib-core](crates/gglib-core) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-complexity.json) |

#### Application Layer
| Crate | Tests | Coverage | LOC | Complexity |
|-------|-------|----------|-----|------------|
| [gglib-agent](crates/gglib-agent) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-agent-complexity.json) |

#### Infrastructure Layer
| Crate | Tests | Coverage | LOC | Complexity |
|-------|-------|----------|-----|------------|
| [gglib-db](crates/gglib-db) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-complexity.json) |
| [gglib-gguf](crates/gglib-gguf) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-complexity.json) |
| [gglib-hf](crates/gglib-hf) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-hf-complexity.json) |
| [gglib-download](crates/gglib-download) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-download-complexity.json) |
| [gglib-mcp](crates/gglib-mcp) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-mcp-complexity.json) |
| [gglib-proxy](crates/gglib-proxy) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-proxy-complexity.json) |
| [gglib-runtime](crates/gglib-runtime) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-complexity.json) |

#### Facade Layer
| Crate | Tests | Coverage | LOC | Complexity |
|-------|-------|----------|-----|------------|
| [gglib-app-services](crates/gglib-app-services) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-complexity.json) |
| [gglib-bootstrap](crates/gglib-bootstrap) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-complexity.json) |

#### Adapter Layer
| Crate | Tests | Coverage | LOC | Complexity |
|-------|-------|----------|-----|------------|
| [gglib-cli](crates/gglib-cli) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-complexity.json) |
| [gglib-axum](crates/gglib-axum) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-complexity.json) |
| [gglib-tauri](crates/gglib-tauri) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-tests.json) | ![N/A](https://img.shields.io/badge/coverage-N%2FA-lightgrey) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-complexity.json) |

#### Frontend Layer
| Component | Tests | Coverage | LOC | Complexity |
|-------|-------|----------|-----|------------|
| [Web UI](src/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-tests.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-coverage.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-complexity.json) |

#### Utility Crates
| Crate | Tests | Coverage | LOC | Complexity |
|-------|-------|----------|-----|------------|
| [gglib-build-info](crates/gglib-build-info) | ![N/A](https://img.shields.io/badge/tests-N%2FA-lightgrey) | ![N/A](https://img.shields.io/badge/coverage-N%2FA-lightgrey) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-build-info-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-build-info-complexity.json) |

### Crate Documentation

Each crate has its own README with architecture diagrams, module breakdowns, and design decisions:

| Layer | Crate | Description |
|-------|-------|-------------|
| **Core** | [gglib-core](crates/gglib-core/README.md) | Pure domain types, ports & traits |
| **App** | [gglib-agent](crates/gglib-agent/README.md) | Pure-domain agentic loop (LLM→tool→LLM, port-injected) |
| **Infra** | [gglib-db](crates/gglib-db/README.md) | SQLite repository implementations |
| **Infra** | [gglib-gguf](crates/gglib-gguf/README.md) | GGUF file format parser |
| **Infra** | [gglib-hf](crates/gglib-hf/README.md) | HuggingFace Hub client |
| **Infra** | [gglib-download](crates/gglib-download/README.md) | Download queue & manager |
| **Infra** | [gglib-mcp](crates/gglib-mcp/README.md) | MCP server management |
| **Infra** | [gglib-proxy](crates/gglib-proxy/README.md) | OpenAI-compatible proxy server |
| **Infra** | [gglib-runtime](crates/gglib-runtime/README.md) | Process manager & system probes |
| **Facade** | [gglib-app-services](crates/gglib-app-services/README.md) | Shared application service ops (feature parity) |
| **Facade** | [gglib-bootstrap](crates/gglib-bootstrap/README.md) | Shared composition root (infra wiring) |
| **Adapter** | [gglib-cli](crates/gglib-cli/README.md) | CLI interface |
| **Adapter** | [gglib-axum](crates/gglib-axum/README.md) | HTTP API server |
| **Adapter** | [gglib-tauri](crates/gglib-tauri/README.md) | Desktop GUI (Tauri + React) |
| **Utility** | [gglib-build-info](crates/gglib-build-info/README.md) | Compile-time version & git metadata |

### Module Reference

#### TypeScript Frontend
- **[`components`](src/components/README.md)** – React UI components
- **[`contexts`](src/contexts/README.md)** – React Context providers
- **[`hooks`](src/hooks/README.md)** – Custom React hooks
- **[`pages`](src/pages/README.md)** – Top-level page components
- **[`types`](src/types/README.md)** – Shared TypeScript type definitions
- **[`utils`](src/utils/README.md)** – Shared helpers (formatting, SSE, platform detection)
- **[`services`](src/services/README.md)** – API client layer (HTTP and Tauri IPC)
- **[`commands`](src/commands/README.md)** – CLI command reference (download, llama management)

<!-- crate-docs:end -->

## Interfaces

All interfaces share the same database and model directory. Pick whichever fits your workflow — or use several.

| Interface | Launch | Details |
|-----------|--------|---------|
| **CLI** | `gglib <command>` | [gglib-cli](crates/gglib-cli/README.md) |
| **Desktop GUI** | `gglib gui` | [gglib-tauri](crates/gglib-tauri/README.md), [src-tauri](src-tauri/README.md) |
| **Web UI** | `gglib web` | [gglib-axum](crates/gglib-axum/README.md) — default `0.0.0.0:9887` |
| **OpenAI Proxy** | `gglib proxy` | [gglib-proxy](crates/gglib-proxy/README.md) — works with OpenWebUI, any OpenAI SDK |

**Shell completions** — enable tab completion for your shell:

| Shell | Setup |
|-------|-------|
| fish | `gglib completions fish > ~/.config/fish/completions/gglib.fish` |
| bash | `gglib completions bash > ~/.bash_completion` |
| zsh | `gglib completions zsh > ~/.zsh/_gglib` |
| elvish | `gglib completions elvish > ~/.config/elvish/lib/gglib.elv` |
| powershell | `gglib completions powershell >> $PROFILE` |

<details>
<summary><strong>Security notes</strong></summary>

- Web server binds `0.0.0.0` (LAN-accessible); proxy binds `127.0.0.1` (local only) by default
- No authentication — designed for trusted networks
- Use firewall rules, private subnets, or VPN; do not expose to the public internet without additional auth

</details>

## Installation

Download from the [Releases page](https://github.com/mmogr/gglib/releases):

| Platform | Archive | Post-install |
|----------|---------|--------------|
| **macOS (Apple Silicon)** | `gglib-gui-*-aarch64-apple-darwin.tar.gz` | Run `macos-install.command` to remove quarantine |
| **macOS (Intel)** | `gglib-gui-*-x86_64-apple-darwin.tar.gz` | Same as above |
| **Linux** | `gglib-gui-*-x86_64-unknown-linux-gnu.tar.gz` | Run `gglib-gui` |
| **Windows** | `gglib-gui-*-x86_64-pc-windows-msvc.zip` | Run `gglib-gui.exe` |

### From Source

```bash
git clone https://github.com/mmogr/gglib.git && cd gglib
make setup   # check deps → build frontend → install CLI → offer llama.cpp install
```

`make setup` checks for Rust, Node.js, and build tools; provisions the Miniconda environment for the `hf_xet` fast download helper; builds the web UI; and installs the CLI to `~/.cargo/bin/`. It exits with an error if Python/Miniconda is missing — run it first on new machines.

> **Developer Mode:** When installed via `make setup`, the database (`gglib.db`), config (`.env`), and llama.cpp binaries live inside your repo folder. Downloaded models default to `~/.local/share/llama_models`.

### Prerequisites

- **Rust** 1.70+ (MSRV). Tooling/CI currently pins Rust **1.97.1** via `rust-toolchain.toml` — using that version is recommended. — [rustup.rs](https://rustup.rs/)
- **Python 3 via Miniconda** — [miniconda](https://docs.conda.io/en/latest/miniconda.html) (for hf_xet fast downloads)
- **Node.js** 20.19+ (matches the `package.json` `engines` field) — [nodejs.org](https://nodejs.org/) (for web UI)
- **SQLite** 3.x
- **Build tools**: macOS `xcode-select --install` + `brew install cmake` · Ubuntu `build-essential cmake git` · Windows VS 2022 C++ + CMake

llama.cpp is managed by GGLib — no separate install needed.

<details>
<summary><strong>Makefile targets</strong></summary>

**Installation & Setup:**
- `make setup` — Full setup (dependencies + build + install + llama.cpp)
- `make install` — Build and install CLI to `~/.cargo/bin/`
- `make uninstall` — Full cleanup (removes binary, system data, database; preserves models)

**Building:**
- `make build` / `make build-dev` — Release / debug binary
- `make build-gui` — Web UI frontend
- `make build-tauri` — Desktop GUI
- `make build-all` — Everything (CLI + web UI)

**Development:**
- `make test` / `make check` / `make fmt` / `make lint` / `make doc`

**llama.cpp:**
- `make llama-install-auto` / `make llama-status` / `make llama-update`

**Running:**
- `make run-gui` / `make run-web` / `make run-serve` / `make run-proxy`

**Cleaning:**
- `make clean` / `make clean-gui` / `make clean-llama` / `make clean-db`

</details>

<details>
<summary><strong>Manual installation (Cargo)</strong></summary>

```bash
cargo install --path .
```

</details>

<details>
<summary><strong>Configuring the models directory</strong></summary>

Default: `~/.local/share/llama_models`. Change via any of:

- `make setup` prompt
- `.env` file: `GGLIB_MODELS_DIR=/absolute/path`
- CLI: `gglib config models-dir set <path>` or `gglib --models-dir <path> download …`
- GUI/Web: Settings modal (gear icon)

Precedence: CLI flag → env var → default. Changing the directory does **not** move existing files.

</details>

## Development

Start the backend and frontend in separate terminals:

```bash
# Backend API server
cargo run --package gglib-cli -- web --api-only --port 9887 --base-port 9000

# Frontend dev server (proxies /api/* to backend)
npm run dev
# → http://localhost:5173
```

Or use the VS Code task **🚀 Run Dev (Frontend + Backend)** to launch both in parallel.

<details>
<summary><strong>Port configuration</strong></summary>

Set `VITE_GGLIB_WEB_PORT` in `.env` to change the API port (default `9887`). Both the Rust backend (via clap env) and Vite proxy read this value. The `VITE_` prefix is required for Vite. Port config only affects dev mode — production uses same-origin relative paths. Tauri uses dynamic port discovery.

</details>

<details>
<summary><strong>Production builds</strong></summary>

```bash
npm run build                                                      # → ./web_ui/
cargo run --package gglib-cli -- web --port 9887 --static-dir ./web_ui  # single-port serving
```

</details>

<details>
<summary><strong>Accelerated downloads (hf_xet)</strong></summary>

gglib bundles a managed Python helper for [hf_xet](https://github.com/huggingface/hf-xet) fast downloads. On first run (or after `make setup` / `gglib config check-deps`), it provisions a Miniconda environment under `<data_root>/.conda/gglib-hf-xet` and installs `huggingface_hub>=1.1.5` + `hf_xet>=0.6`. There is no legacy Rust HTTP fallback — if the helper is missing, `gglib model download` will fail until the environment is repaired.

</details>

<details>
<summary><strong>VS Code tasks</strong></summary>

- **🚀 Run Dev (Frontend + Backend)** — parallel launch
- **🧠 Run Backend Dev (API-only)** — backend only
- **🎨 Run Frontend Dev** — Vite dev server
- **🖥️ Run GUI (Dev)** — Tauri desktop in dev mode
- **🧪 Run All Tests** / **📎 Clippy** / **🎨 Format Code**

</details>

## Troubleshooting & Debugging

<details>
<summary><strong>Verbose logging</strong></summary>

Use `-v` / `--verbose` on any command to enable debug-level logging with file output:

```bash
gglib proxy -v
```

- **Log level:** `debug` (all targets). Override with `RUST_LOG=trace gglib proxy -v` for finer control.
- **Log files:** Written to logs in development builds, or `<data_root>/logs/` in release builds (e.g., `~/.local/share/gglib/logs/`). Files rotate daily.
- The `-v` flag is global — it works with any subcommand (`proxy`, `serve`, `question`, etc.).

</details>

## Documentation

**[View Full API Documentation →](https://mmogr.github.io/gglib)**

Auto-generated from source and updated with every release.

## Acknowledgments

### Core & CLI
![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)
![Clap](https://img.shields.io/badge/Clap-CLI-orange?style=flat-square)
![Tokio](https://img.shields.io/badge/Tokio-Async-blue?style=flat-square)
![SQLx](https://img.shields.io/badge/SQLx-Database-green?style=flat-square)
![Axum](https://img.shields.io/badge/Axum-Web_Server-darkred?style=flat-square)

### GUI & Frontend
![Tauri](https://img.shields.io/badge/tauri-%2324C8DB.svg?style=for-the-badge&logo=tauri&logoColor=white)
![React](https://img.shields.io/badge/react-%2320232a.svg?style=for-the-badge&logo=react&logoColor=%2361DAFB)
![Vite](https://img.shields.io/badge/vite-%23646CFF.svg?style=for-the-badge&logo=vite&logoColor=white)
![Tailwind CSS](https://img.shields.io/badge/tailwindcss-%2338B2AC.svg?style=for-the-badge&logo=tailwind-css&logoColor=white)
![Assistant UI](https://img.shields.io/badge/Assistant_UI-Chat_Interface-purple?style=flat-square)

### Integrations
![HuggingFace](https://img.shields.io/badge/%F0%9F%A4%97%20Hugging%20Face-Hub-yellow?style=flat-square)
![Llama.cpp](https://img.shields.io/badge/Llama.cpp-Inference-lightgrey?style=flat-square)

## LLM Optimization / Progressive Disclosure

When gglib is connected to an external MCP client (VS Code Copilot, OpenWebUI,
etc.), the proxy exposes a **Progressive Disclosure** interface instead of
dumping every tool schema up-front. This reduces context-window consumption by
90 %+ and eliminates timeout failures caused by 100 k-token schema payloads.

### How it works

`tools/list` always returns exactly **three meta-tools** regardless of how many
internal MCP servers are running:

| Meta-tool         | Purpose                                                        |
|-------------------|----------------------------------------------------------------|
| `search_tools`    | Keyword search over tool IDs and descriptions (≤ 30 results)  |
| `get_tool_schema` | Lazily fetch the full JSON input schema for one specific tool  |
| `invoke_tool`     | Execute a tool by its qualified ID with the given arguments    |

### Typical client flow

```
1. tools/list         → receives 3 meta-tool specs (~300 tokens)
2. search_tools       → discovers relevant tool IDs by keyword
3. get_tool_schema    → fetches only the schema it actually needs
4. invoke_tool        → executes with known arguments
```

### Qualified tool IDs

Internal tools are addressed with a double-underscore namespaced format:
`"<server_name>__<tool_name>"` (e.g. `"filesystem__read_file"`). This ID is
returned by `search_tools` and accepted by both `get_tool_schema` and
`invoke_tool`.

### Hard break — no legacy passthrough

Direct calls to raw tool names via `tools/call` return `METHOD_NOT_FOUND`.
All tool invocations must go through `invoke_tool`. There is no compatibility
shim for the pre-disclosure interface.

### Implementation

| Location | Role |
|---|---|
| [`gglib-core/src/domain/mcp/tool_index.rs`](crates/gglib-core/src/domain/mcp/tool_index.rs) | `ToolIndex` + `ToolSummary` pure-data types; `SEARCH_RESULTS_CAP = 30` |
| [`gglib-proxy/src/mcp/meta_tools.rs`](crates/gglib-proxy/src/mcp/meta_tools.rs) | Index construction from `McpService`; 3 static `McpToolSpec` definitions |
| [`gglib-proxy/src/mcp/handlers.rs`](crates/gglib-proxy/src/mcp/handlers.rs) | `handle_meta_tools_list` + `handle_meta_tools_call` dispatch |

## License

GGLib is open source under the [GNU Affero General Public License v3.0](LICENSE) (AGPL-3.0).

- **Personal and open source use** is free.
- **Commercial use** — building a product, running a SaaS, or embedding GGLib in paid software — requires a commercial license.

For commercial licensing enquiries, contact [@mmogr](https://github.com/mmogr) on GitHub.
