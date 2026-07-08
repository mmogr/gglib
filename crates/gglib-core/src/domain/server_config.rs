//! Server-level default configuration for models.
//!
//! This module defines [`ServerConfig`], which stores per-model server
//! parameters that override global settings but can themselves be overridden
//! at request time.
//!
//! # Fallback Chain
//!
//! Parameters are resolved in strict priority order:
//!
//! 1. Runtime request / CLI flag (highest priority)
//! 2. Model `server_defaults` (from DB, stored as JSON)
//! 3. Global app setting
//! 4. Hardcoded default (lowest priority)

use serde::{Deserialize, Serialize};

/// Server-level defaults for a specific model.
///
/// Stores per-model server configuration parameters that override global
/// settings but can themselves be overridden at request time. This is part
/// of the 4-level fallback chain:
///
/// 1. Runtime request / CLI flag (highest priority)
/// 2. Model `server_defaults` (from DB, stored as JSON in `server_defaults` column)
/// 3. Global app setting
/// 4. Hardcoded default (lowest priority)
///
/// All fields are optional to support partial configuration.
///
/// # Examples
///
/// ```rust
/// use gglib_core::domain::ServerConfig;
///
/// // Override only the context length for a long-context model
/// let config = ServerConfig {
///     context_length: Some(32768),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    /// Context length (number of tokens) for the model server.
    ///
    /// Controls the maximum context window the server will use.
    /// Common values: 4096 (default), 8192, 32768, 131072
    pub context_length: Option<usize>,
}
