#![doc = include_str!("README.md")]
pub mod llm_completion;
pub mod model_runtime;

pub use llm_completion::{FarMachine, LlmCompletionAdapter, SamplingObserver};
pub use model_runtime::RuntimePortImpl;
