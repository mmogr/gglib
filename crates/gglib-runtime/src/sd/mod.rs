#![doc = include_str!("README.md")]
mod args;
mod config;
mod install;
mod job;
mod job_api;
mod job_plan;
mod job_poll;
mod record;
mod release;
mod spawn;
mod status;
mod uninstall;

pub use config::SdServerConfig;
pub use install::{install_sd_prebuilt, run_sd_source_build};
pub use job::{IMAGE_JOB_DEADLINE, IMAGE_STALL, POLL, SdImageDriver};
pub use release::{PINNED_SD_RELEASE, SD_RELEASE_ENV, SdAsset, check_sd_prebuilt_availability};
pub use status::{SdStatus, sd_status};
pub use uninstall::{sd_files_present, uninstall_sd};

pub(crate) use record::recorded_release;
pub(crate) use spawn::build_and_spawn_sd;

#[cfg(test)]
pub(crate) mod fake_server;

#[cfg(test)]
#[path = "args_tests.rs"]
mod args_tests;

#[cfg(test)]
#[path = "health_tests.rs"]
mod health_tests;

#[cfg(test)]
#[path = "release_tests.rs"]
mod release_tests;

#[cfg(test)]
#[path = "install_tests.rs"]
mod install_tests;

#[cfg(test)]
#[path = "status_tests.rs"]
mod status_tests;
