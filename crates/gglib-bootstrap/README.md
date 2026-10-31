# gglib-bootstrap

<!-- crate-header:start -->
![Tests](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-tests.json) ![Coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-coverage.json) ![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-loc.json) ![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-bootstrap-complexity.json)

Layer: **facade** · [workspace diagram](../README.md#architecture-overview)
<!-- crate-header:end -->

Shared composition root for gglib adapters.

Wiring happens once, here, so every adapter gets its database, repositories,
download manager and `AppCore` built the same way.

This crate consolidates the infrastructure-wiring steps that were previously duplicated
across the CLI, Axum, and Tauri bootstrap modules into a single
`CoreBootstrap::build(config, emitter) → BuiltCore` call.

## Architecture

This crate is the **Composition Root** — sitting between adapter crates and pure infrastructure, wiring all dependencies before handing a fully-configured `BuiltCore` to each adapter.

## What it wires

1. `SQLite` database pool + repository set
2. `GgufParser` + `ModelRegistrar`
3. `HfClient` (`HuggingFace` HTTP client)
4. Download manager (using the injected `AppEventEmitter`)
5. `AppCore`, with the `ModelVerificationService` it builds from the above,
   whose repair queues on that download manager

## Internal Structure

```text
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                                 gglib-bootstrap                                     │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                     │
│   ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐                     │
│   │ config.rs       │  │ built.rs        │  │ builder.rs      │                     │
│   │ BootstrapConfig │  │ BuiltCore       │  │ CoreBootstrap   │                     │
│   └────────┬────────┘  └────────┬────────┘  └────────┬────────┘                     │
│            └────────────────────┼────────────────────┘                              │
│                                 ▼                                                   │
│                 lib.rs (declares modules + re-exports)                              │
│                                                                                     │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

**Exported API:** `BootstrapConfig` (input), `CoreBootstrap::build()` (async entry point), `BuiltCore` (output carrying all wired infrastructure).

## Hexagonal boundary

Depends **only** on infrastructure crates:
`gglib-core`, `gglib-db`, `gglib-download`, `gglib-gguf`, `gglib-hf`.

Does **not** depend on adapter crates (`gglib-mcp`, `gglib-axum`, `gglib-cli`, `gglib-app-services`).

## Testing

The test suite is split into these layers:

| Layer | Location | Purpose |
|-------|----------|---------|
| Repair | `src/builder_repair_tests.rs` | A repair through the wired core queues its download on the manager the adapters hold, and that manager runs it. The Hub is a stand-in. |
| Happy path / config | `tests/build_happy_path.rs` | Full `CoreBootstrap::build()` calls that confirm the wiring succeeds and the returned `BuiltCore` is live. |
| Config | `src/config.rs` `#[cfg(test)]` | `BootstrapConfig::for_db` names no models directory and no token, and the config's `Debug` does not show a token. |
| Models directory | `src/builder.rs` `#[cfg(test)]` | The download manager's config names the directory in the config, or none. What the manager does with none is tested in `gglib-download`. |
| Hub token | `src/builder.rs` `#[cfg(test)]`, `tests/hub_token.rs` | Inline: one token is handed to the Hub client's config, the download manager's config and `AppCore`, or to none of them; it is the config's, trimmed, and the environment is not read, or, when the config holds none or a blank one, the environment's. `tests/hub_token.rs`: `build()` handed no token reads `HF_TOKEN` itself, and the `AppCore` it returns holds it; handed one, it holds that one whatever `HF_TOKEN` says. Those tests run themselves again in a process started with the variable, since a running process cannot safely set it. |
| Error cases | `tests/build_error_cases.rs` | Exercises the failure paths of `build()` — missing DB directory and DB path pointing at a directory. |
| Functional round-trips | `tests/functional.rs` | End-to-end data round-trips through the wired repositories: model insert/list, settings save/reload, empty-state assertions for downloads, chat history, and MCP servers. |

Shared test helpers (`TempDir`-backed config, `NoopEmitter`, `build_core`) live in
`tests/common/mod.rs` to keep individual test bodies to ≤ 5 lines.

Run all bootstrap tests:

```bash
cargo test -p gglib-bootstrap
```

## Design Principles

1. **No Adapter Dependencies** — Must not depend on tauri, axum, tower, or CLI crates
2. **Single Call** — All infrastructure wired via one `CoreBootstrap::build()` async call
3. **Emitter Injection** — Event emission strategy supplied by the caller (the daemon passes its SSE broadcaster, the CLI a `NoopEmitter`)
4. **Owned Output** — `BuiltCore` owns all constructed values; adapters clone `Arc`s as needed

## Usage

```rust,ignore
use std::sync::Arc;
use gglib_bootstrap::{BootstrapConfig, CoreBootstrap};
use gglib_core::paths::database_path;

let emitter: Arc<dyn AppEventEmitter> = Arc::new(MyAdapterEmitter::new());
let config = BootstrapConfig::for_db(database_path()?);
let core = CoreBootstrap::build(config, emitter).await?;
// core.app, core.downloads, core.hf_client … all ready
```

`BootstrapConfig::for_db` is the daemon's and the CLI's config: a database,
and no models directory or token of their own. A caller that keeps both
itself, as an app on a phone does, names them:

```rust,ignore
let config = BootstrapConfig {
    models_dir: Some(container.join("models")),
    hf_token: stored_token,
    ..BootstrapConfig::for_db(container.join("gglib.db"))
};
```

The `HuggingFace` token `build()` wires is the one in the config, or, when it
holds none, the one in `HF_TOKEN` (`gglib_core::hf_token::from_env`, the one
place the variable is read). The config's is read by the variable's rule,
`gglib_core::hf_token::normalised`: the spaces around it are dropped, and a
blank one is no token, so the variable is read instead. Either way the Hub
client, the download manager and `AppCore` are all handed it. The config's
`Debug` says whether it holds a token, never what it is.

A models directory in the config is the one the download manager puts every
download under. A caller that resolves the directory itself, as
`gglib-app-services`' model upgrade does, does not see it. The daemon names
none: one it resolved would be the directory as it started, and it would
download there for the rest of its life. With none, the download manager
asks `gglib_core::paths::resolve_models_dir` as each download starts, so a
download goes where the directory resolves then: for a running daemon, one
stored since from the settings page or with `gglib config models-dir set`,
unless the daemon's own environment names one.
