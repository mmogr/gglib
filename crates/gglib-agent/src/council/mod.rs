#![doc = include_str!("README.md")]
pub mod chief_of_staff;
pub(crate) mod compaction;
pub mod debate;
pub mod director;
pub mod estimator;
pub mod executor;
pub mod planner;
pub mod prompts;
pub mod spawn;
pub mod steering;
pub(crate) mod synthesis;

pub use director::{DirectorNode, DirectorPlan, PlanError};
pub use executor::{CouncilConfig, ExecuteError, execute};
pub use planner::plan;
pub use steering::NoteQueue;
