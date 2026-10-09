#![doc = include_str!("README.md")]
mod install;
mod record;
mod release;
mod status;
mod uninstall;

pub use install::{install_sd_prebuilt, run_sd_source_build};
pub use release::{PINNED_SD_RELEASE, SD_RELEASE_ENV, SdAsset, check_sd_prebuilt_availability};
pub use status::{SdStatus, sd_status};
pub use uninstall::{sd_files_present, uninstall_sd};

#[cfg(test)]
#[path = "release_tests.rs"]
mod release_tests;

#[cfg(test)]
#[path = "install_tests.rs"]
mod install_tests;

#[cfg(test)]
#[path = "status_tests.rs"]
mod status_tests;
