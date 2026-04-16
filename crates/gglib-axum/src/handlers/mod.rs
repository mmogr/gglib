//! HTTP request handlers for the Axum web server.
//!
//! Handlers are organized into domain-scoped subdirectories:
//! - [`model`]  — CRUD, verification, downloads, HuggingFace discovery
//! - [`config`] — settings, system setup
//!
//! Top-level modules for other domains:

pub mod agent;
pub mod builtin;
pub mod chat;
pub mod config;
pub mod council;
pub mod events;
pub mod mcp;
pub mod model;
pub mod port_utils;
pub mod proxy;
pub mod servers;
pub mod voice;
pub mod voice_ws;
