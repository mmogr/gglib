# gglib-runtime

![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-tests.json)
![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-coverage.json)
![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-complexity.json)

Process management and system probes for gglib — manages llama.cpp server instances and proxy routing.

Every llama-server on the machine is owned here. This crate decides what gets
loaded and when (admission over a bounded resident set), what flags each model
launches with, and narrates those decisions at startup — the work that makes
the runtime feel smarter than llama.cpp on its own.

## Architecture

This crate is in the **Infrastructure Layer** — it manages external processes and provides system information.

```text
gglib-core (ports)          gglib-runtime                       External
┌──────────────────────┐    ┌──────────────────────┐    ┌──────────────────┐
│ ModelRuntimePort     │◄───│ RuntimePortImpl      │───►│  llama-server    │
│ ModelCatalogPort     │    │ CatalogPortImpl      │    │  llama-cli       │
│ LlmCompletionPort    │    │ LlmCompletionAdapter │    └──────────────────┘
│ SystemProbePort      │    │ DefaultSystemProbe   │
│ ServerLogSinkPort    │    │ NoopLogSink,         │
│                      │    │ LogManagerSink       │
│ AdmissionRelease     │    │ AdmissionQueue       │
└──────────────────────┘    └──────────────────────┘
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
│  │ ports_impl/ │     │   llama/    │     │   proxy/    │     │  process/   │        │
│  │ Port trait  │ ──► │ llama-server│     │  OpenAI API │     │  Lifecycle  │        │
│  │   impls     │     │  llama-cli  │     │   routing   │     │  management │        │
│  └─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘        │
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐                                                │
│  │  health.rs  │     │   system/   │                                                │
│  │ Health check│     │ GPU, memory │                                                │
│  │  endpoints  │     │   probes    │                                                │
│  └─────────────┘     └─────────────┘                                                │
│                                                                                     │
│  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐                            │
│  │ command.rs  │     │server_config│     │ compose.rs  │                            │
│  │ Cmd builder │     │ Launch args │     │ Agent loop  │                            │
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
| [`health.rs`](src/health.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health-coverage.json) |
| [`health_monitor.rs`](src/health_monitor.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health_monitor-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health_monitor-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health_monitor-coverage.json) |
| [`health_monitor_tests.rs`](src/health_monitor_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health_monitor_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health_monitor_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-health_monitor_tests-coverage.json) |
| [`launch_narration.rs`](src/launch_narration.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-launch_narration-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-launch_narration-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-launch_narration-coverage.json) |
| [`server_config.rs`](src/server_config.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-server_config-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-server_config-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-server_config-coverage.json) |
| [`server_config_tests.rs`](src/server_config_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-server_config_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-server_config_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-server_config_tests-coverage.json) |
| [`unified_server_config.rs`](src/unified_server_config.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-unified_server_config-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-unified_server_config-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-runtime-unified_server_config-coverage.json) |
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
- **`compose.rs`** — Agent loop composition root (wires LLM adapter + tool executors)
- **`launch_narration.rs`** — Human-readable account of how a launch was configured
- **`server_config.rs`** / **`unified_server_config.rs`** — Launch argument resolution
- **`pidfile/`** — PID file writing and cleanup for spawned servers
- **`llama/`** — llama-server and llama-cli process management
- **`proxy/`** — Proxy supervisor and routing logic
- **`process/`** — Generic process lifecycle (start, stop, signal)
- **`system/`** — System probes (GPU detection, memory info)
- **`ports_impl/`** — Port trait implementations for runtime

## Features

- **Server Management** — Start/stop llama-server with automatic port allocation
- **CLI Chat** — Direct terminal chat via llama-cli
- **`OpenAI` Proxy** — Transparent proxy that routes to appropriate model instances
- **Auto Model Swap** — Proxy automatically loads/unloads models based on requests
- **Pinned Mode** — `ProcessManager::set_pin` restricts the resident set to exactly one model and refuses all others, backing `gglib serve`
- **Admission & Startup Coordination** — concurrent requests during a model's launch wait in the admission queue and are served when the launch completes, rather than failing immediately.
- **Health Monitoring** — Polls server health endpoints for readiness
- **GPU Detection** — Detects available GPUs and VRAM for context sizing
- **Reasoning Model Support** — Streaming of thinking/reasoning phases
- **MTP Speculative Decoding** — Auto-enabled for models with the `"mtp"` tag via the canonical `build_server_config` builder

## Config: one translator, fed by an optional cascade

Every launch surface — the CLI, the proxy, both GUIs — ultimately calls
`build_server_config`, which is what guarantees a given model receives
identical llama-server arguments regardless of what started it. Capability
detection lives in exactly one place; adding a resolver there reaches every
surface automatically.

A caller juggling explicit overrides, curated model defaults and global
defaults resolves them first with `UnifiedServerConfig::resolved_options()`,
then hands the flattened result to `build_server_config` alongside the
model's identity and tags:

```rust,ignore
use gglib_runtime::{UnifiedServerConfig, GlobalDefaults, build_server_config};
use gglib_core::server_config::ServerConfigOptions;

let opts = UnifiedServerConfig {
    explicit: ServerConfigOptions { mlock: Some(true), ..Default::default() },
    globals: GlobalDefaults::default(),
}
.resolved_options();

let config = build_server_config(model.id, model.name.clone(), model.file_path.clone(), base_port, &model.tags, opts);
```

### The translator directly

Callers that have already flattened their options can reach
`build_server_config` on its own:

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
| Jinja templates | `opts.jinja = Some(true)` → `--jinja`, `Some(false)` → `--no-jinja` | `"agent"` tag → `--jinja`; otherwise **no flag**, leaving llama-server's own default (jinja on) |
| Reasoning format | `opts.reasoning_format = Some(…)` | model tags |
| MTP speculative decoding | `opts.mtp_draft_n_max = Some(0)` (off) or `Some(n)` (on) | `"mtp"` tag → `n=2, p_min=0.75` |

The jinja row is the one where gglib's silence is not a "no". llama-server
initialises `use_jinja = true` and the server example never clears it, so a
launch that emits no flag runs *with* jinja. That is why the flag is emitted in
both directions: `--no-jinja` is the only thing gglib can send that turns jinja
off, and nothing tag-derived reaches it — taking tool-call templating and
template kwargs away from every non-agent model is a choice only the user gets
to make. One caveat: llama.cpp also reads `LLAMA_ARG_JINJA` from the
environment, which gglib does not sanitise, so an exported `LLAMA_ARG_JINJA=0`
turns jinja off on a deferred launch. It cannot beat `--no-jinja`, since
arguments are applied after the environment.

### Context size resolution

The `resolve_context_size()` function implements a strict 5-level fallback chain
for determining the context window passed to llama-server:

```text
1. Runtime request / CLI flag (opts.context_size)
2. Per-model server_defaults (opts.model_server_ctx, from DB column server_defaults)
3. Global app setting (opts.global_default_ctx) — only when the user set one
4. Fitted to this machine (opts.fitted_ctx)
5. Hardcoded DEFAULT_CONTEXT_SIZE = 4096
```

Each level fills in only if the previous levels are `None`. This ensures per-model
overrides (`server_defaults.context_length`) take precedence over global settings,
while still allowing runtime flags to win when explicitly provided.

Level 4 is computed by `gglib_core::domain::fit_context` from the model's trained
context, its weights, its KV shape and the device's capacity less what other
models already hold. It refuses — yielding level 5 — whenever any of that is
unknown, and `GGLIB_DISABLE_CONTEXT_FIT` turns it off entirely.

Level 3 carries `None` all the way from `Settings` rather than being resolved
early. That matters: pre-resolving it turned "the user set nothing" into "the
user set 4096", which made levels 4 and 5 unreachable.

## Usage

```rust,ignore
use gglib_runtime::GuiProcessCore;
use gglib_core::ports::ServerConfig;

// Create a process core, given a base port to allocate from
let mut core = GuiProcessCore::new(8080, "/path/to/llama-server");

// Start a server for a model; the allocated port comes back
let port = core.spawn(
    ServerConfig::new(1, "llama-3.2".to_string(), "/path/to/model.gguf".into(), 8080)
        .with_context_size(4096),
).await?;

// Stop the server, by model id
core.kill(1).await?;
```

## Design Decisions

1. **Process Isolation** — Each llama-server runs as a separate process
2. **Graceful Shutdown** — Sends SIGTERM before SIGKILL with bounded timeout (guards against D-state hang)
3. **Port Management** — Auto-allocates ports to avoid conflicts
4. **Proxy Architecture** — Single proxy endpoint routes to multiple backends
