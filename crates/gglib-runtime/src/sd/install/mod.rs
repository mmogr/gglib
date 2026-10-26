#![doc = include_str!("README.md")]
mod pipeline;
mod release;
mod status;

pub use pipeline::{install_sd_prebuilt, run_sd_source_build};
pub use release::{PINNED_SD_RELEASE, SD_RELEASE_ENV, SdAsset, check_sd_prebuilt_availability};
pub use status::{SdStatus, sd_status};

pub(crate) use release::{SD_RELEASE, sd_download_dir};

#[cfg(test)]
#[path = "release_tests.rs"]
mod release_tests;

#[cfg(test)]
#[path = "install_tests.rs"]
mod install_tests;

#[cfg(test)]
#[path = "status_tests.rs"]
mod status_tests;
