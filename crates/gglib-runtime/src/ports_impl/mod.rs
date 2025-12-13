//! Port implementations for gglib-runtime.
//!
//! These implementations provide concrete adapters for the abstract ports
//! defined in gglib-core. They connect the port interfaces to the actual
//! runtime infrastructure (ProcessManager, database, etc.).

pub mod model_catalog;
pub mod model_runtime;

pub use model_catalog::CatalogPortImpl;
pub use model_runtime::RuntimePortImpl;
