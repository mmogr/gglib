#![doc = include_str!("README.md")]

#![doc = include_str!("README.md")]

// MIGRATION: content extracted to README.md — remove this //! block after review
// MIGRATION: content extracted to README.md — remove this //! block after review
//! Inference command handlers.
//!
//! Handles `serve`, `chat`, and `question` — the three top-level commands
//! that run models. Shared inference-config resolution and logging live
//! in the [`shared`] submodule to avoid duplication.

pub mod agent_question;
pub mod chat;
pub mod serve;
pub mod shared;
