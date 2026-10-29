#![doc = include_str!("README.md")]
mod client;
mod download_group;
mod error;
#[cfg(any(test, feature = "test-utils"))]
pub mod fake_hub;
mod image_companions;
mod types;

pub use client::{HfClientPort, SNIFF_HEAD_BYTES, hub_file_url};
pub use download_group::{DownloadGroup, download_group, projector_fetched_with};
pub use error::{HfPortError, HfPortResult};
pub use image_companions::{Companion, image_companions};
pub use types::{
    HfFileInfo, HfModelKind, HfQuantInfo, HfRepoInfo, HfSearchOptions, HfSearchResult, HfSortField,
};
