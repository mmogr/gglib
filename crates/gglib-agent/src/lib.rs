#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]
// Allow dev-only crates (gglib-runtime, reqwest) used exclusively in
// integration-test files under `tests/`.
#![cfg_attr(test, allow(unused_crate_dependencies))]

pub(crate) mod agent_loop;
pub(crate) mod context_pruning;
pub(crate) mod fnv1a;
pub(crate) mod loop_detection;
pub mod council;
pub(crate) mod stagnation;
pub(crate) mod stream_collector;
pub mod structured_output;
pub(crate) mod tool_execution;
pub(crate) mod util;

pub use agent_loop::AgentLoop;

// These symbols are public only so integration tests in `tests/` can reach
// them.  They are NOT part of the stable public API — external crates should
// not depend on them.
#[doc(hidden)]
pub use stream_collector::{CollectedResponse, MAX_TOOL_CALL_INDEX, collect_stream};
