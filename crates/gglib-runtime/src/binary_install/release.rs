//! What an install is told about the product it fetches: the repository, the
//! pinned release and its override, where the archive is put while it is
//! unpacked, and which release asset is this platform's.

use anyhow::{Result, bail};
use std::fmt;
use std::path::PathBuf;

use super::fetch::{GitHubAsset, GitHubRelease};

/// Where GitHub's REST API answers.
pub(crate) const GITHUB_API: &str = "https://api.github.com";

/// Where in a tar.gz the files to install sit.
///
/// A zip's members are always taken by their file name wherever they sit:
/// both products' zips are flat, and that is the rule llama.cpp's Windows
/// zips have always been unpacked by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArchiveLayout {
    /// One directory down (`llama-b<tag>/<file>`); the directory entry itself
    /// and anything deeper are skipped.
    OneDirDeep,
    /// At the archive's root; anything in a directory is skipped.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "stable-diffusion.cpp, the next product, ships flat archives"
        )
    )]
    Flat,
}

/// How many of a release's assets may answer a platform's [`AssetMatcher`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssetChoice {
    /// The first that does, in the order the release lists them.
    First,
    /// Exactly one; two or more is an error naming them.
    ExactlyOne,
}

/// One product released on GitHub, as an install needs to know it.
#[derive(Debug)]
pub(crate) struct ReleaseSpec {
    /// How the product is named in messages (`llama.cpp`).
    pub(crate) product: &'static str,
    /// `owner/name` on GitHub.
    pub(crate) repo: &'static str,
    /// The release tag installed unless [`Self::env`] names another.
    pub(crate) pinned: &'static str,
    /// The environment variable that overrides [`Self::pinned`]: a tag, or
    /// `latest`.
    pub(crate) env: &'static str,
    /// Where the archive is downloaded to. The whole directory is removed
    /// once the archive is unpacked, so no two products may share one.
    pub(crate) download_dir: fn() -> Result<PathBuf>,
    /// Where a tar.gz holds its files.
    pub(crate) archive: ArchiveLayout,
    /// How many assets may match.
    pub(crate) choice: AssetChoice,
    /// Whether an archive member with this file name is installed.
    pub(crate) wanted: fn(&str) -> bool,
}

/// Which release an install should fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReleaseSelector {
    /// A specific tag: the pin, or an override naming one.
    Tag(String),
    /// Whatever upstream currently calls `latest`.
    Latest,
}

impl ReleaseSelector {
    /// The URL under `api_base` this selector resolves through for `repo`.
    pub(crate) fn api_url(&self, api_base: &str, repo: &str) -> String {
        match self {
            Self::Tag(tag) => format!("{api_base}/repos/{repo}/releases/tags/{tag}"),
            Self::Latest => format!("{api_base}/repos/{repo}/releases/latest"),
        }
    }
}

/// Interpret a raw value of `spec`'s override variable.
///
/// `latest` (any casing) floats; anything else is taken as a tag verbatim. A
/// blank or whitespace-only value is treated as unset rather than as an empty
/// tag, so `GGLIB_LLAMA_RELEASE=` does not produce a URL that cannot resolve.
///
/// Split from [`resolve_selector`] so the policy is testable without mutating
/// process environment, which no test can do safely in parallel.
pub(crate) fn selector_from_override(spec: &ReleaseSpec, raw: &str) -> ReleaseSelector {
    let trimmed = raw.trim();

    if trimmed.is_empty() {
        return ReleaseSelector::Tag(spec.pinned.to_owned());
    }
    if trimmed.eq_ignore_ascii_case("latest") {
        return ReleaseSelector::Latest;
    }
    ReleaseSelector::Tag(trimmed.to_owned())
}

/// Resolve which release to install from `spec`'s override variable, falling
/// back to its pin.
pub(crate) fn resolve_selector(spec: &ReleaseSpec) -> ReleaseSelector {
    selector_from_override(spec, &std::env::var(spec.env).unwrap_or_default())
}

/// Which asset of a release is this platform's: one whose name holds every
/// string in `contains` and, when set, ends with `ends_with`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AssetMatcher<'a> {
    pub(crate) contains: &'a [&'a str],
    pub(crate) ends_with: Option<&'a str>,
}

impl AssetMatcher<'_> {
    /// Whether the asset named `name` is this platform's.
    pub(crate) fn matches(&self, name: &str) -> bool {
        self.contains.iter().all(|part| name.contains(part))
            && self.ends_with.is_none_or(|end| name.ends_with(end))
    }

    /// The asset of `release` this matcher picks under `choice`.
    pub(crate) fn pick<'r>(
        &self,
        release: &'r GitHubRelease,
        choice: AssetChoice,
    ) -> Result<&'r GitHubAsset> {
        let mut matching = release.assets.iter().filter(|a| self.matches(&a.name));
        let Some(first) = matching.next() else {
            bail!(
                "No matching asset found for pattern '{}' in release {}",
                self,
                release.tag_name
            );
        };
        if choice == AssetChoice::ExactlyOne {
            let rest: Vec<&str> = matching.map(|a| a.name.as_str()).collect();
            if !rest.is_empty() {
                bail!(
                    "More than one asset matches pattern '{}' in release {}: {}, {}",
                    self,
                    release.tag_name,
                    first.name,
                    rest.join(", ")
                );
            }
        }
        Ok(first)
    }
}

/// The pattern as a message shows it: the parts in order, joined by `*`.
impl fmt::Display for AssetMatcher<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.contains.join("*"))?;
        if let Some(end) = self.ends_with {
            write!(f, "*{end}")?;
        }
        Ok(())
    }
}
