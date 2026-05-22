//! Orchestrator Director — decompose goals into task graphs.
//!
//! This module contains the director agent that translates a high-level goal
//! into a validated [`gglib_core::domain::orchestrator::task_graph::TaskGraph`]
//! of worker nodes.
//!
//! # Modules
//!
//! | Module | Contents |
//! |--------|----------|
//! | [`director`] | [`director::plan`] — director planning function, [`director::PlanError`] |
//! | [`prompts`]  | System prompt template, few-shot examples, JSON Schema |
//!
//! # Phase B scope
//!
//! Phase B implements **planning only** — no worker execution.  The returned
//! [`TaskGraph`] is ready for display (CLI tree, SSE stream, Tauri page) and
//! for Phase C+ execution.

pub mod director;
pub mod prompts;

pub use director::{DirectorNode, DirectorPlan, PlanError, plan};
