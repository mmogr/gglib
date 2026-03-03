#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

pub(crate) mod agent_loop;
pub(crate) mod context_pruning;
pub(crate) mod fnv1a;
pub(crate) mod loop_detection;
pub(crate) mod stagnation;
pub(crate) mod stream_collector;
pub(crate) mod tool_execution;
pub(crate) mod util;

pub use agent_loop::AgentLoop;
pub use stream_collector::{collect_stream, CollectedResponse, MAX_TOOL_CALL_INDEX};
