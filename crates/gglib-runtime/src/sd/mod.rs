#![doc = include_str!("README.md")]
mod install;
mod server;

pub use install::{
    PINNED_SD_RELEASE, SD_RELEASE_ENV, SdAsset, SdStatus, check_sd_prebuilt_availability,
    install_sd_prebuilt, run_sd_source_build, sd_status,
};
pub use server::{IMAGE_JOB_DEADLINE, IMAGE_STALL, POLL, SdImageDriver, SdServerConfig};

pub(crate) use install::{SD_RELEASE, sd_download_dir};
pub(crate) use server::build_and_spawn_sd;
#[cfg(test)]
pub(crate) use server::fake_server;
