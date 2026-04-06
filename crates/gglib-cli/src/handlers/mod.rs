#![doc = include_str!("README.md")]

//! Command handlers that delegate to AppCore.
//!
//! This module contains the command execution logic for CLI commands.
//!
//! Handlers are organized into domain-scoped subdirectories:
//! - [`config`]    — settings, llama management, assistant-ui, paths, dep checks
//! - [`inference`] — serve, chat, question (shared resolve & logging)
//! - [`model`]     — add, list, remove, update, download, verify, search, browse
//!
//! Top-level handlers for commands that stand alone:
//! - [`gui`]       — Tauri desktop GUI launcher
//! - [`web`]       — Axum web-server GUI launcher

pub mod agent_chat;
pub mod config;
pub mod gui;
pub mod history;
pub mod inference;
pub mod mcp_cli;
pub mod model;
pub mod web;
