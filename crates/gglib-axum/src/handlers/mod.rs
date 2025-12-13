//! HTTP request handlers for the Axum web server.
//!
//! Each submodule contains handlers for a specific API area.
//! Handlers are thin wrappers that delegate to `GuiBackend`.

pub mod chat;
pub mod chat_proxy;
pub mod downloads;
pub mod events;
pub mod hf;
pub mod mcp;
pub mod models;
pub mod proxy;
pub mod servers;
pub mod settings;
