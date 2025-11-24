//! Process management utilities.
//!
//! This module provides shared infrastructure for managing llama-server processes
//! across different use cases (proxy, GUI, etc.).

pub mod core;
pub mod health;
pub mod types;

// Re-export commonly used types
pub use core::ProcessCore;
pub use health::{check_process_health, update_health_batch, wait_for_http_health};
pub use types::{RunningProcess, ServerInfo};
