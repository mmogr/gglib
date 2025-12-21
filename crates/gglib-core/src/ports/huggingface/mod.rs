//! `HuggingFace` client port definitions.
//!
//! This module defines the port trait and DTOs for `HuggingFace` Hub interaction.
//! The actual implementation lives in `gglib-hf`.

mod client;
mod error;
mod types;

pub use client::HfClientPort;
pub use error::{HfPortError, HfPortResult};
pub use types::{HfFileInfo, HfQuantInfo, HfRepoInfo, HfSearchOptions, HfSearchResult};
