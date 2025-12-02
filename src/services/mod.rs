#![doc = include_str!(concat!(env!("OUT_DIR"), "/services_docs.md"))]

pub mod chat_history;
pub mod core;
pub mod database;
pub mod gui_backend;
pub mod mcp;
pub mod process_manager;
pub mod settings;

// Re-export commonly used items
pub use chat_history::*;
pub use core::AppCore;
pub use database::*;
pub use mcp::{McpService, McpServerConfig, McpServerType, McpServerStatus, McpTool};
pub use process_manager::ProcessManager;
