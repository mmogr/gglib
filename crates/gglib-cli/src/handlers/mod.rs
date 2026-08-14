#![doc = include_str!("README.md")]

//! Command handlers that delegate to AppCore.
//!
//! This module contains the command execution logic for CLI commands.
//!
//! Handlers are organized into domain-scoped subdirectories:
//! - [`config`]    — settings, llama management, paths, dep checks
//! - [`inference`] — serve, chat, question (shared resolve & logging)
//! - [`model`]     — add, list, remove, update, download, verify, search, browse
//!
//! Top-level handlers for commands that stand alone:
//! - [`up`]        — first-run: hardware → llama.cpp → model → proxy → endpoint
//! - [`gui`]       — Tauri desktop GUI launcher
//! - [`web`]       — Axum web-server GUI launcher
//! - [`proxy_dashboard`] — live terminal view of a running proxy's dashboard stream

pub(crate) mod agent_chat;
pub(crate) mod benchmark;
pub(crate) mod completions;
pub(crate) mod config;
pub(crate) mod daemon;
pub(crate) mod gui;
pub(crate) mod history;
pub(crate) mod inference;
pub(crate) mod mcp_cli;
pub(crate) mod model;
pub(crate) mod proxy_cache_clear;
pub(crate) mod proxy_dashboard;
pub(crate) mod up;
pub(crate) mod web;
