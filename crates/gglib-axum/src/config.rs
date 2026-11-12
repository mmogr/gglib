//! What the Axum adapter is given: [`ServerConfig`].
//!
//! Separate from `bootstrap.rs`, the composition root, along the line
//! `gglib-bootstrap` draws between its own `config.rs` and `builder.rs`: this
//! is a value a caller builds and hands over, and `bootstrap` is what is done
//! with it.

use std::path::PathBuf;

use anyhow::Result;
use gglib_core::paths::{llama_server_path, sd_server_path};

/// What [`bootstrap`](crate::bootstrap::bootstrap) builds the adapter's
/// context from. The hosts the daemon answers to are set by
/// [`DaemonOptions`](crate::DaemonOptions).
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Base port for llama-server instances, when the caller names one.
    ///
    /// `None` leaves it to the saved `llama_base_port` setting, then the
    /// compiled default — see `gglib_app_services`' `resolve_llama_base_port`.
    /// The daemon names none, so `config set llama-base-port`
    /// reaches it when it next starts (#1161); tests name one to stay off the
    /// ports a running daemon uses.
    pub base_port: Option<u16>,
    /// Path to the llama-server binary.
    pub llama_server_path: PathBuf,
    /// Path to the `sd-server` binary, the image runtime.
    pub sd_server_path: PathBuf,
    /// Maximum concurrent agent loop sessions.
    ///
    /// Each agent run holds one permit until it ends. When all permits are
    /// taken, a new turn is refused `429 agent_busy` immediately rather than
    /// queued.
    pub max_concurrent_agent_loops: usize,
    /// Database file to open. `None` resolves through [`database_path`](gglib_core::paths::database_path).
    ///
    /// Naming a path lets a caller run against a database of its own, which
    /// is what keeps the integration tests off the developer's: otherwise
    /// [`database_path`](gglib_core::paths::database_path) resolves into the user's real gglib folder. Either way
    /// the database layer creates the parent directory and makes it `0700`.
    pub db_path: Option<PathBuf>,
    /// The file the remote tunnel keeps device keys in. `None` is the one
    /// beside the endpoint identity, where a daemon keeps them.
    ///
    /// Named for [`db_path`](Self::db_path)'s reason: the default is the
    /// user's real `data/`, which the installed daemon uses too, and a test
    /// that invites or lists devices must not read or write that.
    pub device_keys_path: Option<PathBuf>,
}

impl ServerConfig {
    /// Create config with default paths.
    pub fn with_defaults() -> Result<Self> {
        Ok(Self {
            base_port: None,
            llama_server_path: llama_server_path()?,
            sd_server_path: sd_server_path()?,
            max_concurrent_agent_loops: 4,
            db_path: None,
            device_keys_path: None,
        })
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
