#![doc = include_str!("README.md")]
mod args;
mod config;
mod job;
mod job_api;
mod job_plan;
mod job_poll;
mod source;
mod spawn;

pub use config::SdServerConfig;
pub use job::{IMAGE_JOB_DEADLINE, IMAGE_STALL, POLL, SdImageDriver};

#[cfg(test)]
pub(crate) use args::sd_server_args;
pub(crate) use spawn::build_and_spawn_sd;

#[cfg(test)]
pub(crate) mod fake_server;

#[cfg(test)]
#[path = "args_tests.rs"]
mod args_tests;

#[cfg(test)]
#[path = "health_tests.rs"]
mod health_tests;
