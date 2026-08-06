# GGLib

[![CI](https://github.com/mmogr/gglib/actions/workflows/ci.yml/badge.svg)](https://github.com/mmogr/gglib/actions/workflows/ci.yml)
[![Coverage](https://github.com/mmogr/gglib/actions/workflows/coverage.yml/badge.svg)](https://github.com/mmogr/gglib/actions/workflows/coverage.yml)
[![Release](https://github.com/mmogr/gglib/actions/workflows/release.yml/badge.svg)](https://github.com/mmogr/gglib/actions/workflows/release.yml)
![Version](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/version.json)
[![License](https://img.shields.io/badge/license-AGPL--3.0-blue)](LICENSE)

**The local model runtime that makes llama.cpp behave like an API provider.**

Point your agentic coding client — Cline, Roo Code, Continue, Aider, Zed,
Copilot BYOK — at one OpenAI-compatible endpoint backed by your own GPU. GGLib
downloads GGUFs from HuggingFace, launches and swaps llama-server for you, and
runs an intelligent proxy between your client and the model that fixes the
problems raw llama-server (and Ollama) leave you with: tool calls arriving as
raw XML, context ballooning until the model loops, clients hardcoding sampling
you never chose.

## Start here

```bash
gglib up
```

One command from a clean machine to a working OpenAI-compatible endpoint. It
reads your hardware, installs llama.cpp if it is missing, recommends a model
that actually fits your VRAM (asking before downloading anything), starts the
proxy, sends a real request through it to prove the endpoint answers, and
prints the settings to paste into your client. Re-running it is safe; `--yes`
skips the confirmation, `--model <name>` loads a different installed model,
`--port` moves it off 8080. Everything beyond that is `gglib proxy`.

## Point your client at it

The endpoint is `http://127.0.0.1:8080/v1`. No API key is required on
loopback — enter any placeholder if your client insists. Use a model name from
`gglib model list` (shown below as `qwen3.6`).

**Cline / Roo Code** — Settings → API Provider: *OpenAI Compatible*, Base URL
`http://127.0.0.1:8080/v1`, API Key `gglib`, Model ID `qwen3.6`.

**Continue** — `config.yaml`:

```yaml
models:
  - name: qwen3.6 (local)
    provider: openai
    model: qwen3.6
    apiBase: http://127.0.0.1:8080/v1
    apiKey: gglib
```

**Aider**:

```bash
OPENAI_API_BASE=http://127.0.0.1:8080/v1 OPENAI_API_KEY=gglib aider --model openai/qwen3.6
```

**Zed** — `settings.json`:

```json
{
  "language_models": {
    "openai_compatible": {
      "gglib": {
        "api_url": "http://127.0.0.1:8080/v1",
        "available_models": [{ "name": "qwen3.6", "max_tokens": 32768 }]
      }
    }
  }
}
```

Requests naming `qwen3.6:coding` select a sampling profile on top of the same
model — see [Sampling resolution](docs/sampling.md#inference-profiles-modelprofile).

## What the proxy does to every request

Everything between the OpenAI request and llama-server is the product:

- **Dialect normalization** — a parse → normalize → re-encode pipeline turns
  model-specific markup (Qwen XML tool calls, bare `<think>` tags) into strict
  `chat.completion.chunk` events, selected per model by
  [`format:*` tags](docs/tags.md#format-dialect-tags).
- **Context defense** — oversized tool/assistant messages are compacted before
  forwarding, and prompts that would still blow the context window are rejected
  with a clean 400 instead of a looping model — see
  [History Truncation](crates/gglib-proxy/README.md#history-truncation).
- **Loop defense** — a conversation whose history already repeats the same
  tool-call batch or assistant response is aborted with a clean 400 *before*
  it costs a model swap or another generation, using the same detectors as
  the built-in agent loop; sessions fail fast and loud instead of silently
  burning your GPU — see
  [Loop & Stagnation Defence](crates/gglib-proxy/README.md#loop--stagnation-defence).
- **Sampling authority** — a 5-level resolution hierarchy decides every
  parameter; clients that hardcode `temperature: 0` don't silently win — see
  [Sampling resolution](docs/sampling.md).
- **KV cache tiering** — `q8_0` KV quantization, auto-sized host-RAM prompt
  cache, and opt-in disk slot persistence — see
  [KV cache tiering](docs/cache.md).
- **Capability detection** — GGUF metadata drives launch flags (`--jinja`,
  reasoning format, MTP speculative decoding, embeddings) with no per-model
  setup — see [Tags & capability detection](docs/tags.md).
- **Admission control** — requests for a model that is not loaded are queued
  and batched, so alternating chat and embeddings traffic costs one model swap
  per turn rather than one per request; a small auxiliary model can stay
  co-resident and never swap at all — see
  [admission](crates/gglib-runtime/src/process/admission/README.md).

## Dashboard

The proxy reports what it is doing: active connections, per-slot context
usage, cache reuse, and recent request history, at `GET /v1/proxy/status`
(JSON) and `/v1/proxy/status/stream` (SSE). `gglib proxy dashboard` renders it
live in the terminal; the web and desktop GUIs render the same snapshot as the
Proxy Dashboard modal.

<!-- placeholder: docs/assets/proxy-dashboard.png — GUI dashboard screenshot -->

## Interfaces

All interfaces share the same database and model directory.

| Interface | Launch | Details |
|-----------|--------|---------|
| **OpenAI proxy** | `gglib proxy` | [gglib-proxy](crates/gglib-proxy/README.md) — all models, hot-swapped on demand |
| **Pinned endpoint** | `gglib serve <id>` | Same proxy stack locked to one model |
| **Always-on proxy** | Desktop GUI tray | `--proxy-autostart`, `--close-to-tray`, `--start-at-login` |
| **CLI** | `gglib <command>` | [gglib-cli](crates/gglib-cli/README.md) — download, chat, `cat log \| gglib q "…"`, benchmark/tune |
| **Desktop GUI** | `gglib gui` | [gglib-tauri](crates/gglib-tauri/README.md) |
| **Web UI** | `gglib web` | [gglib-axum](crates/gglib-axum/README.md) — default `127.0.0.1:9887` |

## Security

Everything binds `127.0.0.1` by default. A bearer API key is optional on
loopback (`GGLIB_API_KEY`, `--api-key`, or settings) and minted automatically
the first time you bind anything else. A Host-header allowlist (DNS-rebinding
guard) and local-only CORS are always on. There is no multi-tenancy or rate
limiting — keep it off the public internet. Details:
[gglib-proxy](crates/gglib-proxy/README.md).

## Architecture

A Cargo workspace (~17 crates, Rust + a React front end) with CI-enforced
dependency direction: adapters (`gglib-cli`, `gglib-axum`, `gglib-tauri`) →
facades (`gglib-app-services`, `gglib-bootstrap`) → infrastructure (`gglib-db`,
`gglib-gguf`, `gglib-mcp`, `gglib-proxy`) → core (`gglib-core`, pure domain).
Only [`gglib-runtime`](crates/gglib-runtime/README.md) spawns llama-server;
only [`gglib-download`](crates/gglib-download/README.md) talks to HuggingFace.
Every crate's README carries its architecture diagram and module breakdown —
start at [`crates/`](crates/) or the
[generated API docs](https://mmogr.github.io/gglib), and see
[`CONTRIBUTING.md`](CONTRIBUTING.md) for conventions.

## Installation

Grab a build from the [Releases page](https://github.com/mmogr/gglib/releases)
(macOS Apple Silicon/Intel, Linux, Windows — on macOS run
`macos-install.command` to remove quarantine), or build from source:

```bash
git clone https://github.com/mmogr/gglib.git && cd gglib
make setup   # check deps → build frontend → install CLI → offer fast downloads + llama.cpp
```

Requires Rust (pinned via `rust-toolchain.toml`), Node.js 20.19+, and a C++
toolchain with CMake; `gglib config check-deps` prints your platform's exact
install commands. llama.cpp itself is managed by GGLib — no separate install.

### Faster downloads

Model downloads run over plain HTTP by default, which always works. If you have
Python, `gglib config fast-downloads enable` adds HuggingFace's `hf_xet`
transfer, which is noticeably quicker on large GGUFs. `make setup` and
`gglib up` both offer this, so you generally will not need to run it yourself.

GGLib builds and owns a Python environment for this under its own data
directory — it uses [uv](https://github.com/astral-sh/uv) when you have it and
`venv` otherwise, and it finds an interpreter from `PATH`, conda, pyenv or uv.
You do not have to activate anything, and nothing is installed outside that one
directory. `gglib config fast-downloads status` says what is there;
`disable` removes it and reverts to plain HTTP.

## Documentation

- [Sampling resolution](docs/sampling.md) — the 5-level hierarchy,
  temperature coupling, profiles, and `gglib model explain`
- [Tags & capability detection](docs/tags.md) — GGUF auto-detection taxonomy
  and retagging
- [KV cache tiering](docs/cache.md) — quantization, RAM auto-sizing, disk
  slot offloading
- [Full API documentation](https://mmogr.github.io/gglib) — generated from
  source on every release

## License

GGLib is open source under the
[GNU Affero General Public License v3.0](LICENSE) (AGPL-3.0). Personal and
open source use is free; commercial use — building a product, running a SaaS,
or embedding GGLib in paid software — requires a commercial license. Contact
[@mmogr](https://github.com/mmogr) on GitHub.
