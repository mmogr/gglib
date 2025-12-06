//! Boundary mappers for converting between domain and legacy types.
//!
//! These mappers handle translation at the boundary between the new
//! domain types in `core::domain` and legacy types like `Gguf`.
//!
//! # Dependency Direction
//!
//! - `infra/mappers` may depend on legacy types and `core/domain` types
//! - `core` must never depend on `infra` or legacy parsing modules

mod model_mapper;

pub use model_mapper::{gguf_to_model, gguf_to_new_model, model_to_gguf, new_model_to_gguf};
