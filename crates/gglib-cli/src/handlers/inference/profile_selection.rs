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

use anyhow::{Result, anyhow, bail};

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
        match route {
            ModelRoute::Profiled { .. } => bail!(
                "'{identifier}' already names a profile; drop either the suffix or --profile {name}"
            ),
            // The suffix is not a profile and the base is a real model, so
            // carrying the whole id forward would ask the catalog for
            // something `resolve_route` already proved is not there — and
            // report it as a missing *model*. Name the real problem.
            ModelRoute::ProfileNotFound { suffix, .. } => {
                bail!("{}", not_found_message(suffix, profiles))
            }
            ModelRoute::Bare(_) => {}
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

/// Resolve the profile for a resumed conversation, stripping any suffix the
/// stored identifier carries.
///
/// Differs from [`select`] in the one way that matters on a resume: the
/// identifier was not typed this time, it was replayed from storage. So a
/// stored suffix is *not* a conflict with an explicit `--profile` — the flag
/// is the only thing the user actually said, and it wins.
///
/// A stored profile that has since been deleted degrades to unprofiled with a
/// warning rather than erroring: the alternative is a conversation nobody can
/// ever resume again, which is a steep price for a profile the user may not
/// even want any more.
pub(crate) async fn resume_profile(
    catalog: &dyn ModelCatalogPort,
    profiles: &[InferenceProfile],
    identifier: &mut String,
    flag: Option<&str>,
) -> Result<Option<InferenceProfile>> {
    let stored = match resolve_route(identifier, profiles, catalog).await {
        ModelRoute::Bare(model) => (model.to_owned(), None),
        ModelRoute::Profiled { model, profile } => (model.to_owned(), Some(profile.clone())),
        ModelRoute::ProfileNotFound { requested, suffix } => {
            eprintln!(
                "  Warning: this conversation was started with profile '{suffix}', which no \
                 longer exists. Resuming without it."
            );
            let base = requested
                .rsplit_once(':')
                .map_or(requested, |(base, _)| base)
                .to_owned();
            (base, None)
        }
    };
    *identifier = stored.0;

    match flag {
        Some(name) => profiles
            .iter()
            .find(|p| p.name == name)
            .cloned()
            .map(Some)
            .ok_or_else(|| anyhow!("{}", not_found_message(name, profiles))),
        None => Ok(stored.1),
    }
}

#[cfg(test)]
#[path = "profile_selection_tests.rs"]
mod profile_selection_tests;
