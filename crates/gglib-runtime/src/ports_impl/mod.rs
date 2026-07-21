#![doc = include_str!("README.md")]
pub mod llm_completion;
pub mod model_catalog;
pub mod model_runtime;

pub use llm_completion::LlmCompletionAdapter;
pub use model_catalog::{CatalogPortImpl, total_model_bytes};
pub use model_runtime::RuntimePortImpl;
