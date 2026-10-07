# gglib-cli

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-complexity.json)

<!-- module-docs:start -->

Command-line interface for gglib — the primary user-facing CLI application.

`gglib up` lives here: one command from a clean machine to a working endpoint.
Every command is a thin client of the daemon that owns the runtime — this crate
renders and asks, it never spawns llama-server itself.

## Architecture

This crate is in the **Adapter Layer** — it wires together all infrastructure crates and exposes them via CLI commands.

```text
                              ┌──────────────────┐
                              │    gglib-cli     │
                              │  CLI interface   │
                              └────────┬─────────┘
                                       │
         ┌─────────────┬───────────────┼───────────────┬─────────────┐
         ▼             ▼               ▼               ▼             ▼
┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
│  gglib-db   │ │gglib-download│ │gglib-runtime│ │  gglib-hf   │ │  gglib-mcp  │
│   SQLite    │ │  Downloads  │ │   Servers   │ │  HF client  │ │ MCP servers │
└─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘
         │             │               │               │             │
         └─────────────┴───────────────┴───────────────┴─────────────┘
                                       │
                                       ▼
                              ┌──────────────────┐
                              │    gglib-core    │
                              │   (all ports)    │
                              └──────────────────┘
```

See the [Architecture Overview](../../README.md#architecture) for the complete diagram.

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                gglib-cli                                            │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐        │
│  │   main.rs   │ ──► │  parser.rs  │ ──► │ commands.rs │ ──► │  handlers/  │        │
│  │  Entry pt   │     │   clap CLI  │     │  Dispatch   │     │  Command    │        │
│  │             │     │   parsing   │     │   table     │     │  handlers   │        │
│  └─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘        │
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐                            │
│  │bootstrap.rs │     │presentation/│     │   utils/    │                            │
│  │  DI setup   │     │  Table fmt  │     │   Helpers   │                            │
│  │  & wiring   │     │  & output   │     │             │                            │
│  └─────────────┘     └─────────────┘     └─────────────┘                            │
│                                                                                     │
│  ┌───────────────────────────────────────────────────────────────────────────────┐  │
│  │                          *_commands.rs modules                                │  │
│  │   llama_commands │ config_commands │ model_commands │ ...                   │  │
│  └───────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

**Module Descriptions:**
- **`bootstrap.rs`** — Dependency injection and service wiring
- **`commands.rs`** — Command dispatch and routing
- **`config_commands.rs`** — Configuration management commands
- **`llama_commands.rs`** — Llama server/chat command definitions
- **`parser.rs`** — Clap-based CLI argument parsing
- **`handlers/`** — Individual command handler implementations
- **`presentation/`** — Table formatting and output helpers
- **`utils/`** — CLI-specific utility functions

## Commands

| Command | Description |
|---------|-------------|
| `add <path>` | Add a GGUF model to the library |
| `list` | List all models with metadata; while paired, end with one line on the paired machine (`--remote` lists its models, and which of them read images) |
| `inspect <id\|name>` | Show full details for a model (arch, quant, capabilities, inference defaults, GGUF metadata), and the port it is being served on when it is; `--remote` reads the paired machine's |
| `explain <id\|name> [--profile <name>]` | Show every resolved inference parameter and which layer of the sampling hierarchy supplied it |
| `remove <id\|name>` | Remove a model from the library; refused while the model is being served under this data root (`--force` skips the confirmation only) |
| `serve <id\|name>` | Start llama-server for a model (respects per-model `server_defaults` from DB, overridable with `--ctx-size`) |
| `chat <id\|name>` | Start interactive llama-cli chat |
| `chat --continue <N>` | Resume a previous conversation by ID, on the machine it ran on |
| `chat … --thinking on\|off` | Switch the chat's Thinking on or off, as the chat page's switch does: the choice runs the session and the chat remembers it, on a new chat or with `--continue` |
| `question <text>` | Ask a question (with optional piped context) |
| `question <text>` | Ask a question; filesystem tools are on unless `--no-tools` |
| `question --image <path> <text>`, `chat <id\|name> --image <path>` | Attach a PNG or JPEG to the turn (repeatable); see [Images](#images) |
| `chat history` | List past conversations with message counts |
| `proxy` | Start the OpenAI-compatible proxy (context comes from settings `default_context_size`, or is sized per launch when unset) |
| `proxy dashboard [--host HOST] [--port PORT]` | Live terminal view of a running proxy's active connections, slot context usage, prompt-cache health and reuse, and request history |
| `proxy trips [--since DAYS]` | The loop guard's log from this machine's database, daemon or not: requests scanned per UTC day, model, version and mode, and the ones it noted or refused |
| `download <repo>` | Download a model from `HuggingFace` |
| `search <query>` | Search `HuggingFace` Hub for models |
| `config settings show` | Show current configuration |
| `config default <id\|name>` | Set/show/clear the default model |
| `config profile list` | List named sampling profiles |
| `config profile show <name>` | Show one profile's parameters |
| `config profile set <name> [flags]` | Create or update a profile (only the flags passed are set; `--unset <param>` clears one) |
| `config profile rm <name>` | Delete a profile |
| `config profile install-templates` | Install the starter profiles: three sampling (coding, chat, creative) and six reasoning rungs (minimal…max) |
| `verify <id\|name>` | Verify model integrity via SHA256 hash comparison |
| `repair <id\|name>` | Re-download corrupt shards for a model |
| `completions <shell>` | Print a shell completion script to stdout |

### Shell Completions

Enable tab completion for your shell by piping the generated script into place:

| Shell | One-time setup |
|-------|----------------|
| fish | `gglib completions fish > ~/.config/fish/completions/gglib.fish` |
| bash | `gglib completions bash > ~/.bash_completion` |
| zsh | `gglib completions zsh > ~/.zsh/_gglib` |
| elvish | `gglib completions elvish > ~/.config/elvish/lib/gglib.elv` |
| powershell | `gglib completions powershell >> $PROFILE` |

Supported shells: `bash`, `zsh`, `fish`, `elvish`, `powershell`.

### Proxy Dashboard

`gglib proxy dashboard` connects to an already-running proxy's `GET /v1/proxy/status/stream` SSE endpoint (see [`gglib-proxy`'s Proxy Dashboard docs](../gglib-proxy/README.md#proxy-dashboard) for the full `DashboardSnapshot` data contract) and redraws a live terminal view in place on every update — active connections (model, phase, prompt progress), per-slot context-usage gauges, prompt-cache health and measured reuse, and total request counts.

The prompt-cache section shows any warnings the proxy raised (a cramped RAM budget, or a disk layer disabled because the model's attention keeps only part of the token history) followed by measured reuse: prompt tokens served from cache versus processed, in total and for the most recent request. These are raw counts from the upstream's own `usage` reporting — there is no estimated "time saved", since reuse is measured exactly but what it saved depends on a prefill that never ran.

```bash
# In one terminal
gglib proxy

# In another
gglib proxy dashboard
gglib proxy dashboard --host 127.0.0.1 --port 8123
```

This is a simple redraw-in-place view (via `crossterm` cursor moves), not a full raw-mode TUI — consistent with this crate's existing terminal-handling conventions (see `handlers/model/download/interactive.rs`). Falls back to plain sequential prints on a non-TTY stdout. Press `Ctrl+C` to exit.

### Proxy Cache Management

| Command | Description |
|---|---|
| `gglib proxy --cache --slot-dir <path>` | Start proxy with KV cache session persistence enabled |
| `gglib proxy cache-clear` | Clear KV cache for a session or all sessions on an already-running proxy |

Proxy cache-clear options:
| Flag | Description |
|---|---|
| `--host` | Proxy host (default: 127.0.0.1) |
| `-p`, `--port` | Proxy port (default: the stored `proxy_port` setting) |
| `--session-id` | Optional session ID to target (without it, clears all sessions) |

Cache flags (`gglib proxy`, `gglib serve`):
| Flag | Description |
|---|---|
| `--cache` | Persist llama-server slot state to disk per session |
| `--slot-dir <path>` | Where those slot files live (defaults to `<app-data-dir>/slots`) |
| `--cache-disk-gb <gb>` | Byte budget for on-disk slot cache eviction. Omit to auto-size from free disk space; also settable via `GGLIB_CACHE_DISK_GB`. Ignored for sliding-window/hybrid/recurrent models, where the disk layer is disabled automatically — `GGLIB_FORCE_HYBRID_DISK_CACHE=1` re-enables it. |

The host-RAM prompt cache and KV element types are **not** per-run flags. The
daemon builds its `ProcessManager` once at startup, so there is nothing for a
per-invocation value to attach to; both are auto-sized per launch, and the
environment switches (`GGLIB_DISABLE_CACHE_AUTOSIZE`, `GGLIB_DISABLE_KV_QUANT`,
`GGLIB_DISABLE_CACHE_REUSE`) are read by the daemon process. See
[KV cache tiering](../../docs/cache.md).

### Question Command

The `question` command (alias: `q`) supports piped input or file context:

```bash
# Simple question (uses default model)
gglib q "What is the capital of France?"

# Read context from a file
gglib q --file README.md "Summarize this project"

# Pipe context into the question
cat README.md | gglib q "Summarize this file"

# Use {} placeholder for inline substitution
echo "Paris, London, Tokyo" | gglib q "List these cities: {}"

# Pipe command output
git diff | gglib q "Explain these changes"

# Attach an image (PNG or JPEG); repeat --image for more than one
gglib q --image shot.png "What is the error on this screen?"
gglib q --image before.png --image after.png "What changed?"

# Debug: see the constructed prompt (-v is the global debug-logging flag)
gglib q --show-prompt --file CODE.rs "Explain this"

# Cleaner output for scripting (no prompt echo, no timings)
gglib q -Q "What is 2+2?"

# Agentic mode: multi-step exploration with filesystem tools
gglib q "How is error handling structured in this project?"

# Agentic mode with piped context
git diff | gglib q "Review these changes for potential issues"
```

### Images

`--image <PATH>` attaches a PNG or JPEG to a turn, on `gglib q` and on
`gglib chat` (where it goes with the first message). It is long-form only and
repeatable; the images are sent in the order given. Inside a chat,
`/image <path>` attaches a file to the next message; the path is the rest of
the line, as typed (a `~` is not expanded).

```bash
gglib q --image shot.png "What is the error on this screen?"
gglib chat qwen3.8 --image diagram.png
gglib q --remote --image shot.png "What is the error?"   # the paired machine's model

# In the chat REPL
You: /image shots/error.png
  image error.png: 2560x1440, ~3600 tokens
You: what does the stack trace say?
```

- **Stored once, sent by id.** Each file is stored in this machine's database
  exactly as it is on disk, under the SHA-256 of its bytes, and the message
  names it by that id. The bytes become an `image_url` only in the request to
  the model. The CLI does not resize: a file is at most 8 MiB, and the images
  of one request at most 16 MiB together.
- **The receipt.** One stderr line per image says its name, its size in pixels
  and the prompt tokens it is estimated to cost. `gglib q -Q` suppresses it.
- **A file that cannot be attached** — missing, not a PNG or a JPEG by its
  first bytes, or over the cap — is an error naming the path, before any model
  is looked up or loaded. `gglib q` still requires a question.
- **A model that cannot see** is refused by name before anything is loaded:
  a model of this machine needs a projector linked
  (`gglib model update <model> --projector <path>`). The whole chat counts, so
  resuming a chat that holds an image on a model without one is refused too.
  With `--port`, the server on that port is asked through its `/props`
  (`modalities.vision`); one that says `false` is refused the same way, and
  one that does not answer, or does not say, is sent the image. With
  `--remote`, the paired machine's proxy answers.
- **On resume**, `gglib chat --continue <N>` shows an image of the last turn as
  a marker, `[image 2560x1440]`, after its text.

### Rendering Modes

The CLI auto-detects its output target and selects a rendering mode:

| Stdout target | `--quiet` | Mode     | Behaviour |
|---------------|-----------|----------|-----------|
| TTY           | no        | **Rich** | Buffers tokens → renders Markdown via [termimad](https://crates.io/crates/termimad) |
| TTY           | yes       | **Raw**  | Streams tokens directly, suppresses stderr |
| Pipe / file   | either    | **Raw**  | Streams tokens directly (no ANSI escapes) |

In **Rich** mode a spinner runs on stderr while the response is being received,
so the terminal never appears frozen. Once the full response arrives it is
rendered in one pass with a custom Markdown skin tuned for dark terminals:

- **Headings** — bold cyan
- **Inline code** — yellow
- **Code blocks** — green, indented 2 columns
- **Body text** — default-dark palette (high contrast grays)

The skin is built by `presentation::style::get_markdown_skin()` and uses
`term_text()` for terminal-width-aware line wrapping.

In **Raw** mode each token is printed to stdout as it arrives — identical to the
pre-Rich behaviour. This keeps piped output clean and machine-parseable:

```bash
# Pipe-safe: only the raw answer reaches the file
gglib q "Summarize this" > answer.txt

# Quiet mode: suppresses tool progress, reasoning, iteration counts
gglib q -Q "What is 2+2?" | pbcopy
```

### Thinking Block

When a reasoning model emits chain-of-thought tokens (via `ReasoningDelta`
events or inline `<think>` tags), the CLI wraps them in a visually distinct
block on stderr:

```text
  ╭─ 💭 Thinking ───────────────────────────╮
  (dim) The user is asking about … (dim)
```

The thinking block uses a **top border only** — no side or bottom borders.
This is deliberate: SSE chunks arrive at arbitrary byte boundaries, so
line-prefixing would cause visual corruption. Instead the body is rendered
in `DIM` mode (`\x1b[2m`) and reset (`\x1b[0m`) when the thinking phase
ends.

Thinking visuals are suppressed when `--quiet` is set or stderr is not a TTY.

### Inline Thinking Reclassification

Reasoning content is split from response text upstream by
`gglib-core::normalize::NormalizingStream` (or by llama-server's
`--reasoning-format auto`). The CLI consumes pre-classified
`AgentEvent::ReasoningDelta` and routes reasoning to stderr while answer
text reaches stdout. This works regardless of rendering mode.

**Set a default model** to avoid using `--model` every time:

```bash
gglib config default 1
```

## Usage

```bash
# Add a local model
gglib model add ~/models/llama-2-7b.Q4_K_M.gguf

# Re-import one already in the library, refreshing its derived metadata
# (capabilities, quantization, context length, expert counts, tags, spec)
gglib model add --reimport ~/models/llama-2-7b.Q4_K_M.gguf

# List all models
gglib model list

# Give a model image input by linking it to a projector (an mmproj GGUF),
# and take it away again. A download from a repository that has projectors
# fetches one and links it; `model download <repo> --list-quants` shows which.
gglib model update 1 --projector ~/models/mmproj-F16.gguf
gglib model update 1 --no-projector

# Pin one model to an OpenAI-compatible endpoint (proxy stack, dashboard included)
gglib serve 1 --port 8123

# Same, with KV cache session persistence on disk
gglib serve 1 --cache --slot-dir ~/.gglib/slots

# By name, with a default inference profile for bare-model requests
gglib serve qwen3.6 --profile chat

# Search HuggingFace
gglib model search "llama 3 GGUF"

# Download from HuggingFace
gglib model download TheBloke/Llama-2-7B-GGUF --quantization Q4_K_M

# Download an Unsloth Dynamic ("UD-") quant -- distinct from the plain quant
# of the same suffix, e.g. "UD-Q6_K" vs "Q6_K"
gglib model download unsloth/Qwen3-Coder-Next-GGUF --quantization UD-Q6_K
```

## Design Decisions

1. **Composition Root** — `bootstrap.rs` wires all dependencies (DI without framework)
2. **Clap Derive** — Uses clap's derive macros for type-safe argument parsing
3. **Handler Pattern** — Each command has a dedicated handler for testability
4. **No Event Emitter** — Download progress is read as queue snapshots and drawn on stderr by the download board (`handlers/model/download/board.rs`) through `CliConsole`, with no broadcast bus

<!-- module-docs:end -->
