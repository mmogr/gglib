//! Where an engine stands against its pin, and how a status says so.

use std::path::Path;

use super::{InstallPlan, PinStanding, PinState, pin_state_at, plan_with, standing_line};
use crate::binary_install::{Recorded, ReleaseSelector};

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
            rebuilds: false,
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
            rebuilds: false,
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
        rebuilds: false,
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

/// A source build of a release is built again; a download, or a source build
/// recorded by commit from before builds were of a release, is not.
#[test]
fn only_a_source_build_of_a_release_rebuilds() {
    let tmp = tempfile::tempdir().unwrap();
    let server = binary(tmp.path());
    let checkout = tmp.path().join("checkout");
    let sha = "d81235049384534c167caea52b85a694f6103d14";
    let rebuilds = |recorded: Recorded| {
        pin_state_at(&server, Some(&recorded), &checkout, &pin("b11429")).rebuilds
    };
    assert!(rebuilds(build("b11429", sha)));
    assert!(!rebuilds(build("d812350", sha)));
    assert!(!rebuilds(download("b11429")));
}

fn rebuilding(rebuilds: bool) -> PinState {
    PinState {
        rebuilds,
        ..state(PinStanding::Older, Some("b10327"), Some("b11429"))
    }
}

/// A machine with a Metal download, as the platform table answers.
#[allow(
    clippy::unnecessary_wraps,
    reason = "the shape plan_with asks this machine's download in"
)]
fn macos() -> Result<(String, Option<String>), String> {
    Ok(("macOS ARM64 (Metal)".to_owned(), None))
}

#[test]
fn an_install_downloads_where_this_machine_has_a_download() {
    assert_eq!(
        plan_with(&rebuilding(false), false, false, macos),
        InstallPlan::Download {
            description: "macOS ARM64 (Metal)".to_owned(),
            warning: None,
        }
    );
    let cpu = || Ok(("Linux x64 (CPU)".to_owned(), Some("slow".to_owned())));
    assert_eq!(
        plan_with(&rebuilding(false), false, false, cpu),
        InstallPlan::Download {
            description: "Linux x64 (CPU)".to_owned(),
            warning: Some("slow".to_owned()),
        }
    );
}

#[test]
fn an_install_builds_where_this_machine_has_no_download_and_says_why() {
    assert_eq!(
        plan_with(&rebuilding(false), false, false, || Err(
            "no arm64 build".to_owned()
        )),
        InstallPlan::Build {
            no_download: Some("no arm64 build".to_owned()),
        }
    );
}

/// Asked for, or what is installed: the machine's download is not consulted.
#[test]
fn a_build_asked_for_or_kept_never_asks_for_a_download() {
    let never = || -> Result<(String, Option<String>), String> { panic!("asked for a download") };
    let built = InstallPlan::Build { no_download: None };
    assert_eq!(plan_with(&rebuilding(false), false, true, never), built);
    assert_eq!(plan_with(&rebuilding(true), false, false, never), built);
    assert_eq!(plan_with(&rebuilding(true), true, true, never), built);
}

/// `--force` without `--build` takes the default over a kept source build.
#[test]
fn force_without_build_takes_the_download_over_a_source_build() {
    assert!(matches!(
        plan_with(&rebuilding(true), true, false, macos),
        InstallPlan::Download { .. }
    ));
}

fn built_by_commit() -> PinState {
    PinState {
        built: Some(true),
        rebuilds: false,
        ..state(PinStanding::Older, Some("74bca39"), Some("b11429"))
    }
}

#[allow(
    clippy::unnecessary_wraps,
    reason = "the shape plan_with asks this machine's download in"
)]
fn cpu_only() -> Result<(String, Option<String>), String> {
    Ok(("Linux x64 (CPU)".to_owned(), Some("slow".to_owned())))
}

/// A source build made for a GPU, recorded before builds were of a release,
/// is not traded for the CPU-only download an update would otherwise take.
#[test]
fn a_source_build_is_never_traded_for_a_cpu_only_download_unasked() {
    assert_eq!(
        plan_with(&built_by_commit(), false, false, cpu_only),
        InstallPlan::Build { no_download: None }
    );
    // A GPU download is the default it takes.
    assert!(matches!(
        plan_with(&built_by_commit(), false, false, macos),
        InstallPlan::Download { .. }
    ));
    // Forced, the download is taken, warning and all.
    assert_eq!(
        plan_with(&built_by_commit(), true, false, cpu_only),
        InstallPlan::Download {
            description: "Linux x64 (CPU)".to_owned(),
            warning: Some("slow".to_owned()),
        }
    );
}
