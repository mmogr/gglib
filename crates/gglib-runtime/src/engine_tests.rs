//! Where an engine stands against its pin, and how a status says so.

use std::path::Path;

use super::{PinStanding, PinState, pin_state_at, standing_line};
use crate::binary_install::{Recorded, ReleaseSelector};

fn download(release: &str) -> Recorded {
    Recorded {
        release: release.to_owned(),
        commit_sha: None,
    }
}

fn pin(tag: &str) -> ReleaseSelector {
    ReleaseSelector::Tag(tag.to_owned())
}

/// A file standing in for an installed server.
fn binary(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("server");
    std::fs::write(&path, "x").unwrap();
    path
}

#[test]
fn a_download_of_an_earlier_release_is_older() {
    let tmp = tempfile::tempdir().unwrap();
    let state = pin_state_at(
        &binary(tmp.path()),
        Some(&download("b10327")),
        &tmp.path().join("checkout"),
        &pin("b11429"),
    );
    assert_eq!(
        state,
        PinState {
            standing: PinStanding::Older,
            installed_release: Some("b10327".to_owned()),
            target_release: Some("b11429".to_owned()),
            built: Some(false),
        }
    );
}

#[test]
fn no_binary_is_missing_even_with_a_record() {
    let tmp = tempfile::tempdir().unwrap();
    let state = pin_state_at(
        &tmp.path().join("server"),
        Some(&download("b11429")),
        &tmp.path().join("checkout"),
        &pin("b11429"),
    );
    assert_eq!(state.standing, PinStanding::Missing);
}

/// A folder where the server should be is not a server.
#[test]
fn a_directory_at_the_binary_path_is_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("server");
    std::fs::create_dir_all(&dir).unwrap();
    let state = pin_state_at(&dir, None, &tmp.path().join("checkout"), &pin("b11429"));
    assert_eq!(state.standing, PinStanding::Missing);
}

#[test]
fn a_binary_with_no_record_is_unknown() {
    let tmp = tempfile::tempdir().unwrap();
    let state = pin_state_at(
        &binary(tmp.path()),
        None,
        &tmp.path().join("checkout"),
        &pin("b11429"),
    );
    assert_eq!(
        state,
        PinState {
            standing: PinStanding::Unknown,
            installed_release: None,
            target_release: Some("b11429".to_owned()),
            built: None,
        }
    );
}

#[test]
fn following_upstream_has_no_target_and_is_unknown() {
    let tmp = tempfile::tempdir().unwrap();
    let state = pin_state_at(
        &binary(tmp.path()),
        Some(&download("b11429")),
        &tmp.path().join("checkout"),
        &ReleaseSelector::Latest,
    );
    assert_eq!(state.standing, PinStanding::Unknown);
    assert_eq!(state.target_release, None);
}

fn state(standing: PinStanding, installed: Option<&str>, target: Option<&str>) -> PinState {
    PinState {
        standing,
        installed_release: installed.map(str::to_owned),
        target_release: target.map(str::to_owned),
        built: Some(false),
    }
}

#[test]
fn a_status_says_where_the_install_stands() {
    let pin = Some("b11429");
    assert_eq!(standing_line(&state(PinStanding::Missing, None, pin)), None);
    assert_eq!(
        standing_line(&state(PinStanding::AtPin, pin, pin)).as_deref(),
        Some("Pinned release: b11429, which is the one installed.")
    );
    assert_eq!(
        standing_line(&state(PinStanding::Older, Some("b10327"), pin)).as_deref(),
        Some("Pinned release: b11429. The one installed, b10327, is older.")
    );
    assert_eq!(
        standing_line(&state(PinStanding::Newer, Some("b11870"), pin)).as_deref(),
        Some("Pinned release: b11429. The one installed, b11870, is newer.")
    );
    assert_eq!(
        standing_line(&state(PinStanding::Unknown, None, pin)).as_deref(),
        Some("Pinned release: b11429. There is no record of what is installed.")
    );
    assert_eq!(
        standing_line(&state(PinStanding::Unknown, Some("b11429"), None)).as_deref(),
        Some("Following upstream's newest release, which is not compared offline.")
    );
    assert_eq!(
        standing_line(&state(PinStanding::Unknown, Some("b11669"), Some("v0.7.0"))).as_deref(),
        Some(
            "Pinned release: v0.7.0. The one installed, b11669, cannot be placed against it offline."
        )
    );
}
