# The gglib command line

One page per command, each showing the command running. `gglib <command>
--help` lists every flag: clap generates it from the same source the binary
parses, so it cannot drift, and these pages do not repeat it beyond the
groups several commands share. They cover what `--help` cannot: what a
command is for, what it prints, which rules it follows, and how the commands
fit together.

Start at [`gglib up`](up.md). Everything else is here for when you want to
choose.

## Everything goes through the daemon

One process owns llama-server: the **gglib daemon**. It holds an exclusive
lock on the data directory, and every other surface, this command line, the
desktop app, the web page, a paired device, is a client of it. No command
spawns llama-server itself.

```text
  gglib up ─┐
  gglib q  ─┤                    ┌──────────────────┐
  gglib    ─┼──► gglib daemon ──►│   llama-server   │
  chat      │    (owns the       │  (1–2 resident)  │
  Desktop  ─┤     runtime,       └──────────────────┘
  app       │     holds the lock)          ▲
  Web page ─┘           │                  │
                        └── admission queue ┘
```

That is why a model started from the desktop app is the model the proxy
serves, why the endpoint keeps running after you close the terminal that
started it, and why a command that needs the runtime starts a daemon when
none is up. A *foreign* process on the daemon port is never fought: it is
reported as an error.

## Global options

| Option | Effect |
|---|---|
| `--remote` | Do this on the machine this one is paired with; `gglib remote status` names it ([Remote access](../remote.md)) |
| `-v`, `--verbose` | Debug-level logging, and a log file under `logs/` |
| `-h`, `--help`; `-V`, `--version` | `--version` ends with the commit the binary was built from |

## The commands

Grouped as `gglib --help` groups them.

| | Command | For |
|---|---|---|
| **Getting started** | [`up`](up.md) | Nothing → a working endpoint, in one command |
| **Management** | [`model`](model.md) | The library: add, list, download, verify, upgrade, inspect, explain |
| | [`config`](config.md) | Settings, sampling profiles, the engines, dependencies, paths |
| | [`mcp`](mcp.md) | Tool servers for `chat`, `q` and the chat page |
| **Inference** | [`serve <model>`](proxy.md#serve) | The endpoint pinned to one model |
| | [`chat <model>`](chat.md#chat) | An interactive session with tools |
| | [`q`](chat.md#q) | One question, pipe-friendly (`question` is the long spelling) |
| | [`run`](chat.md#run) | A reply the daemon finishes while you do something else |
| | [`image`](image.md) | Draw a picture with this machine's image model |
| | [`attachment save`](chat.md#attachment-save) | Write an image a chat stored to a file |
| **Interfaces** | [`gui`](proxy.md#gui-and-web) / [`web`](proxy.md#gui-and-web) | The desktop app and the web page |
| | [`proxy`](proxy.md) | The OpenAI-compatible endpoint, every model, swapped on demand |
| | [`daemon`](proxy.md#daemon) | Inspect or stop the process that owns the runtime |
| | [`remote`](../remote.md) | This machine's endpoint on your other machines and your phone |
| **Measurement** | [`benchmark`](benchmark.md) | Compare outputs, measure throughput, tune sampling, measure the pipeline |
| **Shell** | `completions <shell>` | A completion script for bash, zsh, fish, elvish or powershell |

## Flags several commands share

`proxy`, `serve`, `chat` and `q` are built from the same flag groups. Learn
them once; the last column says which commands take each.

| Group | Flags | What to know | On |
|---|---|---|---|
| **Context** | `--ctx-size` / `-c`, `--mlock` | `--mlock` locks the weights in memory. `--ctx-size` is a number, or `max` for the model's trained window. Left out, the first of these that is set wins: the model's own default, your global default, the size fitted to the model and this machine, then 4096. [`model explain`](model.md#explain) shows the result. `proxy` takes `--default-context` instead, for every model it serves. | `serve`, `chat`, `q` |
| **Sampling** | `--temperature`, `--top-p`, `--top-k`, `--min-p`, `--max-tokens`, the penalties, DRY, dynatemp, `--top-n-sigma`, `--reasoning-effort`, `--reasoning-budget-tokens` | One layer of a five-level hierarchy: request → profile → this model → your global default → the model's own recipe. [Sampling resolution](../sampling.md) has the rules; `model explain` shows which layer won. | `proxy`, `serve`, `chat`, `q` |
| **Profile** | `--profile <name>` | A named sampling profile ([config profile](config.md#profile)) for this run, where one model is named. | `serve`, `chat`, `q` |
| **Cache** | `--cache`, `--slot-dir`, `--cache-disk-gb` | The on-disk slot cache. KV quantization and the host-RAM prompt cache are sized by the daemon at every launch, not per run: [KV cache tiering](../cache.md). | `proxy`, `serve` |
| **Access** | `--host`, `--api-key`, `--allowed-host` | A loopback bind with no key given and none stored runs unauthenticated. Binding elsewhere mints a key and stores it, as `gglib remote enable` does among others, and a stored key is demanded on loopback too; `gglib config settings reveal proxy-api-key` prints it. Every host a client will reach the endpoint by is named with `--allowed-host` (the DNS-rebinding guard). | `proxy`, `serve` |
| **MTP** | `--mtp-draft-n-max`, `--mtp-draft-p-min` | Speculative decoding, on by itself for `mtp`-tagged models; `--mtp-draft-n-max 0` forces it off. | `serve` |
| **Upstream** | `--port` | A llama-server already running here on this port, asked directly, with no daemon started. | `chat`, `q` |
| **Tool limits** | `--tool-timeout-ms`, `--max-parallel` | How long one tool call may take, and how many may run at once ([chat](chat.md#guardrails-for-tool-calls)). | `chat`, `q` |
| **Retry** | `--no-retry` | Transient upstream failures on the completion path are retried unless this says not to; `GGLIB_LLM_RETRY_MAX_ATTEMPTS` and `GGLIB_LLM_RETRY_DEADLINE_SECS` in the environment tune the budget (4 attempts inside 60 s). | `chat` |

`up` takes none of them: it is the defaults, on purpose.

## How these pages were made

Every output block on these pages was recorded by running the command, with
[`docs/screenshots/snippets.py`](../screenshots/snippets.py), against a gglib
with its own data folder and models; the pages change when the output does. The machine's user is shown as `mmogr`.

## See also

- [The README](../../README.md): what gglib is, and how to point a client at it
- [Client configuration](../clients.md) · [Sampling](../sampling.md) ·
  [Tags](../tags.md) · [KV cache](../cache.md) · [Remote access](../remote.md)
- [gglib-cli](../../crates/gglib-cli/README.md): the Rust behind these commands
