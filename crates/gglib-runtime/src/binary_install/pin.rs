//! Where an installed release stands against the release an install would
//! fetch: the pin, unless the product's override names another.
//!
//! Every surface asks the same question of both products: is there anything
//! to install? Each reads the answer from [`standing`], so no two can
//! disagree.

use std::path::Path;

use serde::Serialize;

use super::checkout::tags_at;
use super::release::{ReleaseSelector, release_number};

/// Where what is installed stands against the release an install would
/// fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum PinStanding {
    /// Nothing is installed.
    Missing,
    /// The release an install would fetch is the one installed.
    AtPin,
    /// An earlier release, or a build gglib recorded without a release it
    /// can place. An install has the pin to put in its place.
    Older,
    /// A later release than the pin, such as one installed to try it before
    /// the pin moves. Nothing is offered.
    Newer,
    /// Installed, with nothing to compare: no record of the install (a binary
    /// put there by hand), or a target that cannot be placed offline, such as
    /// an override that follows upstream's newest or names a `v*` release,
    /// whose build is only known from GitHub. Nothing is offered.
    Unknown,
}

impl PinStanding {
    /// Whether an install has something to do without being forced.
    #[must_use]
    pub const fn wants_install(self) -> bool {
        matches!(self, Self::Missing | Self::Older)
    }
}

/// What an install recorded about itself, as far as [`standing`] needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Recorded {
    /// The release the record names: a tag, or a source build's short commit.
    pub(crate) release: String,
    /// The commit a source build was made from; `None` for a download.
    pub(crate) commit_sha: Option<String>,
}

impl Recorded {
    /// Whether the install was a source build.
    pub(crate) const fn built(&self) -> bool {
        self.commit_sha.is_some()
    }

    /// Whether the record names its build by commit rather than by release,
    /// as every source build did before builds were of a release.
    pub(crate) fn names_a_commit(&self) -> bool {
        !self.release.is_empty()
            && self
                .commit_sha
                .as_deref()
                .is_some_and(|sha| sha.starts_with(&self.release))
    }
}

/// The release an install is, as [`installed_release`] makes it out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Installed {
    /// A tag, or, for a source build whose commit carries no release tag
    /// gglib can find, its short commit.
    pub(crate) release: String,
    /// Whether [`Self::release`] is a release, as opposed to a commit no
    /// release could be found for. A commit is never compared by its
    /// characters: one that reads `b123456` is not build 123456.
    pub(crate) placed: bool,
}

/// The release an install is: the one its record names, unless that is a
/// source build's commit, which is replaced by the release tag on that
/// commit in the checkout it was built from, when there is one.
///
/// A shallow checkout of upstream's newest still carries the tag of that
/// commit's release, and a record that names a tag is never second-guessed:
/// the checkout can hold other tags on the same commit.
pub(crate) fn installed_release(recorded: &Recorded, checkout: &Path) -> Installed {
    let Some(sha) = recorded
        .commit_sha
        .as_deref()
        .filter(|_| recorded.names_a_commit())
    else {
        return Installed {
            release: recorded.release.clone(),
            placed: true,
        };
    };
    let tag = tags_at(checkout, sha).ok().and_then(|tags| {
        tags.into_iter()
            .filter_map(|tag| release_number(&tag).map(|n| (n, tag)))
            .max()
            .map(|(_, tag)| tag)
    });
    tag.map_or_else(
        || Installed {
            release: recorded.release.clone(),
            placed: false,
        },
        |release| Installed {
            release,
            placed: true,
        },
    )
}

/// Where `installed` stands against what `selector` would install.
///
/// `binary` is whether the product's server is there, and `installed` the
/// release its record makes it (see [`installed_release`]), `None` with no
/// record. Against a numbered pin, an install that cannot be placed, such
/// as a commit with no release, is older: it was recorded by an install
/// gglib made, and the pin is what gglib installs. A target with no number
/// of its own is only matched by name; anything else is unknown.
pub(crate) fn standing(
    binary: bool,
    installed: Option<&Installed>,
    selector: &ReleaseSelector,
) -> PinStanding {
    if !binary {
        return PinStanding::Missing;
    }
    let (Some(installed), ReleaseSelector::Tag(pin)) = (installed, selector) else {
        return PinStanding::Unknown;
    };
    if installed.placed && installed.release == *pin {
        return PinStanding::AtPin;
    }
    let have = release_number(&installed.release).filter(|_| installed.placed);
    match (have, release_number(pin)) {
        (_, None) => PinStanding::Unknown,
        (Some(have), Some(want)) if have > want => PinStanding::Newer,
        _ => PinStanding::Older,
    }
}

#[cfg(test)]
#[path = "pin_tests.rs"]
mod tests;
