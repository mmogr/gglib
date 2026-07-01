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
//! - [`proxy_dashboard`] — live terminal view of a running proxy's dashboard stream

pub mod agent_chat;
pub mod benchmark;
pub mod completions;
pub mod config;
pub mod council;
pub mod gui;
pub mod history;
pub mod inference;
pub mod mcp_cli;
pub mod model;
pub mod plan;
pub mod proxy_dashboard;
pub mod web;
