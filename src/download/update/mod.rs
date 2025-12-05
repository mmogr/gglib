//! Update checking and application for downloaded models.
//!
//! This module handles checking for updates to locally downloaded models
//! by comparing stored commit SHAs with the latest on HuggingFace.

mod apply;
mod check;

pub use apply::handle_update_model;
pub use check::{check_model_update, handle_check_updates};
