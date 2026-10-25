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
    /// Whatever upstream currently calls `latest`, followed to the release
    /// its `nightly-tag.txt` names when it carries one.
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

/// The build number a release tag carries: the `n` of llama.cpp's `b<n>` and
/// of stable-diffusion.cpp's `master-<n>-<commit>`. Upstream counts them up
/// with every release, so a larger one is a later release. `None` for any
/// other tag, such as llama.cpp's `v*`.
pub(crate) fn release_number(tag: &str) -> Option<u64> {
    let digits = if let Some(rest) = tag.strip_prefix("master-") {
        let (number, commit) = rest.split_once('-')?;
        let is_commit = !commit.is_empty() && commit.bytes().all(|b| b.is_ascii_hexdigit());
        is_commit.then_some(number)?
    } else {
        tag.strip_prefix('b')?
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// Which asset of a release is this platform's: one whose name, when set,
/// starts with `starts_with`, holds every string in `contains` and, when set,
/// ends with `ends_with`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AssetMatcher<'a> {
    pub(crate) starts_with: Option<&'a str>,
    pub(crate) contains: &'a [&'a str],
    pub(crate) ends_with: Option<&'a str>,
}

impl AssetMatcher<'_> {
    /// Whether the asset named `name` is this platform's.
    pub(crate) fn matches(&self, name: &str) -> bool {
        self.starts_with.is_none_or(|start| name.starts_with(start))
            && self.contains.iter().all(|part| name.contains(part))
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
        let parts: Vec<&str> = self
            .starts_with
            .into_iter()
            .chain(self.contains.iter().copied())
            .chain(self.ends_with)
            .collect();
        f.write_str(&parts.join("*"))
    }
}

#[cfg(test)]
mod tests {
    use super::release_number;

    #[test]
    fn a_release_number_is_read_from_either_products_tags() {
        assert_eq!(release_number("b11429"), Some(11429));
        assert_eq!(release_number("master-948-228c707"), Some(948));
    }

    #[test]
    fn a_tag_of_any_other_shape_has_no_release_number() {
        for tag in [
            "",
            "b",
            "b12x",
            "v0.7.0",
            "74bca39",
            "master-",
            "master-948",
            "master-948-",
            "master-x-228c707",
            "master-948-xyz",
            "B11429",
        ] {
            assert_eq!(release_number(tag), None, "{tag:?}");
        }
    }
}
