#![doc = include_str!("README.md")]
mod client;
mod download_group;
mod error;
#[cfg(test)]
pub(crate) mod fake_hub;
mod types;

pub use client::HfClientPort;
pub use download_group::{DownloadGroup, download_group, projector_fetched_with};
pub use error::{HfPortError, HfPortResult};
pub use types::{
    HfFileInfo, HfQuantInfo, HfRepoInfo, HfSearchOptions, HfSearchResult, HfSortField,
};
