#![doc = include_str!("README.md")]
pub mod llm_completion;
pub mod model_catalog;
pub mod model_runtime;
pub mod model_shards;

pub use llm_completion::{FarMachine, LlmCompletionAdapter, SamplingObserver};
pub use model_catalog::CatalogPortImpl;
pub use model_runtime::RuntimePortImpl;
pub use model_shards::{resident_bytes, total_model_bytes};
