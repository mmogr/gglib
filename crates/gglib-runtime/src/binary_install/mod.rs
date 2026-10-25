#![doc = include_str!("README.md")]
mod checkout;
mod extract;
mod fetch;
mod install;
mod pin;
mod record;
mod release;

#[cfg(test)]
pub(crate) mod fake_github;

#[cfg(test)]
pub(crate) use checkout::clone_args;
pub(crate) use checkout::{SourceRepo, leftovers, prepare_checkout, retire_checkout, settle};
#[cfg(test)]
pub(crate) use extract::extract_binaries;
#[cfg(test)]
pub(crate) use install::install_prebuilt_from;
pub(crate) use install::{PrebuiltTarget, completed, install_prebuilt, started};
pub use pin::PinStanding;
pub(crate) use pin::{Recorded, installed_release, standing};
pub use record::BuildRecord;
pub(crate) use record::{
    InstallRecord, PrebuiltRecord, recorded_acceleration, recorded_install, recorded_release,
};
pub(crate) use release::{
    ArchiveLayout, AssetChoice, AssetMatcher, ReleaseSelector, ReleaseSpec, release_number,
    resolve_selector,
};
#[cfg(test)]
pub(crate) use release::{GITHUB_API, selector_from_override};

#[cfg(test)]
#[path = "install_tests.rs"]
mod install_tests;
