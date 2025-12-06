# GGLib - Rust GGUF Library Management Tool

[![CI](https://github.com/mmogr/gglib/actions/workflows/ci.yml/badge.svg)](https://github.com/mmogr/gglib/actions/workflows/ci.yml)
[![Coverage](https://github.com/mmogr/gglib/actions/workflows/coverage.yml/badge.svg)](https://github.com/mmogr/gglib/actions/workflows/coverage.yml)
[![Docs](https://github.com/mmogr/gglib/actions/workflows/docs.yml/badge.svg)](https://github.com/mmogr/gglib/actions/workflows/docs.yml)
[![Release](https://github.com/mmogr/gglib/actions/workflows/release.yml/badge.svg)](https://github.com/mmogr/gglib/actions/workflows/release.yml)
![Version](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/version.json)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
![Boundaries](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/boundary.json)

![Rust Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/tests.json)
![Rust Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/coverage.json)
![TS Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-tests.json)
![TS Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-coverage.json)

<!-- crate-docs:start -->

A multi-interface platform for managing and serving GGUF (GPT-Generated Unified Format) model files, providing CLI, desktop GUI, and web-based access.

## Overview

gglib provides a simple interface to catalog, organize, and serve GGUF models locally. It maintains a SQLite database of your models with their metadata, making it easy to find and work with specific models.

## Features

- **Add models**: Import GGUF files and extract metadata automatically
- **List models**: View all models with their properties in a clean table format
- **Update models**: Edit model metadata including name, parameters, and custom fields
- **Remove models**: Clean removal of models from your database
- **Serve models**: Start llama-server with automatic context size detection
- **Chat via CLI**: Launch llama-cli directly for quick terminal chat sessions
- **OpenAI-compatible Proxy**: Automatic model swapping with OpenAI API compatibility
- **HuggingFace Hub Integration**: Download models directly from HuggingFace Hub
- **Fast-path Downloads**: Managed Python helper (hf_xet) via Miniconda for multi-gigabyte transfers
- **Search & Browse**: Discover GGUF models on HuggingFace with search and browse commands
- **Quantization Support**: Intelligent detection and handling of various quantization formats
- **Rich metadata**: Support for complex metadata operations and Unicode content
- **Reasoning Model Support**: Auto-detection and streaming of thinking/reasoning phases with collapsible UI for models like DeepSeek R1, Qwen3, and QwQ

## Architecture Overview

GGLib follows a layered architecture where multiple frontends share common backend services:

```text
┌─────────────────────────────────────────────────────────────────┐
│                         Frontends                               │
│  ┌──────────┐      ┌──────────────┐      ┌──────────────────┐   │
│  │   CLI    │      │ Desktop GUI  │      │   Web UI/API     │   │
│  │ Commands │      │   (Tauri)    │      │    (Axum)        │   │
│  └────┬─────┘      └──────┬───────┘      └────────┬─────────┘   │
│       │                   │                       │             │
│       │                   └───────────┬───────────┘             │
│       │                               │                         │
└───────┼───────────────────────────────┼─────────────────────────┘
        │                               ▼
        │              ┌──────────────────────────────────────┐
        │              │         GuiBackend                   │
        │              │  • Model operations                  │
        │              │  • Server/process management         │
        │              │  • Proxy management                  │
        │              │  • Chat history                      │
        │              └──────────────────────────────────────┘
        │                               │
        ▼                               ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Shared Services                              │
│  ┌────────────┐  ┌────────────┐  ┌──────────────┐               │
│  │  Database  │  │  Process   │  │  Proxy       │               │
│  │  (SQLite)  │  │  Manager   │  │  Service     │               │
│  └────────────┘  └────────────┘  └──────────────┘               │
└─────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────┐
│              External Processes                                 │
│  ┌──────────────────┐          ┌──────────────────────┐         │
│  │  llama-server    │          │ OpenAI-compatible    │         │
│  │   instances      │          │       Proxy          │         │
│  └──────────────────┘          └──────────────────────┘         │
└─────────────────────────────────────────────────────────────────┘
```

**Core Modules:**

- **[`models`](src/models/README.md)** – GGUF model metadata, GUI/API DTOs, and data structures
- **[`download`](src/download/README.md)** – Download manager, queue, and HuggingFace file resolution
- **[`services`](src/services/README.md)** – Business logic layer including:
  - `database`: SQLite operations for model metadata and chat history
  - `gui_backend`: Unified backend service used by Tauri and Web GUI
  - `process_manager`: Manages llama-server processes and health checks
  - `chat_history`: Stores and retrieves conversation history
- **[`commands`](src/commands/README.md)** – CLI command handlers and web API endpoints
- **[`utils`](src/utils/README.md)** – Lower-level helpers for process management, path resolution, and parsing
- **[`proxy`](src/proxy/README.md)** – OpenAI-compatible HTTP proxy with automatic model swapping

**How Frontends Connect:**

- **CLI** → Directly calls database and service layer functions from command handlers in `src/commands/`
- **Desktop GUI (Tauri)** → Tauri commands call `GuiBackend` methods, which coordinate services. Spawns embedded HTTP server for React frontend
- **Web UI (Axum)** → HTTP handlers in `src/commands/gui_web/handlers.rs` call `GuiBackend` methods

The `GuiBackend` service provides a unified interface for GUI operations, while the CLI commands access services directly for efficiency.

### Workspace Crates (Phase 5)

The codebase is organized as a Cargo workspace with compile-time enforced boundaries:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Core Layer                                     │
│  ┌─────────────────────────────────┐    ┌─────────────────────────────────┐ │
│  │          gglib-core             │◄───│           gglib-db              │ │
│  │   Pure domain types & ports     │    │   SQLite repository impls       │ │
│  │   (no infra dependencies)       │    │   (core + sqlx)                 │ │
│  └─────────────────────────────────┘    └─────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                    ┌───────────────┼───────────────┐
                    ▼               ▼               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                            Adapter Layer                                    │
│  ┌───────────────────┐  ┌───────────────────┐  ┌───────────────────────┐   │
│  │    gglib-cli      │  │    gglib-axum     │  │     gglib-tauri       │   │
│  │  CLI interface    │  │   HTTP API        │  │    Desktop GUI        │   │
│  │  (core+db+clap)   │  │  (core+db+axum)   │  │   (core+db+tauri)     │   │
│  └───────────────────┘  └───────────────────┘  └───────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Crate Health:**

| Crate | Tests | Coverage | Description |
|-------|-------|----------|-------------|
| [`gglib-core`](crates/gglib-core) | ![core](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tests.json) | ![cov](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-coverage.json) | Domain types, ports, events |
| [`gglib-db`](crates/gglib-db) | ![db](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-tests.json) | ![cov](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-coverage.json) | SQLite repositories |
| [`gglib-cli`](crates/gglib-cli) | ![cli](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-tests.json) | ![cov](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-coverage.json) | Command-line interface |
| [`gglib-axum`](crates/gglib-axum) | ![axum](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-tests.json) | ![cov](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-coverage.json) | HTTP API server |
| [`gglib-tauri`](crates/gglib-tauri) | ![tauri](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-tests.json) | ![cov](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-coverage.json) | Desktop GUI backend |

<!-- crate-docs:end -->

## Installation

GGLib provides a streamlined installation process using the included Makefile for the best developer and user experience.

### Pre-built Releases

Download the latest release for your platform from the [Releases page](https://github.com/mmogr/gglib/releases).

#### macOS

1. Download the macOS release tarball for your architecture:
   - `gglib-gui-*-aarch64-apple-darwin.tar.gz` for Apple Silicon (M1/M2/M3)
   - `gglib-gui-*-x86_64-apple-darwin.tar.gz` for Intel Macs
2. Extract the archive: `tar -xzf gglib-gui-*.tar.gz`
3. Double-click `macos-install.command` (or run `./macos-install.command` in Terminal)
4. The script will remove the quarantine attribute and optionally install to `/Applications`

> **Note:** macOS marks downloaded apps as "damaged" because they are not code-signed. The install script fixes this automatically by removing the quarantine attribute.

#### Windows

Download and extract the Windows release (`gglib-gui-*-x86_64-pc-windows-msvc.zip`), then run `gglib-gui.exe`.

#### Linux

Download and extract the Linux release (`gglib-gui-*-x86_64-unknown-linux-gnu.tar.gz`), then run the `gglib-gui` binary.

### Quick Install (From Source)

The recommended way to install GGLib is using the Makefile:

```bash
# Clone the repository
git clone https://github.com/mmogr/gglib.git
cd gglib

# Full setup: check dependencies, build, and install
make setup
```

The `make setup` command will:
- Check for required system dependencies (Rust, Node.js, build tools)
- Provision the managed Miniconda environment used by the hf_xet fast download helper
- Build the web UI frontend
- Build and install the CLI tool to `~/.cargo/bin/`
- Optionally install llama.cpp with automatic GPU detection

`make setup` (and `gglib check-deps`) exits with an error if Python/Miniconda is missing or the fast download helper cannot be prepared. Run those commands first on new machines so large downloads succeed without manual intervention.

**Note:** When installed via `make setup`, GGLib operates in **Developer Mode**. It will keep its database (`gglib.db`), configuration (`.env`), and `llama.cpp` binaries inside your repository folder. This keeps your development environment self-contained. (Downloaded models are still stored in `~/.local/share/llama_models` by default).

### Makefile Utilities

The Makefile provides several convenient targets:

**Installation & Setup:**
- `make setup` - Full setup (dependencies + build + install + llama.cpp)
- `make install` - Build and install CLI to `~/.cargo/bin/`
- `make uninstall` - **Full Cleanup**: Removes binary, system data, database, and cleans the repo. (Preserves downloaded models).

**Building:**
- `make build` - Build release binary
- `make build-dev` - Build debug binary
- `make build-gui` - Build web UI frontend
- `make build-tauri` - Build desktop GUI application
- `make build-all` - Build everything (CLI + web UI)

**Development:**
- `make test` - Run all tests
- `make check` - Check code without building
- `make fmt` - Format code
- `make lint` - Run clippy linter
- `make doc` - Generate and open documentation

**llama.cpp Management:**
- `make llama-install-auto` - Install llama.cpp with auto GPU detection
- `make llama-status` - Show llama.cpp installation status
- `make llama-update` - Update llama.cpp to latest version

**Running:**
- `make run-gui` - Launch desktop GUI
- `make run-web` - Start web server
- `make run-serve` - Run model server
- `make run-proxy` - Run OpenAI-compatible proxy

**Cleaning:**
- `make clean` - Remove build artifacts
- `make clean-gui` - Remove web UI build
- `make clean-llama` - Remove llama.cpp installation
- `make clean-db` - Remove database files

### Manual Installation (Alternative)

If you prefer to use Cargo directly:

```bash
# Install from source
cargo install --path .

# Or install from crates.io (when published)
cargo install gglib
```

### Prerequisites

- **Rust** 1.70 or later - [Install Rust](https://rustup.rs/)
- **Python 3 via Miniconda** (required for the hf_xet fast download helper) - [Install Miniconda](https://docs.conda.io/en/latest/miniconda.html)
- **Node.js** 18+ (for web UI) - [Install Node.js](https://nodejs.org/)
- **SQLite** 3.x
- **Build tools** (platform-specific):
  - **macOS**: `xcode-select --install` and `brew install cmake`
  - **Ubuntu/Debian**: `sudo apt install build-essential cmake git`
  - **Fedora/RHEL**: `sudo dnf install gcc-c++ cmake git`
  - **Arch Linux**: `sudo pacman -S base-devel cmake git`
  - **Windows**: Visual Studio 2022 with C++ tools, CMake, and Git

**Note:** llama.cpp is managed by GGLib itself. You don't need to install it separately!

### Configuring the models directory

Downloaded GGUF files now live in a user-configurable directory (default: `~/.local/share/llama_models`). You can change it at any time using whichever interface is most convenient:

- **During `make setup`** – the installer now prompts for the location and accepts Enter to keep the default.
- **Environment file** – copy `.env.example` to `.env` and set `GGLIB_MODELS_DIR=/absolute/path`, or edit the value via `gglib config models-dir set` (see below). All helpers expand `~/` and will create the directory when needed.
- **CLI overrides** – use `gglib --models-dir /tmp/models download …` for a one-off run, or persist the change with `gglib config models-dir prompt|set <path>`.
- **GUI/Web settings** – click the gear icon in the header to open the Settings modal, review the current directory, and update it without touching the CLI.

The precedence order is: CLI `--models-dir` flag → `GGLIB_MODELS_DIR` from the environment/.env → default path. All download code paths rely on the shared helper in `src/utils/paths.rs`, so whichever option you choose applies consistently across CLI, desktop, web, and background tasks.

Changing the directory only affects future downloads and servers—it does **not** move any GGUF files you already downloaded. If you want your existing models in the new location, move them manually and then rescan/add them as needed.

### Accelerated downloads via hf_xet

Large GGUFs can saturate a single HTTP stream, so gglib bundles a managed Python helper that talks to Hugging Face's [hf_xet](https://github.com/huggingface/hf-xet) service. Fast downloads are now the only path—if the helper is missing or broken, commands like `gglib download` will fail until you repair the environment.

- On the first run (or after `gglib check-deps`/`make setup`), gglib provisions a Miniconda environment under `<data_root>/.conda/gglib-hf-xet` and installs `huggingface_hub>=1.1.5` plus `hf_xet>=0.6`. A tiny helper script lives in `<data_root>/.gglib-runtime/python/hf_xet_downloader.py`.
- The helper emits newline-delimited JSON so both the CLI and GUI can keep their existing progress indicators.
- Missing Python packages are treated as errors. Run `gglib check-deps` or `make setup` to reinstall the managed environment; there is no legacy Rust HTTP fallback anymore.

Requirements: install Miniconda (or another Python 3 distribution with `venv` support) and ensure enough disk space to populate the per-user `.conda/gglib-hf-xet` directory. The helper respects the same Hugging Face tokens you pass to `gglib download` and does not change how downloads are recorded in the SQLite database.

## Interfaces & Modes

GGLib provides three complementary interfaces for interacting with GGUF models. All interfaces share the same backend implementation (database, services, process manager, and proxy), ensuring consistent behavior and data across all modes.

### CLI (Command-Line Interface)

Command-line interface for GGUF model management and service control.

**Capabilities:**
- Model operations: `gglib add`, `gglib list`, `gglib remove`, `gglib update`
- HuggingFace Hub integration: `gglib download`, `gglib search`, `gglib browse`
- Direct terminal chat: `gglib chat <id|name>`
- Server management: `gglib serve`, `gglib proxy`
- Interface launchers: `gglib gui`, `gglib web`
- llama.cpp management: `gglib llama install`, `gglib llama update`

### Desktop GUI (Tauri)

Cross-platform desktop application built with Tauri (Rust backend) and React frontend.

**Technical details:**
- Launched via `gglib gui` command
- Uses shared `GuiBackend` service for all operations
- Spawns embedded HTTP API server on localhost for frontend-backend communication
- React frontend communicates via standard HTTP endpoints (`/api/models`, `/api/servers`, etc.)
- Same API routes as standalone web server
- Shares business logic, data model, and process management with other interfaces

### Web UI + HTTP API

Browser-based interface backed by Axum HTTP server.

**Technical details:**
- Started via `gglib web` command
- Default binding: `0.0.0.0:9887` (LAN accessible)
- API routes: `/api/models`, `/api/servers`, `/api/chat`, `/api/proxy/...`
- React frontend (in `src/`) uses same HTTP endpoints as Tauri embedded server
- Services layer (`TauriService`, `ChatService`) detects environment and uses either Tauri IPC (`invoke`) or HTTP calls

## Web UI Server

The web server provides browser-based access to GGLib's functionality via an Axum HTTP API.

**Configuration:**
```bash
# Start web server (binds to 0.0.0.0:9887 by default)
gglib web --port 9887 --base-port 9000
```

**Access:**
- Local: `http://localhost:9887/`
- Network: `http://<HOST_IP>:9887/`
- API: `http://<HOST_IP>:9887/api`

**Parameters:**
- `--port`: Web server port (default: 9887)
- `--base-port`: Starting port for llama-server instances (default: 9000)

## OpenAI-Compatible Proxy

The proxy provides OpenAI API-compatible endpoints for model inference. This enables GGLib to work seamlessly with OpenWebUI and other tools that support the OpenAI API format.

**Configuration:**
```bash
# Start proxy (binds to 127.0.0.1:8080 by default)
gglib proxy --host 0.0.0.0 --port 8080 --llama-port 5500
```

**Endpoints:**
- Base URL: `http://<HOST_IP>:8080/v1/`
- `/v1/models` - List available models
- `/v1/chat/completions` - Chat completions
- `/health` - Health check

**Parameters:**
- `--host`: Bind address (default: 127.0.0.1, use 0.0.0.0 for network access)
- `--port`: Proxy port (default: 8080)
- `--llama-port`: Starting port for llama-server instances (default: 5500)
- `--default-context`: Default context size (default: 4096)

**Features:**
- Automatic model server management (start/stop on demand)
- Request routing to appropriate llama-server instances
- Full OpenAI SDK compatibility
- Seamless integration with OpenWebUI

**OpenWebUI Integration:**

To use GGLib with OpenWebUI:

1. Start the proxy with network access: `gglib proxy --host 0.0.0.0 --port 8080`
2. In OpenWebUI settings, configure:
   - API Base URL: `http://localhost:8080/v1`
   - API Key: (any value, not validated)
3. Select models from the dropdown - GGLib will automatically start the appropriate llama-server

For details on developing the desktop GUI, see [src-tauri/README.md](src-tauri/README.md).

The desktop build embeds the same REST API that powers `gglib gui web`. It binds to `http://localhost:8888` by default; make sure that port is free before running `gglib gui`. You can pick a different port by setting `GGLIB_GUI_API_PORT=<port>` before launching. The desktop UI now detects the port at runtime, so no frontend rebuild is required.

### Testing

```bash
export OPENAI_BASE_URL="http://localhost:8080/v1"
export OPENAI_API_KEY="dummy"
```

Then use any OpenAI-compatible client library as you normally would.

### Security Considerations

**Network Binding:**
- Web server binds to `0.0.0.0` by default (network accessible)
- Proxy binds to `127.0.0.1` by default (local only)
- Use `--host 0.0.0.0` for network access to proxy

**Authentication:**
- No authentication required by default
- Designed for trusted network environments

**Recommendations:**
- Use firewall rules to restrict access to trusted IP ranges
- Only expose on private networks (192.168.x.x, 10.x.x.x, 172.16-31.x.x)
- Use VPN for access from outside local network
- Do not port-forward to public internet without additional authentication

## 📖 Documentation

**[📚 View Full API Documentation →](https://mmogr.github.io/gglib)**

The complete API documentation is automatically generated from the source code and hosted on GitHub Pages. It includes:

- 🔍 **Detailed API reference** for all public functions and types
- 💡 **Usage examples** and code snippets
- 🏗️ **Architecture overview** of the codebase
- 🔧 **Developer guides** for contributing

The documentation is automatically updated with every release, so you'll always have access to the latest information.

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
![Assistant UI](https://img.shields.io/badge/Assistant_UI-Chat_Interface-purple?style=flat-square)

### Integrations
![HuggingFace](https://img.shields.io/badge/%F0%9F%A4%97%20Hugging%20Face-Hub-yellow?style=flat-square)
![Llama.cpp](https://img.shields.io/badge/Llama.cpp-Inference-lightgrey?style=flat-square)

### Health Status

**Rust Workspace:**
![core](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-core-tests.json&style=flat-square)
![db](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-db-tests.json&style=flat-square)
![cli](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-tests.json&style=flat-square)
![axum](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-axum-tests.json&style=flat-square)
![tauri](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-tauri-tests.json&style=flat-square)
![boundaries](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/boundary.json&style=flat-square)

**Web UI:**
![TS Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-tests.json&style=flat-square)
![TS Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-coverage.json&style=flat-square)
