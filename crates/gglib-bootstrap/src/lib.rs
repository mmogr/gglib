//! Shared composition root for gglib adapters.
//!
//! This crate consolidates the common infrastructure-wiring steps that the
//! adapters that build a backend (the CLI and the Axum daemon) need:
//!
//! 1. Database pool + repository set
//! 2. GGUF parser + model registrar
//! 3. `HuggingFace` HTTP client
//! 4. Download manager (accepting an injected event emitter)
//! 5. `AppCore`, with the `ModelVerificationService` it builds from the above,
//!    whose repair queues on that download manager
//!
//! Each adapter then adds its own concerns on top of the returned [`BuiltCore`]
//! (MCP service, proxy supervisor, SSE broadcaster, 7 domain `*Ops`, etc.).
//!
//! # Hexagonal boundary
//!
//! This crate depends **only** on pure infrastructure crates
//! (`gglib-core`, `gglib-db`, `gglib-download`, `gglib-gguf`, `gglib-hf`).
//! It does **not** depend on adapter crates (`gglib-mcp`, `gglib-axum`,
//! `gglib-tauri`, `gglib-cli`, `gglib-app-services`).
//!
//! # Example
//!
//! ```ignore
//! use std::sync::Arc;
//! use gglib_bootstrap::{BootstrapConfig, CoreBootstrap};
//! use gglib_core::ports::AppEventEmitter;
//!
//! let emitter: Arc<dyn AppEventEmitter> = Arc::new(MyEmitter::new());
//! let config = BootstrapConfig {
//!     db_path: database_path()?,
//! };
//! let core = CoreBootstrap::build(config, emitter).await?;
//! // core.app, core.downloads, core.hf_client, … all ready
//! ```
//!
//! # Testing
//!
//! The test suite uses these layers:
//!
//! - **Repair** (`src/builder_repair_tests.rs`): a repair through the wired
//!   core queues its download on the manager the adapters hold, and that
//!   manager runs it. The Hub is a stand-in.
//! - **Happy path / config** (`tests/build_happy_path.rs`): full
//!   `CoreBootstrap::build()` calls that confirm wiring succeeds and the
//!   returned [`BuiltCore`] is live.
//! - **Models directory** (`src/builder.rs`): the download manager's config
//!   names no models directory. What the manager does with none is tested in
//!   `gglib-download`.
//! - **Hub token** (`src/builder.rs`, `tests/hub_token.rs`): inline tests
//!   that one token is handed to the Hub client's config, the download
//!   manager's config and `AppCore`, or to none of them; and a test that
//!   `build()` reads `HF_TOKEN` itself and the returned `AppCore` holds it,
//!   which runs itself again in a process started with the variable.
//! - **Error cases** (`tests/build_error_cases.rs`): failure paths such as a
//!   missing database directory.
//! - **Functional round-trips** (`tests/functional.rs`): data round-trips
//!   through the wired repositories (model insert/list, settings save/reload,
//!   empty-state assertions for downloads, chat history, and MCP servers).
//!
//! Shared helpers in `tests/common/mod.rs` provide a `TempDir`-backed
//! `BootstrapConfig` and a [`gglib_core::ports::NoopEmitter`] so individual
//! test bodies stay to ≤ 5 lines.

mod builder;
mod built;
mod config;

pub use builder::CoreBootstrap;
pub use built::BuiltCore;
pub use config::BootstrapConfig;
