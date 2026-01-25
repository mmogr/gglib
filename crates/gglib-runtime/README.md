# gglib-runtime

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-complexity.json)

Process management and system probes for gglib — manages llama.cpp server instances and proxy routing.

## Architecture

This crate is in the **Infrastructure Layer** — it manages external processes and provides system information.

```text
gglib-core (ports)          gglib-runtime                   External
┌──────────────────┐        ┌──────────────────┐        ┌──────────────────┐
│  ProcessManager  │◄───────│  LlamaRunner     │───────►│  llama-server    │
│  SystemProbe     │        │  ProxyManager    │        │  llama-cli       │
│  HealthCheck     │        │  HealthChecker   │        └──────────────────┘
└──────────────────┘        └──────────────────┘
                                     │
                                     ▼
                            ┌──────────────────┐
                            │   System APIs    │
                            │  (GPU, memory)   │
                            └──────────────────┘
```

See the [Architecture Overview](../../README.md#architecture-overview) for the complete diagram.

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                              gglib-runtime                                          │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐        │
│  │  runner.rs  │     │   llama/    │     │   proxy/    │     │  process/   │        │
│  │ High-level  │ ──► │ llama-server│     │  OpenAI API │     │  Lifecycle  │        │
│  │   facade    │     │  llama-cli  │     │   routing   │     │  management │        │
│  └─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘        │
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐                            │
│  │  health.rs  │     │   system/   │     │assistant_ui/│                            │
│  │ Health check│     │ GPU, memory │     │  Terminal   │                            │
│  │  endpoints  │     │   probes    │     │  chat UI    │                            │
│  └─────────────┘     └─────────────┘     └─────────────┘                            │
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐                                                │
│  │ command.rs  │     │process_core │                                                │
│  │ Cmd builder │     │ Core types  │                                                │
│  └─────────────┘     └─────────────┘                                                │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
                                          │
                                depends on
                                          ▼
                              ┌───────────────────┐
                              │    gglib-core     │
                              │  (port traits)    │
                              └───────────────────┘
```

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`command.rs`](src/command) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-command-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-command-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-command-coverage.json) |
| [`health_monitor.rs`](src/health_monitor) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health_monitor-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health_monitor-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health_monitor-coverage.json) |
| [`health.rs`](src/health) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health-coverage.json) |
| [`process_core.rs`](src/process_core) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process_core-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process_core-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process_core-coverage.json) |
| [`runner.rs`](src/runner) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-runner-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-runner-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-runner-coverage.json) |
| [`assistant_ui/`](src/assistant_ui/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-assistant_ui-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-assistant_ui-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-assistant_ui-coverage.json) |
| [`llama/`](src/llama/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-llama-coverage.json) |
| [`pidfile/`](src/pidfile/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-pidfile-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-pidfile-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-pidfile-coverage.json) |
| [`ports_impl/`](src/ports_impl/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-ports_impl-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-ports_impl-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-ports_impl-coverage.json) |
| [`process/`](src/process/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process-coverage.json) |
| [`proxy/`](src/proxy/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-proxy-coverage.json) |
| [`system/`](src/system/) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-system-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-system-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-system-coverage.json) |
<!-- module-table:end -->

</details>

**Module Descriptions:**
- **`command.rs`** — Command builder for llama processes
- **`health_monitor.rs`** — Continuous health monitoring for processes
- **`health.rs`** — Health check endpoint polling
- **`process_core.rs`** — Core process types and abstractions
- **`runner.rs`** — High-level runner facade for llama operations
- **`llama/`** — llama-server and llama-cli process management
- **`proxy/`** — Proxy supervisor and routing logic
- **`process/`** — Generic process lifecycle (start, stop, signal)
- **`system/`** — System probes (GPU detection, memory info)
- **`assistant_ui/`** — Terminal-based interactive chat UI
- **`ports_impl/`** — Port trait implementations for runtime
- **`assistant_ui/`** — Terminal-based chat UI for llama-cli
- **`command.rs`** — Command-line argument builder for llama binaries

## Features

- **Server Management** — Start/stop llama-server with automatic port allocation
- **CLI Chat** — Direct terminal chat via llama-cli
- **`OpenAI` Proxy** — Transparent proxy that routes to appropriate model instances
- **Auto Model Swap** — Proxy automatically loads/unloads models based on requests
- **Health Monitoring** — Polls server health endpoints for readiness
- **GPU Detection** — Detects available GPUs and VRAM for context sizing
- **Reasoning Model Support** — Streaming of thinking/reasoning phases

## Usage

```rust,ignore
use gglib_runtime::LlamaServerRunner;
use gglib_core::ports::ProcessRunner;
use gglib_core::domain::ServerConfig;

// Create a runner
let runner = LlamaServerRunner::new(8080, "/path/to/llama-server", 4);

// Start a server for a model
let handle = runner.start(ServerConfig {
    model_id: 1,
    model_name: "llama-3.2".to_string(),
    model_path: "/path/to/model.gguf".into(),
    context_size: Some(4096),
    ..Default::default()
}).await?;

// Stop the server
runner.stop(&handle).await?;
```

## Design Decisions

1. **Process Isolation** — Each llama-server runs as a separate process
2. **Graceful Shutdown** — Sends SIGTERM before SIGKILL with timeout
3. **Port Management** — Auto-allocates ports to avoid conflicts
4. **Proxy Architecture** — Single proxy endpoint routes to multiple backends
