//! Post-download workflows.
//!
//! This module contains workflows that run after a download completes,
//! such as registering models in the database.

mod finalize;

pub use finalize::{register_model, register_model_from_path};
