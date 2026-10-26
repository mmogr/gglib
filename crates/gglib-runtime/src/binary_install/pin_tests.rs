//! Where an install stands against the pin.

use std::path::Path;
#[cfg(unix)]
use std::process::Command;

use super::{Installed, PinStanding, Recorded, installed_release, standing};
use crate::binary_install::ReleaseSelector;

fn pin(tag: &str) -> ReleaseSelector {
    ReleaseSelector::Tag(tag.to_owned())
}

/// An install known to be `release`.
fn placed(release: &str) -> Installed {
    Installed {
        release: release.to_owned(),
        placed: true,
    }
}

/// A source build's commit that no release tag was found for.
fn commit(short: &str) -> Installed {
    Installed {
        release: short.to_owned(),
        placed: false,
    }
}

#[test]
fn no_binary_is_missing_whatever_the_record_says() {
    assert_eq!(
        standing(false, Some(&placed("b11429")), &pin("b11429")),
        PinStanding::Missing
    );
    assert_eq!(standing(false, None, &pin("b11429")), PinStanding::Missing);
}

#[test]
fn the_pin_installed_is_at_the_pin() {
    assert_eq!(
        standing(true, Some(&placed("b11429")), &pin("b11429")),
        PinStanding::AtPin
    );
    assert_eq!(
        standing(
            true,
            Some(&placed("master-948-228c707")),
            &pin("master-948-228c707")
        ),
        PinStanding::AtPin
    );
}

#[test]
fn an_earlier_release_is_older_and_a_later_one_newer() {
    assert_eq!(
        standing(true, Some(&placed("b10327")), &pin("b11429")),
        PinStanding::Older
    );
    assert_eq!(
        standing(true, Some(&placed("b11870")), &pin("b11429")),
        PinStanding::Newer
    );
    assert_eq!(
        standing(
            true,
            Some(&placed("master-900-aaaaaaa")),
            &pin("master-948-228c707")
        ),
        PinStanding::Older
    );
    assert_eq!(
        standing(
            true,
            Some(&placed("master-1006-0886f11")),
            &pin("master-948-228c707")
        ),
        PinStanding::Newer
    );
}

/// A short commit was written by a source build of master, before builds
/// were of a release: gglib made it, and the pin replaces it.
#[test]
fn a_release_that_cannot_be_placed_against_a_numbered_pin_is_older() {
    assert_eq!(
        standing(true, Some(&placed("74bca39")), &pin("b11429")),
        PinStanding::Older
    );
    // The same build number with another commit is not the pin either.
    assert_eq!(
        standing(
            true,
            Some(&placed("master-948-1234567")),
            &pin("master-948-228c707")
        ),
        PinStanding::Older
    );
}

/// An override naming llama.cpp's `v0.7.0` is installed as the `b<n>` that
/// release names, which only GitHub knows: the two are matched by name or
/// not placed at all, never called older.
#[test]
fn a_target_with_no_build_number_is_matched_by_name_alone() {
    assert_eq!(
        standing(true, Some(&placed("v0.7.0")), &pin("v0.7.0")),
        PinStanding::AtPin
    );
    assert_eq!(
        standing(true, Some(&placed("b11669")), &pin("v0.7.0")),
        PinStanding::Unknown
    );
    assert_eq!(
        standing(true, Some(&placed("74bca39")), &pin("v0.7.0")),
        PinStanding::Unknown
    );
}

#[test]
fn nothing_to_compare_is_unknown() {
    // A binary with no record: put there by hand.
    assert_eq!(standing(true, None, &pin("b11429")), PinStanding::Unknown);
    // Following upstream's newest, which offline cannot be known.
    assert_eq!(
        standing(true, Some(&placed("b11429")), &ReleaseSelector::Latest),
        PinStanding::Unknown
    );
}

#[test]
fn only_missing_and_older_want_an_install() {
    let wanting: Vec<PinStanding> = [
        PinStanding::Missing,
        PinStanding::AtPin,
        PinStanding::Older,
        PinStanding::Newer,
        PinStanding::Unknown,
    ]
    .into_iter()
    .filter(|standing| standing.wants_install())
    .collect();
    assert_eq!(wanting, [PinStanding::Missing, PinStanding::Older]);
}

/// A shallow checkout of one commit with `tags` on it, the way a clone of
/// master that a release had been tagged on looks; the commit's full hash.
#[cfg(unix)]
fn checkout_with_tags(dir: &Path, tags: &[&str]) -> String {
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "tag.gpgsign=false",
                "-C",
            ])
            .arg(dir)
            .args(args)
            .output()
            .expect("git runs");
        assert!(output.status.success(), "git {args:?}");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    };
    git(&["init", "-q"]);
    git(&["commit", "-q", "--allow-empty", "-m", "head"]);
    for tag in tags {
        git(&["tag", tag]);
    }
    git(&["rev-parse", "HEAD"])
}

fn download(release: &str) -> Recorded {
    Recorded {
        release: release.to_owned(),
        commit_sha: None,
    }
}

fn build(release: &str, commit_sha: &str) -> Recorded {
    Recorded {
        release: release.to_owned(),
        commit_sha: Some(commit_sha.to_owned()),
    }
}

#[test]
fn a_download_is_the_release_its_record_names() {
    assert_eq!(
        installed_release(&download("b11429"), Path::new("/nowhere")),
        placed("b11429")
    );
}

/// A build recorded by tag is that tag, though its checkout's commit also
/// carries another, as a `v*` release's commit carries its `b<n>`.
#[cfg(unix)]
#[test]
fn a_build_recorded_by_tag_is_that_tag() {
    let tmp = tempfile::tempdir().unwrap();
    let sha = checkout_with_tags(tmp.path(), &["v0.7.0", "b11669"]);
    assert_eq!(
        installed_release(&build("v0.7.0", &sha), tmp.path()),
        placed("v0.7.0")
    );
    assert_eq!(
        installed_release(&build("b11669", &sha), tmp.path()),
        placed("b11669")
    );
}

#[cfg(unix)]
#[test]
fn a_source_build_recorded_by_commit_is_the_release_its_commit_is_tagged() {
    let tmp = tempfile::tempdir().unwrap();
    let sha = checkout_with_tags(tmp.path(), &["v0.8.0", "b11870"]);
    let short = &sha[..7];
    assert_eq!(
        installed_release(&build(short, &sha), tmp.path()),
        placed("b11870")
    );
}

#[cfg(unix)]
#[test]
fn a_source_build_with_no_release_tag_is_its_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let sha = checkout_with_tags(tmp.path(), &["v0.8.0"]);
    let short = &sha[..7];
    assert_eq!(
        installed_release(&build(short, &sha), tmp.path()),
        commit(short)
    );
    // And with no checkout at all.
    assert_eq!(
        installed_release(&build(short, &sha), &tmp.path().join("gone")),
        commit(short)
    );
}

/// A short commit that happens to look like a tag (`b123456`) is still a
/// commit: the record's full commit starts with it, so it is never read as
/// build 123456.
#[test]
fn a_commit_that_reads_like_a_release_is_never_compared_as_one() {
    let sha = "b123456aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let installed = installed_release(&build("b123456", sha), Path::new("/nowhere"));
    assert_eq!(installed, commit("b123456"));
    assert_eq!(
        standing(true, Some(&installed), &pin("b11429")),
        PinStanding::Older
    );
    assert_eq!(
        standing(
            true,
            Some(&commit("master-948-228c707")),
            &pin("master-948-228c707")
        ),
        PinStanding::Older
    );
    assert_eq!(
        standing(true, Some(&commit("v0.7.0")), &pin("v0.7.0")),
        PinStanding::Unknown
    );
}
