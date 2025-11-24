#![doc = include_str!(concat!(env!("OUT_DIR"), "/crate_docs.md"))]

pub mod cli;
pub mod commands;
pub mod models;
pub mod proxy;
pub mod services;
pub mod utils;

// Re-export specific commonly used types
pub use models::gui::{ApiResponse, GuiModel, StartServerRequest, StartServerResponse};
pub use models::{Gguf, GgufMetadata};
pub use services::{database, gui_backend};
pub use utils::{gguf_parser, input, validation};
