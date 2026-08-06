<!-- module-docs:start -->

# GGLib Command Reference

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-commands-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-commands-complexity.json)

What each command is for, and how they fit together. For the exhaustive flag
list on any command, run `gglib <command> --help` — clap generates that from the
same source the binary parses, so it cannot drift. This document covers what
`--help` cannot: why a command exists, how the shared flag groups compose, and
which resolution layer wins.

Start at [`gglib up`](#getting-started). Everything beyond it is
[`gglib proxy`](#serving-an-endpoint).

## Everything goes through the daemon

One process owns llama-server: the **gglib daemon**. It holds an exclusive lock
on the data directory, and every other surface — this CLI, the desktop GUI, the
web UI — is a client of it. No CLI command spawns llama-server itself.

```text
  gglib up ─┐
  gglib q  ─┤                    ┌──────────────────┐
  gglib    ─┼──► gglib daemon ──►│   llama-server   │
  chat      │    (owns the       │  (1–2 resident)  │
  Desktop  ─┤     runtime,       └──────────────────┘
  GUI       │     holds the lock)          ▲
  Web UI   ─┘           │                  │
                        └── admission queue ┘
```

That is why a model started from the GUI is the same model the proxy serves, why
the endpoint keeps running after you close the desktop window, and why commands
that used to configure a per-run process (`--llama-port`, the `--cache-*` flags)
now warn that the daemon does not apply them per run.

Commands that need the runtime start a daemon automatically if one is not
already up. A *foreign* process on the daemon port is never fought — it is
reported as an error.

## Global options

| Option | Effect |
|---|---|
| `--models-dir <PATH>` | Override the models directory for this invocation (wins over `.env` and defaults) |
| `-v`, `--verbose` | Debug-level logging plus file output to `logs/` |
| `--help`, `--version` | Standard |

## Command map

| Command | For |
|---|---|
| [`up`](#getting-started) | Nothing → working endpoint, in one command |
| [`proxy`](#serving-an-endpoint) | OpenAI-compatible endpoint, all models, swapped on demand |
| [`serve <id>`](#serving-an-endpoint) | Same stack pinned to one model |
| [`daemon`](#serving-an-endpoint) | Inspect or stop the process that owns the runtime |
| [`model`](#models) | Add, list, download, verify, retag, inspect, explain |
| [`chat`](#chat-and-ask) | Interactive tool-calling session |
| [`q`](#chat-and-ask) | One-shot question, pipe-friendly (alias of `question`) |
| [`benchmark`](#benchmark) | Compare outputs, measure throughput, tune sampling |
| [`mcp`](#mcp-tool-servers) | Manage MCP tool servers |
| [`config`](#configuration) | Settings, profiles, llama.cpp, dependencies, paths |
| [`gui`](#interfaces) / [`web`](#interfaces) | Desktop app / web dashboard |
| `completions <shell>` | Print a completion script for bash, zsh, fish, elvish, or powershell |

## Getting started

### `up`

Detects your hardware, installs llama.cpp if missing, recommends and downloads a
model that fits your VRAM (asking first), starts the proxy, sends a real request
through it to prove the endpoint answers, and prints the settings to paste into
your client.

Safe to re-run — anything already present is skipped, so a second run is just
"start the proxy". Deliberately unconfigurable beyond `--yes`, `--model` and
`--port`: it binds loopback with gglib's defaults. Reach for `gglib proxy` when
you want to choose the host, the upstream port, or sampling and cache behaviour.

```bash
gglib up                      # ask before downloading
gglib up --yes                # take the recommendation
gglib up --model qwen3.6 --port 8081
```

## Serving an endpoint

### `proxy`

Serves `/v1` chat completions, `/v1/embeddings`, `/v1/models` and `/mcp` (MCP
Streamable HTTP) from one port. When a request names a model that is not
resident, the proxy queues it, launches that model, and applies flags derived
from its capability tags — `mtp` → speculative decoding, `reasoning` →
`--reasoning-format`, `agent` → `--jinja`. Every surface uses the same config
builder, so a model behaves identically however it was started.

Sampling flags here act as proxy-wide defaults: per-request and per-model values
still win. Subcommands: `dashboard` (live terminal view), `cache-clear`, `stop`.

### `serve <id>`

The same proxy stack pinned to one model — requests naming any other are refused
rather than swapped. That is what clients which cannot switch models via
`/v1/models` need, VS Code Copilot's BYOK endpoint among them. `--port` is the
endpoint; `--llama-port` is the upstream behind it.

### `daemon`

`run` (foreground, `--share-lan` to expose on the network), `status`, `stop`.
Stopping the daemon stops every llama-server it owns.

## Shared flag groups

Several commands flatten the same argument groups. Learning them once covers
`up`, `proxy`, `serve`, `chat` and `q`.

| Group | Flags cover | Notes |
|---|---|---|
| **Context** | `--ctx-size` / `-c` | A number, or `max` to take the model's own metadata. Falls back to the global default. |
| **Sampling** | temperature, top-p, penalties, … | One layer of a five-level hierarchy — see [Sampling resolution](../../docs/sampling.md). Use [`model explain`](#models) to see which layer won. |
| **Cache** | `--cache-ram`, `--cache-reuse`, KV cache types | KV quantization and host-RAM prompt cache — see [KV cache tiering](../../docs/cache.md). |
| **Access** | `--host`, `--api-key`, `--allowed-host` | Loopback needs no key. Binding elsewhere mints one and prints it, and every host a client will reach the endpoint by must be named with `--allowed-host` (DNS-rebinding guard). |
| **MTP** | `--mtp-draft-n-max`, `--mtp-draft-p-min` | Auto-enabled for `mtp`-tagged models; set `n-max` to `0` to force it off. |
| **Retry** | retry budget, `--no-retry` | Transient upstream failures on the completion path. |

## Models

`gglib model <subcommand>`:

**Library** — `add`, `list`, `remove`, `update`. `list` sorts by added, name,
params, or benchmark speed.

**HuggingFace** — `download`, `search`, `browse`, `check-updates`, `upgrade`.

Downloads route through the shared queue the GUI uses. In a TTY the terminal
becomes a live monitor: **[a]** adds another model while one is in flight;
**[q]** / `Esc` / `Ctrl-C` drains once, then force-quits on a second press. The
bar shows the lifecycle phase — `Downloading` → `Finalizing` → `Registering` →
`Completed` / `Failed` / `Cancelled`. Non-TTY environments fall back to a plain
monitor automatically. Set `HF_TOKEN` in the environment for private repos.

Transfers run natively over HTTP, resumable and checksum-verified; no Python is
required. See the [Download Module](download/README.md).

**Integrity** — `verify` (SHA-256), `repair` (re-download failed shards).

**Metadata and capability** — `capabilities`, `inspect`, `retag`, `explain`.

`retag` re-derives auto-tags from persisted GGUF metadata; run it after
upgrading gglib to backfill newly-introduced tags such as the `format:*` dialect
family. It is additive by default and never touches user-curated tags; `--full`
rebuilds the auto namespace only. See [Tags & capability
detection](../../docs/tags.md).

`explain` prints every resolved sampling parameter alongside the layer that
supplied it, using the same resolver the live path uses — so it cannot describe
a hierarchy different from the one that runs.

## Chat and ask

### `chat <identifier>`

Interactive session with tool access (filesystem plus any configured MCP
servers). `--no-tools` for plain chat. Conversations persist: `--continue <id>`
resumes one, and `gglib chat history` lists them.

Local models need guardrails to finish a tool-calling task, so the loop carries
iteration limits (`--max-iterations`), a tool allowlist (`--tools`, evaluated
once at session start), parallelism and timeout caps, and a dual-threshold loop
guard — `--observation-tool` / `--max-observation-steps` let read-only tools
repeat more often than mutating ones before loop detection fires.

### `q` / `question`

One-shot, built for pipes. `{}` in the question is replaced by piped or `--file`
input. `-Q` / `--quiet` strips tool progress and reasoning tokens for scripting.

```bash
gglib q "What is Rust?"
cat file.rs | gglib q "Explain this code"
gglib q --file README.md "Summarize this project"
```

## Benchmark

`compare` runs one prompt through N models sequentially and shows outputs
side-by-side. `perf` measures prompt-processing and generation throughput via
llama-bench. `list`, `show` and `model` read past runs.

`tune` sweeps sampling parameters against an agentic tool-calling suite, scoring
both tool-call accuracy and resistance to loops and stagnation, and can write the
winning settings straight back to the model's inference defaults.

## MCP tool servers

`list`, `add`, `remove`, `start`, `stop`, `enable`, `disable`, `tools`, `test`.

Servers are `stdio` (a process) or `sse` (HTTP). The `lifecycle` policy decides
when gglib spawns one: `eager` at host init, `lazy` on first tool use (default),
`manual` never.

## Configuration

`gglib config <subcommand>`:

- **`settings`** — context size, ports, download queue size, agent loop limits,
  display preferences. `show` / `set` / `reset`.
- **`profile`** — named sampling profiles, selectable per request as
  `<model>:<profile>`. Only the flags you pass are set; the rest fall through to
  the model's own defaults.
- **`default`** — view or set the default model.
- **`models-dir`** — `show` (with its source), `prompt` (interactive), or
  `set <PATH>`.
- **`llama`** — install, status, check-updates, update, rebuild, uninstall.
  gglib manages llama.cpp itself; see [Llama Management](llama/README.md).
- **`assistant-ui`** — install, status, update.
- **`check-deps`** — report what is missing and print your platform's exact
  install commands. Reporting only; it installs nothing.
- **`fast-downloads`** — `status`, `enable`, `disable`, `prompt` for the
  optional `hf_xet` accelerator. Downloads work without it, over native HTTP.
  `enable` builds a Python environment gglib owns under its own data
  directory, using `uv` when available; `--python <path>` names the
  interpreter to build it from. `disable` removes it.
- **`paths`** — resolved locations for models, database, config and logs.

Changing settings affects future behaviour only — it does not alter existing
downloads or models.

## Interfaces

`gui` launches the desktop app (`--dev` for hot-reload, contributors only).
`web` ensures the daemon is up and prints the dashboard URL; the daemon serves
UI and API from one loopback port. To expose it on the network, run
`gglib daemon run --share-lan` in the foreground.

## See Also

- [Main README](../../README.md) — what GGLib is and how to point a client at it
- [Sampling resolution](../../docs/sampling.md) · [Tags](../../docs/tags.md) · [KV cache](../../docs/cache.md)
- [gglib-cli crate](../../crates/gglib-cli/README.md) — the Rust source behind these commands
- [Desktop GUI](../../src-tauri/README.md)

<!-- module-docs:end -->
