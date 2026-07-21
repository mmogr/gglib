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

See the [Architecture Overview](../../README.md#architecture) for the complete diagram.

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
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐                            │
│  │ command.rs  │     │process_core │     │ compose.rs  │                            │
│  │ Cmd builder │     │ Core types  │     │ Agent loop  │                            │
│  └─────────────┘     └─────────────┘     │ composition │                            │
│                                          └─────────────┘                            │
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
| [`command.rs`](src/command.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-command-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-command-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-command-coverage.json) |
| [`compose.rs`](src/compose.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-compose-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-compose-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-compose-coverage.json) |
| [`council_runner.rs`](src/council_runner.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-council_runner-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-council_runner-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-council_runner-coverage.json) |
| [`health.rs`](src/health.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health-coverage.json) |
| [`health_monitor.rs`](src/health_monitor.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health_monitor-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health_monitor-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health_monitor-coverage.json) |
| [`process_core.rs`](src/process_core.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process_core-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process_core-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-process_core-coverage.json) |
| [`runner.rs`](src/runner.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-runner-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-runner-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-runner-coverage.json) |
| [`server_config.rs`](src/server_config.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-server_config-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-server_config-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-server_config-coverage.json) |
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
- **`compose.rs`** — Agent loop composition root (wires LLM adapter + tool executors)
- **`runner.rs`** — High-level runner facade for llama operations
- **`llama/`** — llama-server and llama-cli process management
- **`proxy/`** — Proxy supervisor and routing logic
- **`process/`** — Generic process lifecycle (start, stop, signal)
- **`system/`** — System probes (GPU detection, memory info)
- **`assistant_ui/`** — Terminal-based interactive chat UI for llama-cli
- **`ports_impl/`** — Port trait implementations for runtime

## Features

- **Server Management** — Start/stop llama-server with automatic port allocation
- **CLI Chat** — Direct terminal chat via llama-cli
- **`OpenAI` Proxy** — Transparent proxy that routes to appropriate model instances
- **Auto Model Swap** — Proxy automatically loads/unloads models based on requests
- **Concurrent Startup Coordination** — SingleSwap strategy uses watch channels so concurrent requests during model startup wait for the result rather than failing immediately.
- **Health Monitoring** — Polls server health endpoints for readiness
- **GPU Detection** — Detects available GPUs and VRAM for context sizing
- **Reasoning Model Support** — Streaming of thinking/reasoning phases
- **MTP Speculative Decoding** — Auto-enabled for models with the `"mtp"` tag via the canonical `build_server_config` builder

## ServerConfig Builder

All launch surfaces (proxy auto-start, GUI/HTTP start-server, CLI agent chat)
must use `build_server_config` to construct a `ServerConfig`. This is the
canonical entry point that calls all capability resolvers in one place and
guarantees identical llama-server arguments across every surface.

```rust,ignore
use gglib_runtime::{build_server_config, ServerConfigOptions};

// Fully tag-driven — capabilities auto-detected from model metadata:
let config = build_server_config(
    model_id,
    model_name,
    model_path,
    base_port,
    &model.tags,
    ServerConfigOptions::default(),
);

// With caller overrides (e.g. explicit context size from a GUI request):
let config = build_server_config(
    model_id,
    model_name,
    model_path,
    base_port,
    &model.tags,
    ServerConfigOptions {
        context_size: Some(8192),
        mtp_draft_n_max: Some(0), // explicitly disable MTP
        ..Default::default()
    },
);
```

### Capability detection precedence

| Feature | Explicit override wins over… | Tag-based default |
|---------|------------------------------|-------------------|
| Jinja templates | `opts.jinja = Some(true/false)` | `"agent"` tag → enabled |
| Reasoning format | `opts.reasoning_format = Some(…)` | model tags |
| MTP speculative decoding | `opts.mtp_draft_n_max = Some(0)` (off) or `Some(n)` (on) | `"mtp"` tag → `n=2, p_min=0.75` |

### Context size resolution

The `resolve_context_size()` function implements a strict 4-level fallback chain
for determining the context window passed to llama-server:

```text
1. Runtime request / CLI flag (opts.context_size)
2. Per-model server_defaults (opts.model_server_ctx, from DB column server_defaults)
3. Global app setting (opts.global_default_ctx)
4. Hardcoded DEFAULT_CONTEXT_SIZE = 4096
```

Each level fills in only if the previous levels are `None`. This ensures per-model
overrides (`server_defaults.context_length`) take precedence over global settings,
while still allowing runtime flags to win when explicitly provided.

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
2. **Graceful Shutdown** — Sends SIGTERM before SIGKILL with bounded timeout (guards against D-state hang)
3. **Port Management** — Auto-allocates ports to avoid conflicts
4. **Proxy Architecture** — Single proxy endpoint routes to multiple backends
