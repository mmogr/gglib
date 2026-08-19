//! Turning what the user typed into a model name and a profile.
//!
//! Two forms mean the same thing: `--profile coding`, and the `{model}:{profile}`
//! suffix that HTTP clients already use. Both funnel through here so that
//! `chat`, `q` and `serve` cannot disagree about what a colon means.
//!
//! # Why both, and why not both at once
//!
//! The suffix is the form the proxy speaks, so supporting it lets a command
//! line be pasted from a client config unchanged. The flag is the form that
//! works when there is no identifier to suffix — `gglib chat --continue 42`
//! resumes a conversation and names no model at all.
//!
//! Passing both is an error rather than a precedence rule. Silent precedence
//! is one more rule to memorise, and the doctrine this feature already follows
//! is that an ambiguous profile fails loudly rather than sampling at the wrong
//! temperature — see [`gglib_core::request_pipeline::profile_route`].
//!
//! # Why the port, not the context
//!
//! [`select`] takes `&dyn ModelCatalogPort` and a profile slice rather than
//! `&CliContext`, which owns a live `AppCore`, an MCP service and a SQLite
//! pool and cannot be built in a unit test. The narrow signature is what makes
//! the conflict and not-found paths testable at all.

use anyhow::{Result, bail};

use gglib_core::domain::InferenceProfile;
use gglib_core::ports::ModelCatalogPort;
use gglib_core::request_pipeline::{ModelRoute, resolve_route};

use crate::handlers::config::settings::profiles::not_found_message;

/// What a command-line identifier plus an optional `--profile` resolved to.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProfileSelection {
    /// The identifier with any profile suffix removed, for model lookup.
    pub model: String,
    /// The selected profile, if either form named one.
    pub profile: Option<InferenceProfile>,
}

/// Resolve an identifier and an optional `--profile` into a model and profile.
///
/// # Errors
///
/// - both a `--profile` flag and a `:suffix` were given;
/// - either form named a profile that is not configured.
pub(crate) async fn select(
    catalog: &dyn ModelCatalogPort,
    profiles: &[InferenceProfile],
    identifier: &str,
    flag: Option<&str>,
) -> Result<ProfileSelection> {
    let route = resolve_route(identifier, profiles, catalog).await;

    if let Some(name) = flag {
        if matches!(route, ModelRoute::Profiled { .. }) {
            bail!(
                "'{identifier}' already names a profile; drop either the suffix or --profile {name}"
            );
        }
        let Some(profile) = profiles.iter().find(|p| p.name == name) else {
            bail!("{}", not_found_message(name, profiles));
        };
        return Ok(ProfileSelection {
            model: identifier.to_owned(),
            profile: Some(profile.clone()),
        });
    }

    match route {
        ModelRoute::Bare(model) => Ok(ProfileSelection {
            model: model.to_owned(),
            profile: None,
        }),
        ModelRoute::Profiled { model, profile } => Ok(ProfileSelection {
            model: model.to_owned(),
            profile: Some(profile.clone()),
        }),
        // The base is a real model and the suffix means nothing. Failing here
        // rather than dropping the suffix is the same call the proxy makes:
        // a renamed profile must not keep working at the wrong temperature.
        ModelRoute::ProfileNotFound { suffix, .. } => {
            bail!("{}", not_found_message(suffix, profiles))
        }
    }
}

#[cfg(test)]
#[path = "profile_selection_tests.rs"]
mod profile_selection_tests;
