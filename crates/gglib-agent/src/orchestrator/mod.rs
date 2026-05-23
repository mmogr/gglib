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
//! | [`chief_of_staff`] | [`chief_of_staff::brief`] — decompose goal into department briefs |
//! | [`director`] | [`director::plan`] — flat director planning, [`director::PlanError`] |
//! | [`planner`] | [`planner::plan`] — hierarchical two-tier planning entry point |
//! | [`prompts`]  | System prompt templates, few-shot examples, JSON Schemas |
//!
//! # Phase H scope
//!
//! Phase H replaces the flat single-shot director with a two-tier hierarchical
//! planner.  The executor's external call signature is unchanged — it calls
//! [`planner::plan`] which internally runs Chief of Staff → N × Director.

pub(crate) mod chief_of_staff;
pub(crate) mod compaction;
pub mod director;
pub mod executor;
pub mod planner;
pub mod prompts;
pub(crate) mod synthesis;

pub use director::{DirectorNode, DirectorPlan, PlanError};
pub use executor::{ExecuteError, OrchestratorConfig, execute};
pub use planner::plan;
