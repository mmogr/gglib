#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

pub(crate) mod agent_loop;
pub(crate) mod context_pruning;
pub(crate) mod filter;
pub(crate) mod loop_detection;
pub(crate) mod stagnation;
pub(crate) mod stream_collector;
pub(crate) mod tool_execution;

pub use agent_loop::AgentLoop;
pub use filter::FilteredToolExecutor;
