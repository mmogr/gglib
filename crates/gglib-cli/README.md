# gglib-cli

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-complexity.json)

<!-- module-docs:start -->

Command-line interface for gglib — the primary user-facing CLI application.

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
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐        │
│  │bootstrap.rs │     │presentation/│     │   utils/    │     │  error.rs   │        │
│  │  DI setup   │     │  Table fmt  │     │   Helpers   │     │   Errors    │        │
│  │  & wiring   │     │  & output   │     │             │     │             │        │
│  └─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘        │
│                                                                                     │
│  ┌───────────────────────────────────────────────────────────────────────────────┐  │
│  │                          *_commands.rs modules                                │  │
│  │   llama_commands │ config_commands │ assistant_ui_commands │ ...             │  │
│  └───────────────────────────────────────────────────────────────────────────────┘  │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`assistant_ui_commands.rs`](src/assistant_ui_commands.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-assistant_ui_commands-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-assistant_ui_commands-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-assistant_ui_commands-coverage.json) |
| [`benchmark_commands.rs`](src/benchmark_commands.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-benchmark_commands-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-benchmark_commands-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-benchmark_commands-coverage.json) |
| [`bootstrap.rs`](src/bootstrap.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-bootstrap-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-bootstrap-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-bootstrap-coverage.json) |
| [`commands.rs`](src/commands.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-commands-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-commands-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-commands-coverage.json) |
| [`config_commands.rs`](src/config_commands.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-config_commands-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-config_commands-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-config_commands-coverage.json) |
| [`dispatch.rs`](src/dispatch.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-dispatch-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-dispatch-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-dispatch-coverage.json) |
| [`error.rs`](src/error.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-error-coverage.json) |
| [`llama_commands.rs`](src/llama_commands.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-llama_commands-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-llama_commands-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-llama_commands-coverage.json) |
| [`mcp_commands.rs`](src/mcp_commands.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-mcp_commands-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-mcp_commands-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-mcp_commands-coverage.json) |
| [`model_commands.rs`](src/model_commands.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-model_commands-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-model_commands-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-model_commands-coverage.json) |
| [`parser.rs`](src/parser.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-parser-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-parser-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-parser-coverage.json) |
| [`shared_args.rs`](src/shared_args.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-shared_args-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-shared_args-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-shared_args-coverage.json) |
| [`handlers/`](src/handlers/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-handlers-coverage.json) |
| [`presentation/`](src/presentation/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-presentation-coverage.json) |
| [`utils/`](src/utils/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-utils-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-utils-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-utils-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`assistant_ui_commands.rs`** — Interactive assistant UI command definitions
- **`bootstrap.rs`** — Dependency injection and service wiring
- **`commands.rs`** — Command dispatch and routing
- **`config_commands.rs`** — Configuration management commands
- **`error.rs`** — CLI error types and handling
- **`llama_commands.rs`** — Llama server/chat command definitions
- **`parser.rs`** — Clap-based CLI argument parsing
- **`handlers/`** — Individual command handler implementations
- **`presentation/`** — Table formatting and output helpers
- **`utils/`** — CLI-specific utility functions

## Commands

| Command | Description |
|---------|-------------|
| `add <path>` | Add a GGUF model to the library |
| `list` | List all models with metadata |
| `inspect <id\|name>` | Show full details for a model (arch, quant, capabilities, inference defaults, GGUF metadata) |
| `remove <id>` | Remove a model from the library |
| `serve <id>` | Start llama-server for a model (respects per-model server_defaults from DB, overridable with `--ctx-size`) |
| `chat <id>` | Start interactive llama-cli chat |
| `chat <id> --continue <N>` | Resume a previous conversation by ID |
| `question <text>` | Ask a question (with optional piped context) |
| `question --agent <text>` | Agentic question with filesystem tools |
| `chat history` | List past conversations with message counts |
| `proxy` | Start the OpenAI-compatible proxy (context defaults to settings `default_context_size`) |
| `proxy dashboard [--host HOST] [--port PORT]` | Live terminal view of a running proxy's active connections, slot context usage, and request history |
| `download <repo>` | Download a model from HuggingFace |
| `search <query>` | Search HuggingFace Hub for models |
| `config settings show` | Show current configuration |
| `config default <id>` | Set/show/clear the default model |
| `council run "<goal>"` | Plan and execute a DAG task graph |
| `council list [--status]` | List past orchestrator runs |
| `council show <id>` | Show run details + event timeline |
| `council resume <id>` | Continue an interrupted run |
| `council rewind <id> --wave N` | Roll back to a previous wave and re-execute |
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

`gglib proxy dashboard` connects to an already-running proxy's `GET /v1/proxy/status/stream` SSE endpoint (see [`gglib-proxy`'s Proxy Dashboard docs](../gglib-proxy/README.md#proxy-dashboard) for the full `DashboardSnapshot` data contract) and redraws a live terminal view in place on every update — active connections (model, phase, prompt progress), per-slot context-usage gauges, and total request counts.

```bash
# In one terminal
gglib proxy

# In another
gglib proxy dashboard
gglib proxy dashboard --host 127.0.0.1 --port 8080
```

This is a simple redraw-in-place view (via `crossterm` cursor moves), not a full raw-mode TUI — consistent with this crate's existing terminal-handling conventions (see `handlers/model/download/interactive.rs`). Falls back to plain sequential prints on a non-TTY stdout. Press `Ctrl+C` to exit.

### Proxy Cache Management

| Command | Description |
|---|---|
| `gglib proxy start --cache --slot-dir <path>` | Start proxy with KV cache session persistence enabled |
| `gglib proxy cache-clear` | Clear KV cache for a session or all sessions on an already-running proxy |

Proxy cache-clear options:
| Flag | Description |
|---|---|
| `--host` | Proxy host (default: 127.0.0.1) |
| `-p`, `--port` | Proxy port (default: 8080) |
| `--session-id` | Optional session ID to target (without it, clears all sessions) |

Cache tuning flags (`gglib proxy start`):
| Flag | Description |
|---|---|
| `--cache-ram-mb <mb>` | Host-RAM prompt cache budget (`--cache-ram`). Omit to auto-size from RAM/weights/KV; `0` disables it. `GGLIB_DISABLE_CACHE_AUTOSIZE=1` skips auto-sizing entirely. Independent of `--cache`/`--slot-dir`. |
| `--cache-reuse <n>` | Min chunk size (tokens) for KV-shift cache reuse (`--cache-reuse`). Omit to disable; `GGLIB_DISABLE_CACHE_REUSE=1` suppresses it. |
| `--cache-disk-gb <gb>` | Byte budget for on-disk slot cache eviction. Omit to auto-size from free disk space; also settable via `GGLIB_CACHE_DISK_GB`. Ignored for sliding-window/hybrid/recurrent models, where the disk layer is disabled automatically — `GGLIB_FORCE_HYBRID_DISK_CACHE=1` re-enables it. |
| `--cache-type-k <type>` / `--cache-type-v <type>` | KV cache element type (`f32`, `f16`, `bf16`, `q8_0`, `q5_1`, `q5_0`, `q4_1`, `q4_0`). Defaults to `q8_0` on both axes; `GGLIB_DISABLE_KV_QUANT=1` falls back to `f16`. Quantizing V requires Flash Attention to be active. |

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

# Debug: see the constructed prompt
gglib q --verbose --file CODE.rs "Explain this"

# Cleaner output for scripting (no prompt echo, no timings)
gglib q -Q "What is 2+2?"

# Agentic mode: multi-step exploration with filesystem tools
gglib q --agent "How is error handling structured in this project?"

# Agentic mode with piped context
git diff | gglib q --agent "Review these changes for potential issues"
```

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

### Orchestrator (council)

The `council` command family runs a DAG-structured task graph using a local
llama-server.  The director LLM decomposes your goal into parallel worker
nodes; each node is executed with optional human-in-the-loop (HITL) gates.

```bash
# Basic run — director plans, workers execute, synthesis produces final answer
gglib council run "Audit the codebase for security issues"

# With a specific model and custom context window
gglib council run "Write a blog post on Rust async" --model llama3 --ctx-size 8192

# Approve the plan before execution starts
gglib council run "Refactor the auth module" --hitl plan

# Approve every node before it executes
gglib council run "Deploy staging" --hitl node

# Auto-reject approval gates after 30 seconds
gglib council run "Summarise docs" --hitl plan \
  --approval-timeout 30 --approval-timeout-action reject

# Auto-approve after 60 seconds (unattended CI run with a human fallback window)
gglib council run "Summarise docs" --hitl plan \
  --approval-timeout 60 --approval-timeout-action approve

# Machine-readable JSONL output — every CouncilEvent as one JSON line on stdout
# (requires --hitl none, which is the default)
gglib council run "Summarise logs" --json | jq 'select(.type == "council_complete")'

# List all runs
gglib council list

# Filter by status
gglib council list --status awaiting_approval

# Inspect a run's graph + full event timeline
gglib council show 01j2kxw3...

# Resume an interrupted or awaiting-approval run
gglib council resume 01j2kxw3...

# Rewind to wave 2 and re-execute from there
gglib council rewind 01j2kxw3... --wave 2

# Rewind and inject a steering note at the rewind point
gglib council rewind 01j2kxw3... --wave 2 --note "Focus on the authentication module"
```

#### HITL Approval Modes

| Mode | `--hitl` value | What triggers a prompt |
|------|---------------|------------------------|
| None (default) | `none` | Never — fully automatic |
| Plan gate | `plan` | Once, after the director produces the initial task graph |
| Node gate | `node` | Before each worker node starts |
| Tool gate | `tools` | Before each tool call inside any node |

At each prompt you can type:

| Input | Action |
|-------|--------|
| `y` / Enter | Approve and continue |
| `n` | Reject (prompts for an optional reason) |
| `e` | Edit the plan JSON in `$EDITOR` (Plan gates only) |
| *(timeout)* | Auto-resolve with `--approval-timeout-action` |

#### Live Steering

While a `council run`, `resume`, or `rewind` is executing you can type `/note`
lines directly into stdin.  The background input router forwards them into the
executor's `NoteQueue`; at each wave boundary the steering LLM converts each
queued note into a `GraphDiff` and applies it to the live task graph.

```text
/note focus only on the authentication module, skip the UI layer
/note add a node to write a summary report at the end
```

Any line that does **not** start with `/note ` is interpreted as a response to
the current HITL approval prompt.

#### JSON Output

Passing `--json` redirects every [`CouncilEvent`] to stdout as a
newline-delimited JSON object.  No ANSI colour, DAG trees, or progress output
reaches stdout — only valid JSONL.  All diagnostic text goes to stderr.

```bash
gglib council run "Summarise" --json 2>/dev/null | while IFS= read -r line; do
  echo "$line" | jq -r 'select(.type=="council_complete") | .answer'
done
```

Each line has a `"type"` field matching the [`CouncilEvent`] variant name in
`snake_case` (e.g. `node_started`, `node_complete`, `council_complete`).

## Usage

```bash
# Add a local model
gglib model add ~/models/llama-2-7b.Q4_K_M.gguf

# List all models
gglib model list

# Start a server
gglib serve 1 --port 8080

# Search HuggingFace
gglib model search "llama 3 GGUF"

# Download from HuggingFace
gglib model download TheBloke/Llama-2-7B-GGUF --quant Q4_K_M

# Download an Unsloth Dynamic ("UD-") quant -- distinct from the plain quant
# of the same suffix, e.g. "UD-Q6_K" vs "Q6_K"
gglib model download unsloth/Qwen3-Coder-Next-GGUF --quant UD-Q6_K
```

## Design Decisions

1. **Composition Root** — `bootstrap.rs` wires all dependencies (DI without framework)
2. **Clap Derive** — Uses clap's derive macros for type-safe argument parsing
3. **Handler Pattern** — Each command has a dedicated handler for testability
4. **No Event Emitter** — Uses `NoopEmitter` since CLI has direct stdout

<!-- module-docs:end -->
