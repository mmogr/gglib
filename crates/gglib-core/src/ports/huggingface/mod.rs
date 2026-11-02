#![doc = include_str!("README.md")]
mod client;
mod download_group;
mod error;
#[cfg(any(test, feature = "test-utils"))]
pub mod fake_hub;
mod image_companions;
mod types;
mod variant_heads;
mod variant_match;
mod variants;

pub use client::{HfClientPort, SNIFF_HEAD_BYTES, hub_file_url};
pub use download_group::{DownloadGroup, download_group, largest_projector, repository_projector};
pub use error::{HfPortError, HfPortResult};
pub use image_companions::{Companion, image_companions};
pub use types::{
    HfFileInfo, HfModelKind, HfQuantInfo, HfRepoInfo, HfSearchOptions, HfSearchResult, HfSortField,
};
pub use variant_heads::{VARIANT_HEAD_BYTES, read_roles, stored_label, variant_head, variant_role};
pub use variant_match::{LabelMismatch, label_refusal, match_label, match_variant};
pub use variants::{HELPER_FILES_REFUSAL, variants_of};
