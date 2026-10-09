#![doc = include_str!("README.md")]
mod extract;
mod fetch;
mod install;
mod record;
mod release;

#[cfg(test)]
pub(crate) mod fake_github;

#[cfg(test)]
pub(crate) use extract::extract_binaries;
#[cfg(test)]
pub(crate) use install::install_prebuilt_from;
pub(crate) use install::{PrebuiltTarget, completed, install_prebuilt, started};
pub(crate) use record::PrebuiltRecord;
pub(crate) use release::{ArchiveLayout, AssetChoice, AssetMatcher, ReleaseSpec};
#[cfg(test)]
pub(crate) use release::{GITHUB_API, ReleaseSelector, selector_from_override};

#[cfg(test)]
#[path = "install_tests.rs"]
mod install_tests;
