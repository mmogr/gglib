//! Where an engine stands against its pin, and how a status says so.

use std::path::{Path, PathBuf};

use gglib_core::domain::RuntimeKind;

use super::{
    EnginePaths, InstallPlan, PinStanding, PinState, calls_for_download, pin_state_at, plan_with,
    standing_line, uninstall_from,
};
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
            update_downloads: false,
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
            update_downloads: false,
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
        update_downloads: false,
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
fn macos() -> (String, Option<String>) {
    ("macOS ARM64 (Metal)".to_owned(), None)
}

#[test]
fn an_install_downloads_where_this_machine_has_a_download() {
    assert_eq!(
        plan_with(&rebuilding(false), false, false, || Ok(macos())),
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
        plan_with(&rebuilding(true), true, false, || Ok(macos())),
        InstallPlan::Download { .. }
    ));
}

fn built_by_commit() -> PinState {
    PinState {
        built: Some(true),
        rebuilds: false,
        update_downloads: false,
        ..state(PinStanding::Older, Some("74bca39"), Some("b11429"))
    }
}

/// A machine whose only download runs on the CPU, with its warning.
fn cpu_only() -> (String, Option<String>) {
    ("Linux x64 (CPU)".to_owned(), Some("slow".to_owned()))
}

/// A source build made for a GPU, recorded before builds were of a release,
/// is not traded for the CPU-only download an update would otherwise take.
#[test]
fn a_source_build_is_never_traded_for_a_cpu_only_download_unasked() {
    assert_eq!(
        plan_with(&built_by_commit(), false, false, || Ok(cpu_only())),
        InstallPlan::Build { no_download: None }
    );
    // A GPU download is the default it takes.
    assert!(matches!(
        plan_with(&built_by_commit(), false, false, || Ok(macos())),
        InstallPlan::Download { .. }
    ));
    // Forced, the download is taken, warning and all.
    assert_eq!(
        plan_with(&built_by_commit(), true, false, || Ok(cpu_only())),
        InstallPlan::Download {
            description: "Linux x64 (CPU)".to_owned(),
            warning: Some("slow".to_owned()),
        }
    );
}

/// The flag answers for a missing engine's install and an older one's
/// update, and the plan decides whether that is a download: not where there
/// is none, nor for a source build left behind with no binary.
#[test]
fn a_missing_or_older_engine_calls_for_a_download_where_the_plan_takes_one() {
    let missing = PinState {
        built: None,
        ..state(PinStanding::Missing, None, Some("b11429"))
    };
    assert!(calls_for_download(&missing, || Ok(macos())));
    assert!(!calls_for_download(&missing, || Err("none".to_owned())));
    let left_behind = PinState {
        standing: PinStanding::Missing,
        ..built_by_commit()
    };
    assert!(!calls_for_download(&left_behind, || Ok(cpu_only())));
    let older = state(PinStanding::Older, Some("b10327"), Some("b11429"));
    assert!(calls_for_download(&older, || Ok(macos())));
    for standing in [PinStanding::AtPin, PinStanding::Newer, PinStanding::Unknown] {
        let installed = state(standing, Some("b11429"), Some("b11429"));
        assert!(
            !calls_for_download(&installed, || Ok(macos())),
            "{standing:?}"
        );
    }
}

/// An engine's folder as an install leaves it: the binary, the record, a
/// checkout and a new one an unfinished build left; and what `installed`
/// names of it.
fn installed_in(engine: &Path) -> Vec<PathBuf> {
    std::fs::create_dir_all(engine.join("bin")).unwrap();
    std::fs::create_dir_all(engine.join("llama.cpp.new")).unwrap();
    std::fs::create_dir_all(engine.join("llama.cpp")).unwrap();
    std::fs::write(engine.join("bin").join("llama-server"), b"x").unwrap();
    std::fs::write(engine.join("llama-config.json"), b"{}").unwrap();
    [
        "bin",
        "llama-config.json",
        "llama.cpp",
        "llama.cpp.new",
        "llama.cpp.old",
    ]
    .map(|name| engine.join(name))
    .to_vec()
}

#[test]
fn uninstalling_removes_what_gglib_installed_and_the_folder_it_empties() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let engine = tmp.path().join(".llama");
    let installed = installed_in(&engine);

    let outcome = uninstall_from(&engine, &installed).unwrap();

    assert!(outcome.was_installed);
    assert_eq!(
        outcome.removed_paths,
        installed[..4]
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
    );
    assert!(!engine.exists(), "the folder it emptied goes too");
    assert!(tmp.path().exists());

    let again = uninstall_from(&engine, &installed).unwrap();
    assert!(!again.was_installed);
    assert!(again.removed_paths.is_empty());
}

/// With the data folder at home, the engine's folder is `~/.llama`, which
/// Meta's `llama` tools keep checkpoints in. Those are not gglib's: they
/// stay, and so does the folder that holds them.
#[test]
fn uninstalling_keeps_what_another_tool_put_in_the_folder() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let engine = tmp.path().join(".llama");
    let installed = installed_in(&engine);
    let theirs = engine.join("checkpoints").join("Llama3.2-1B");
    std::fs::create_dir_all(&theirs).unwrap();
    std::fs::write(theirs.join("consolidated.00.pth"), b"weights").unwrap();

    let outcome = uninstall_from(&engine, &installed).unwrap();

    assert!(outcome.was_installed);
    assert!(theirs.join("consolidated.00.pth").exists());
    for path in &installed {
        assert!(!path.exists(), "{}", path.display());
    }

    // With nothing of gglib's there, nothing is installed and nothing goes.
    let again = uninstall_from(&engine, &installed).unwrap();
    assert!(!again.was_installed);
    assert!(theirs.join("consolidated.00.pth").exists());

    // Nor does a folder that holds nothing at all: gglib did not empty it.
    let empty = tmp.path().join(".sd");
    std::fs::create_dir(&empty).unwrap();
    let none = uninstall_from(&empty, &[empty.join("bin")]).unwrap();
    assert!(!none.was_installed);
    assert!(empty.exists());
}

/// A path on the list that is a link is removed as a link: what it points
/// at, outside the engine's folder, stays.
#[cfg(unix)]
#[test]
fn uninstalling_removes_a_link_and_not_what_it_points_at() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let elsewhere = tmp.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("llama-server"), b"x").unwrap();
    let engine = tmp.path().join(".llama");
    std::fs::create_dir_all(&engine).unwrap();
    std::os::unix::fs::symlink(&elsewhere, engine.join("bin")).unwrap();

    let outcome = uninstall_from(&engine, &[engine.join("bin")]).unwrap();

    assert!(outcome.was_installed);
    assert!(
        engine.join("bin").symlink_metadata().is_err(),
        "the link went"
    );
    assert!(elsewhere.join("llama-server").exists(), "its target stayed");
}

/// Everything an engine installs is inside its own folder and on its list,
/// so an uninstall that removes each listed path leaves nothing of the
/// install, and neither engine's folder holds the other's.
#[test]
fn each_engine_keeps_what_it_installs_in_a_folder_of_its_own() {
    gglib_core::paths::isolate_data_root();
    let llama = EnginePaths::of(RuntimeKind::Llama).unwrap();
    let sd = EnginePaths::of(RuntimeKind::StableDiffusion).unwrap();
    for paths in [&llama, &sd] {
        let named = [&paths.binary, &paths.record, &paths.checkout];
        for inside in named.into_iter().chain(&paths.installed) {
            assert!(
                inside.starts_with(&paths.dir),
                "{} is outside {}",
                inside.display(),
                paths.dir.display()
            );
        }
    }
    assert!(!llama.dir.starts_with(&sd.dir) && !sd.dir.starts_with(&llama.dir));
    for left in crate::binary_install::leftovers(&llama.checkout) {
        assert!(llama.installed.contains(&left), "{}", left.display());
    }
    assert!(
        sd.installed
            .contains(&crate::sd::sd_download_dir().unwrap())
    );
}

/// The keys the daemon's removal answers with, which the app reads.
#[test]
fn an_uninstall_answers_in_the_apps_keys() {
    let outcome = super::UninstallOutcome {
        was_installed: true,
        removed_paths: vec!["/data/.sd/bin".to_owned()],
    };
    assert_eq!(
        serde_json::to_value(&outcome).unwrap(),
        serde_json::json!({ "wasInstalled": true, "removedPaths": ["/data/.sd/bin"] })
    );
}
