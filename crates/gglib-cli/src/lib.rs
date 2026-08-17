#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

// Silence unused dev-dependency warnings for planned test infrastructure
#[cfg(test)]
use tempfile as _;
#[cfg(test)]
use tokio_test as _;

// Dependencies used by handlers module (will be used as handlers are migrated)
use anyhow as _;
use dotenvy as _;
use hf_hub as _;
use rustyline as _;
use tokio as _;
use tracing as _;
use tracing_subscriber as _;

// gglib-runtime used for process runner in bootstrap
use gglib_runtime as _;

// gglib-axum used for web command in main.rs
use gglib_axum as _;

// gglib-proxy used for the shared SlotSnapshot/tokens_in_use parser in
// handlers/proxy_dashboard.rs
use gglib_proxy as _;

// Nothing in the workspace depends on this crate: its only outside consumers
// are its own `gglib` binary and its own `tests/`, and between them they need
// the twelve names re-exported below and nothing else. So the module tree is
// crate-internal and the re-export list *is* the public API — which is what
// lets `unreachable_pub` and then `dead_code` see inside `handlers/`.
pub(crate) mod benchmark_commands;
pub(crate) mod bootstrap;
pub(crate) mod commands;
pub(crate) mod config_commands;
pub(crate) mod conversation_settings;
pub(crate) mod daemon_client;
pub(crate) mod dispatch;
pub(crate) mod handlers;
pub(crate) mod llama_commands;
pub(crate) mod mcp_commands;
pub(crate) mod model_commands;
pub(crate) mod model_sort;
pub(crate) mod parser;
pub(crate) mod presentation;
pub(crate) mod reasoning_args;
pub(crate) mod sampling_params;
pub(crate) mod shared_args;
pub(crate) mod utils;

// Re-export primary types for convenient access
pub use bootstrap::{CliConfig, CliContext, bootstrap};
pub use commands::Commands;
pub use config_commands::{ConfigCommand, ModelsDirCommand, SettingsCommand};
pub use dispatch::dispatch;
pub use llama_commands::LlamaCommand;
pub use model_commands::ModelCommand;
pub use parser::Cli;
// Exported for `tests/cli_parity.rs` alone, and for one reason: the flag-parity
// guard derives the expected flag set from clap's own introspection of this
// type (`SamplingArgs::augment_args`) instead of a hand-written list. The list
// it replaced named seven of fifteen flags and passed anyway.
pub use shared_args::SamplingArgs;
