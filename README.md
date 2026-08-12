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
- **Tool-call repair** — emitted tool calls are validated against the schema
  the client advertised; one that does not conform is re-issued with
  `tool_choice: "required"`, which is where llama.cpp installs its own
  schema-derived grammar. The client receives the corrected call and never
  sees the broken one — see [Tool-call repair](docs/tool-call-repair.md).
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

## Does that actually help? Measured

The claim above is that getting the sampling boundary right is worth something.
Here is that claim as a number, on one model.

`gglib benchmark agentic` runs a BFCL-style agentic suite twice against the
*same loaded model*: once with the pipeline bypassed — what a client pointed
straight at llama-server gets — and once through GGLib. Same weights, same
machine, same tasks, same scoring. The only difference is which request reached
llama-server.

**Qwen3.5-4B (Q8_0) @ 131072 ctx, 5 seeds per task, 45 runs per arm.** Two
independent runs, on the same seeds:

| axis | raw llama-server | through GGLib | delta |
|------|-----------------:|--------------:|------:|
| tool-call accuracy | 0.922 | **0.967** | +0.044 |
| task completion | 0.867 | **0.933** | +0.067 |
| loop avoidance | 0.286 | **0.333** | +0.048 |
| **composite** | **0.698** | **0.748** | **+0.050** |

Every axis moves the same way, in both runs. But the size of that movement is
**not yet resolved**, and the eval says so itself — which is the more useful
thing to know about it.

```bash
gglib benchmark agentic --model Qwen3.5-4B --seeds 12345,67890,11111,22222,33333
```

### What the eval says it cannot show

A benchmark that only reports its own wins is marketing. This one runs two extra
arms whose entire job is to catch it overclaiming, and on this model they caught
it:

- **A positive control** — the pipeline with sampling deliberately broken — that
  has to score far below the real arm. It did: **a 0.526 gap**. Without it, "the
  pipeline made no difference" and "this harness cannot detect a difference" are
  the same output. (The first version of this control *failed*: temperature 2.0
  alone scored **above** both real arms, because llama.cpp runs the truncation
  samplers before temperature, so a `top_k: 20` recipe absorbed it. It only
  works with truncation disabled outright.)
- **An A/A arm** — the raw arm re-run on a *different* seed set, nothing else
  changed. Whatever gap it opens is the eval's own drift. Here it opened
  **0.054** — meaning two runs of the identical raw configuration differ by more
  than the pipeline appeared to gain. At **0.9×** the drift, the +0.050 is
  **inside the noise floor**, and this eval declines to call it a result.

The first of the two runs measured **+0.082** on the same seeds. The second
measured +0.050 — and roughly half of *that* traces to a transport flake that
cost the raw arm three runs it would otherwise have scored (excluding them, the
delta is +0.027). A single number from a single run of this suite is worth very
little, which is exactly what the A/A arm exists to make visible.

**One finding did replicate.** `multi_turn_search_then_read` failed on **all ten**
raw seeds across both runs and passed **four of ten** through the pipeline. That
is a categorical difference rather than a shift in a rate, and it is the claim
this eval currently supports.

The honest summary: **directionally positive on every axis in two independent
runs, with one replicated categorical win, on an apparatus proven able to detect
a large change — and a magnitude that five seeds cannot yet separate from
drift.** Reproduce it on your own model rather than trusting the delta; that is
what the command is for, and the A/A arm will tell you the same thing about your
numbers that it told us about ours.

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
- [ADR 0001 — Compensation, Policy, Observation](docs/adr/0001-runtime-capability-tiers.md)
  — which GGLib behaviours exist to work around llama.cpp and which are
  GGLib's own job, and how the pinned build and runtime capability probe keep
  the two apart
- [ADR 0002 — Defer tool-call constraint to llama.cpp](docs/adr/0002-defer-tool-call-constraint-to-llama-cpp.md)
  — the measurement behind that deferral: native schema conformance per model,
  why it does not generalise, and the discovery that GGLib's dialect parser is
  bypassed entirely
- [ADR 0003 — Defer sampler defaults to llama.cpp](docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md)
  — six of the seven values GGLib force-wrote into every request were measured
  to be llama.cpp's own defaults, and the launch flags that set them affect no
  request that goes through the pipeline
- [ADR 0004 — Observe the sampling boundary](docs/adr/0004-observe-the-sampling-boundary.md)
  — reading back what llama-server says it sampled with, what that can and
  cannot catch, why an observation organ has to be able to fail, and the A/B
  measurement behind the table above — including the positive control that
  failed on its first attempt and what that cost to notice
- [ADR 0005 — The autonomous closed loop](docs/adr/0005-autonomous-closed-loop-and-reactive-grammar.md)
  — the defect ledger, the idle-time scheduler, and the drift-gated apply
  that lets the system tune its own models; why reactive tool-call repair is
  the permanent mechanism after the lazy-grammar probe hit the endpoint's
  wall
- [Tool-call repair](docs/tool-call-repair.md) — validating tool arguments
  against the advertised schema, and re-issuing with `tool_choice: "required"`
  when they do not conform
- [Full API documentation](https://mmogr.github.io/gglib) — generated from
  source on every release

## License

GGLib is open source under the
[GNU Affero General Public License v3.0](LICENSE) (AGPL-3.0). Personal and
open source use is free; commercial use — building a product, running a SaaS,
or embedding GGLib in paid software — requires a commercial license. Contact
[@mmogr](https://github.com/mmogr) on GitHub.
