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

See the [Architecture Overview](../../README.md#architecture-overview) for the complete diagram.

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
| [`assistant_ui_commands.rs`](src/assistant_ui_commands) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-assistant_ui_commands-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-assistant_ui_commands-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-assistant_ui_commands-coverage.json) |
| [`bootstrap.rs`](src/bootstrap) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-bootstrap-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-bootstrap-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-bootstrap-coverage.json) |
| [`commands.rs`](src/commands) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-commands-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-commands-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-commands-coverage.json) |
| [`config_commands.rs`](src/config_commands) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-config_commands-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-config_commands-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-config_commands-coverage.json) |
| [`error.rs`](src/error) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-error-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-error-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-error-coverage.json) |
| [`llama_commands.rs`](src/llama_commands) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-llama_commands-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-llama_commands-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-llama_commands-coverage.json) |
| [`parser.rs`](src/parser) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-parser-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-parser-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-parser-coverage.json) |
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
| `remove <id>` | Remove a model from the library |
| `serve <id>` | Start llama-server for a model |
| `chat <id>` | Start interactive llama-cli chat |
| `question <text>` | Ask a question (with optional piped context) |
| `proxy` | Start the OpenAI-compatible proxy |
| `download <repo>` | Download a model from HuggingFace |
| `search <query>` | Search HuggingFace Hub for models |
| `config settings show` | Show current configuration |
| `config default <id>` | Set/show/clear the default model |

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
```

**Set a default model** to avoid using `--model` every time:

```bash
gglib config default 1
```

## Usage

```bash
# Add a local model
gglib add ~/models/llama-2-7b.Q4_K_M.gguf

# List all models
gglib list

# Start a server
gglib serve 1 --port 8080

# Search HuggingFace
gglib hf search "llama 3 GGUF"

# Download from HuggingFace
gglib hf download TheBloke/Llama-2-7B-GGUF --quant Q4_K_M
```

## Design Decisions

1. **Composition Root** — `bootstrap.rs` wires all dependencies (DI without framework)
2. **Clap Derive** — Uses clap's derive macros for type-safe argument parsing
3. **Handler Pattern** — Each command has a dedicated handler for testability
4. **No Event Emitter** — Uses `NoopEmitter` since CLI has direct stdout

<!-- module-docs:end -->
