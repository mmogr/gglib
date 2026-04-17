<!-- module-docs:start -->

# GGLib Command Reference

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-commands-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-commands-complexity.json)

This document provides a detailed reference for all available CLI commands.

For an overview of all interfaces (CLI, Desktop GUI, Web UI), see the main [README](../README.md#interfaces).

## Global Options

- `--help`: Show help information
- `--version`: Show version information
- `--models-dir <PATH>`: Override the models directory for the current invocation (CLI flag takes precedence over `.env`/defaults)

## Command Flow

```text
┌─────────────┐      ┌────────────────┐      ┌───────────────────┐
│  CLI Input  │ ───► │ Command Parser │ ───► │  Command Handler  │
│ (clap args) │      │ (main.rs)      │      │ (commands/*.rs)   │
└─────────────┘      └────────────────┘      └─────────┬─────────┘
                                                       │
                                                       ▼
                                             ┌───────────────────┐
                                             │  Shared Services  │
                                             │ (Database, Proxy) │
                                             └───────────────────┘
```

## Sub-modules

- **[Download Module](download/README.md)**: HuggingFace Hub integration and file operations.
- **[Llama Management](llama/README.md)**: Installation and building of llama.cpp.

## Internal Modules

- **llama_invocation**: Shared builder for constructing llama-cli/llama-server commands. Eliminates duplication between `chat` and `serve` commands by centralizing model resolution, context size handling, and flag construction.

## Commands

### Model Management

#### `model add <file_path>`
Add a GGUF model to the database.

**Example:**
```bash
gglib model add /path/to/model.gguf
```

#### `model list`
List all models in the database.

**Example:**
```bash
gglib model list
```

#### `model remove <identifier> [--force]`
Remove a model by name or ID.

**Options:**
- `--force`: Skip confirmation prompt

**Example:**
```bash
gglib model remove 1 --force
```

#### `model update <id> [OPTIONS]`
Update model metadata.

**Options:**
- `--name <NAME>`: Update model name
- `--param-count <COUNT>`: Update parameter count (in billions)
- `--architecture <ARCH>`: Update architecture
- `--quantization <QUANT>`: Update quantization type
- `--context-length <LENGTH>`: Update context length
- `--metadata <KEY=VALUE>`: Add or update metadata (can be used multiple times)
- `--remove-metadata <KEYS>`: Remove metadata keys (comma-separated)
- `--replace-metadata`: Replace all metadata instead of merging
- `--dry-run`: Preview changes without applying
- `--force`: Skip confirmation prompts

**Example:**
```bash
gglib model update 1 --name "Llama 2 7B" --metadata "use-case=chat"
```

### Model Operations

#### `serve <id> [OPTIONS]`
Serve a model with llama-server.

**Options:**
- `--ctx-size <SIZE>`, `-c`: Context size override (number or "max" for model metadata). Falls back to the global default from `gglib config settings`.
- `--mlock`: Enable memory lock
- `--jinja`: Force-enable Jinja template parsing for llama-server chat templates
- `--port <PORT>`, `-p`: Port to serve on (default: 8080)

**Example:**
```bash
gglib serve 1 --ctx-size max --mlock
```

#### `chat <identifier> [OPTIONS]`
Start an interactive chat session with `llama-cli` using any stored model.

**Options:**
- `--ctx-size <SIZE>`: Context size override (number or `max` for model metadata). Falls back to the global default from `gglib config settings`.
- `--mlock`: Enable memory locking
- `--chat-template <NAME>`: Override the template name baked into llama-cli
- `--chat-template-file <PATH>`: Provide a custom Jinja template path
- `--jinja`: Force-enable Jinja parsing for custom templates
- `--system-prompt <TEXT>` / `-s`: Supply a system prompt passed as `-sys`
- `--multiline-input`: Allow multi-line prompts without trailing `\`
- `--simple-io`: Switch to simplified IO for restricted terminals

**Example:**
```bash
gglib chat llama-3.1 --ctx-size max --system-prompt "You are a helpful assistant"
```

#### `proxy [OPTIONS]`
Start OpenAI-compatible proxy for automatic model swapping.

**Options:**
- `--host <HOST>`: Host to bind to (default: 127.0.0.1)
- `--port <PORT>`: Port to bind the proxy to (default: 8080)
- `--llama-port <PORT>`: Starting port for llama-server instances (default: 5500)
- `--default-context <SIZE>`: Default context size when not specified by client (default: 4096)

**Example:**
```bash
# Local access only
gglib proxy --port 8080

# LAN access (see LAN Server Mode documentation)
gglib proxy --host 0.0.0.0 --port 8080 --llama-port 5500
```

### HuggingFace Hub Integration

#### `model download <repo_id> [OPTIONS]`
Download a model from HuggingFace Hub.

**Options:**
- `--quantization <QUANT>`, `-q`: Specific quantization to download (e.g., "Q4_K_M", "F16")
- `--list-quants`: List available quantizations for the model
- `--skip-db`: Skip adding to database after download (models are registered by default)
- `--token <TOKEN>`: HuggingFace token for private models
- `--force`, `-f`: Skip confirmation prompt

**Example:**
```bash
# List available quantizations
gglib model download microsoft/DialoGPT-medium --list-quants

# Download specific quantization (auto-registered in database)
gglib model download microsoft/DialoGPT-medium --quantization Q4_K_M

# Download without registering in database
gglib model download microsoft/DialoGPT-medium -q Q4_K_M --skip-db
```

#### `model search <query> [OPTIONS]`
Search for GGUF models on HuggingFace Hub.

**Options:**
- `--limit <N>`: Maximum number of results (default: 10)
- `--sort <FIELD>`: Sort by "downloads", "created", "likes", or "updated" (default: downloads)
- `--gguf-only`: Only show models with GGUF files

**Example:**
```bash
gglib model search "llama 7b gguf" --limit 5 --sort downloads
```

#### `model browse <category> [OPTIONS]`
Browse popular GGUF models on HuggingFace Hub.

**Options:**
- `--limit <N>`: Maximum number of results (default: 20)
- `--size <SIZE>`: Filter by model size (e.g., "7B", "13B", "70B")

**Categories:**
- `popular`: Most popular models
- `recent`: Recently updated models
- `trending`: Trending models

**Example:**
```bash
gglib model browse popular --limit 10
gglib model browse recent --size 7B
```

#### `model check-updates [OPTIONS]`
Check for updates to downloaded models.

**Options:**
- `--model-id <ID>`: Check specific model by ID
- `--all`: Check all models

**Example:**
```bash
gglib model check-updates --all
```

#### `model upgrade <model_id> [--force]`
Update a model to the latest version from HuggingFace Hub.

**Options:**
- `--force`: Skip confirmation prompt

**Example:**
```bash
gglib model upgrade 1
```

### User Interfaces

#### `gui [OPTIONS]`
Launch the Tauri desktop GUI.

**Options:**
- `--dev`: Run in development mode with hot-reload (requires Node.js and npm)

**Example:**
```bash
# Launch desktop GUI
gglib gui

# Development mode (for contributors)
gglib gui --dev
```

For more details, see the [Desktop GUI documentation](../src-tauri/README.md).

#### `web [OPTIONS]`
Start the web-based GUI server.

**Options:**
- `--port <PORT>`: Port to serve the web GUI on (default: 9887)
- `--base-port <PORT>`: Base port for llama-server instances (default: 9000)
- `--api-only`: Serve API endpoints only (do not serve static UI assets)

**Example:**
```bash
# Start web server (accessible from LAN by default)
gglib web --port 9887

# API-only mode (useful when running the React dev server separately)
gglib web --api-only --port 9887

# Use different base port for model servers
gglib web --port 9887 --base-port 9000
```

The web server binds to `0.0.0.0` by default, making it accessible from your LAN. See [Interfaces](../README.md#interfaces) for details.

### llama.cpp Management

#### `llama install`
Install and build llama.cpp with automatic hardware detection.

**Example:**
```bash
gglib config llama install
```

#### `llama status`
Show llama.cpp installation status and build information.

**Example:**
```bash
gglib config llama status
```

#### `llama check-updates`
Check if a newer version of llama.cpp is available.

**Example:**
```bash
gglib config llama check-updates
```

#### `llama update`
Update llama.cpp to the latest version and rebuild.

**Example:**
```bash
gglib config llama update
```

#### `llama rebuild [OPTIONS]`
Rebuild llama.cpp with different acceleration options.

**Options:**
- `--cuda`: Force CUDA acceleration (NVIDIA GPUs)
- `--metal`: Force Metal acceleration (Apple Silicon)
- `--cpu-only`: Force CPU-only build

**Example:**
```bash
gglib config llama rebuild --cuda
```

#### `llama uninstall`
Remove llama.cpp installation.

**Example:**
```bash
gglib config llama uninstall
```

### assistant-ui Management

#### `assistant-ui install`
Install assistant-ui npm dependencies.

**Example:**
```bash
gglib config assistant-ui install
```

#### `assistant-ui status`
Show assistant-ui installation status.

**Example:**
```bash
gglib config assistant-ui status
```

#### `assistant-ui update`
Update assistant-ui dependencies.

**Example:**
```bash
gglib config assistant-ui update
```

### System

#### `paths`
Show resolved paths for all gglib directories (models, database, config, etc.).

**Example:**
```bash
gglib config paths
```

#### `check-deps`
Check system dependencies required for gglib.

**Example:**
```bash
gglib config check-deps
```

#### `config models-dir <action>`
Inspect or update the persistent models directory configuration used by downloads and serving commands.

**Actions:**
- `show` – print the resolved path along with its source (CLI flag/env/default)
- `prompt` – interactively ask for a new path, creating it if necessary and saving to `.env`
- `set <PATH>` – non-interactively persist a new path to `.env`

**Examples:**
```bash
# Review the current directory
gglib config models-dir show

# Walk through the interactive prompt (Enter keeps existing value)
gglib config models-dir prompt

# Force a specific path
gglib config models-dir set /fast-ssd/llama_models
```

#### `config settings <action>`
Manage application settings including download queue configuration, agent loop parameters, and display preferences.

**Actions:**
- `show` – display current settings
- `set` – update settings (see flags below)
- `reset` – reset all settings to defaults

**`set` flags:**
- `--default-context-size <N>` – default context size (512-1000000)
- `--proxy-port <PORT>` – OpenAI-compatible proxy port (≥ 1024)
- `--llama-base-port <PORT>` – llama-server base port (≥ 1024)
- `--max-download-queue-size <N>` – max download queue size (1-50)
- `--default-download-path <PATH>` – default model download directory
- `--max-tool-iterations <N>` – max agent loop iterations (1-50)
- `--max-stagnation-steps <N>` – max stagnation steps before abort
- `--show-memory-fit-indicators <BOOL>` – show memory fit in HF browser

**Examples:**
```bash
# View current settings
gglib config settings show

# Set max agent iterations to 40
gglib config settings set --max-tool-iterations 40

# Set max download queue size
gglib config settings set --max-download-queue-size 20

# Reset to defaults
gglib config settings reset
```

> Changing settings only affects application behavior; it does **not** affect existing downloads or models.


## See Also

- [Main README](../README.md) — Overview and getting started
- [Interfaces](../README.md#interfaces) — CLI, Desktop GUI, Web UI, and Proxy
- [Architecture](../README.md#architecture) — How GGLib is structured
- [gglib-cli crate](../../crates/gglib-cli/README.md) — Rust source for the CLI
- [Desktop GUI](../src-tauri/README.md) — Tauri app details

<!-- module-docs:end -->
