//! Shared application state type.
//!
//! Defines the `AppState` type used across all handlers and routers.

use crate::bootstrap::AxumContext;
use std::sync::Arc;

/// Application state shared across all handlers.
///
/// This is an Arc-wrapped `AxumContext` containing all services
/// needed by API handlers (core, gui, mcp, downloads, etc.).
pub type AppState = Arc<AxumContext>;
