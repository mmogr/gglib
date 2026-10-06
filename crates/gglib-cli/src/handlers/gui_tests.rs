//! Tests for [`super`]: where `gglib gui` finds the app on each system and
//! the command it runs to open it. The system is an argument, so every row
//! runs on whichever one runs the tests.

use std::ffi::OsString;
use std::path::Path;

use super::*;

/// An empty file at `root/relative`, with the directories above it.
fn made(root: &Path, relative: &str) -> PathBuf {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("its directory");
    std::fs::write(&path, b"stub").expect("the file");
    path
}

/// A command's program and its arguments.
fn line(command: &Command) -> (OsString, Vec<OsString>) {
    (
        command.get_program().to_owned(),
        command.get_args().map(ToOwned::to_owned).collect(),
    )
}

/// The command line `gglib gui` runs, for each system, from an install (the
/// app beside the binary) and from a checkout (the app in the build output):
/// macOS hands the bundle to `open`, and Linux and Windows run the artifact.
#[test]
fn each_system_launches_its_own_artifact_from_an_install_and_from_a_checkout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    for built in [
        "GGLib GUI.app",
        "gglib-app",
        "gglib-app.exe",
        "target/release/bundle/macos/GGLib GUI.app",
        "target/release/gglib-app",
        "target/release/gglib-app.exe",
    ] {
        made(root, built);
    }
    let run = |artifact: &str| (root.join(artifact).into_os_string(), Vec::new());
    let open = |bundle: &str| {
        (
            OsString::from("open"),
            vec![root.join(bundle).into_os_string()],
        )
    };

    for (os, installed, in_checkout) in [
        (
            Os::Mac,
            open("GGLib GUI.app"),
            open("target/release/bundle/macos/GGLib GUI.app"),
        ),
        (Os::Linux, run("gglib-app"), run("target/release/gglib-app")),
        (
            Os::Windows,
            run("gglib-app.exe"),
            run("target/release/gglib-app.exe"),
        ),
    ] {
        let sibling = sibling_artifact(os, root).expect("the app is beside the binary");
        let built = repo_artifact(os, root);

        assert_eq!(line(&launch_command(os, &sibling)), installed, "{os:?}");
        assert_eq!(line(&launch_command(os, &built)), in_checkout, "{os:?}");
    }
}

/// An install with no app beside the binary has nothing to launch, on any
/// system: what follows is the line that names `gglib web`.
#[test]
fn an_install_without_the_app_has_nothing_to_launch() {
    let dir = tempfile::tempdir().expect("tempdir");
    made(dir.path(), "gglib");

    for os in [Os::Mac, Os::Linux, Os::Windows] {
        assert_eq!(sibling_artifact(os, dir.path()), None, "{os:?}");
    }
}

#[test]
fn a_linux_install_launches_the_appimage_beside_the_binary() {
    let dir = tempfile::tempdir().expect("tempdir");
    let appimage = made(dir.path(), "GGLib GUI_0.2.4_amd64.AppImage");

    assert_eq!(sibling_artifact(Os::Linux, dir.path()), Some(appimage));
}

#[test]
fn linux_gui_artifact_prefers_any_appimage() {
    let dir = tempfile::tempdir().expect("tempdir");
    let appimage = made(
        dir.path(),
        "target/release/bundle/appimage/GGLib GUI_0.2.4_amd64.AppImage",
    );
    made(dir.path(), "target/release/gglib-app");

    assert_eq!(repo_artifact(Os::Linux, dir.path()), appimage);
}

#[test]
fn linux_gui_artifact_falls_back_to_binary_when_no_appimage() {
    let dir = tempfile::tempdir().expect("tempdir");

    let chosen = repo_artifact(Os::Linux, dir.path());

    assert_eq!(chosen, dir.path().join("target/release/gglib-app"));
}

/// The Windows lookup must name the executable with its extension. Before
/// this arm existed, `gglib gui` on Windows printed "Desktop GUI is not
/// included in this release" even with `gglib-app.exe` sitting beside it,
/// because there was no Windows branch at all.
#[test]
fn windows_repo_artifact_is_the_exe() {
    let root = Path::new("C:\\repo");
    let chosen = repo_artifact(Os::Windows, root);

    assert_eq!(chosen, root.join("target/release/gglib-app.exe"));
    assert!(
        chosen.to_string_lossy().ends_with(".exe"),
        "{} should name a Windows executable",
        chosen.display()
    );
}

/// A checkout with nothing built is told where the app was expected, and on
/// Linux where an `AppImage` would also have been taken from.
#[test]
fn a_checkout_with_nothing_built_is_told_where_the_app_was_expected() {
    let root = Path::new("/repo");
    let told = |os| not_found_line(os, root, &repo_artifact(os, root));

    assert_eq!(
        told(Os::Mac),
        format!(
            "Desktop GUI not found at: {}",
            root.join("target/release/bundle/macos/GGLib GUI.app")
                .display()
        )
    );
    assert_eq!(
        told(Os::Linux),
        format!(
            "Desktop GUI not found at: {} (or any *.AppImage in {})",
            root.join("target/release/gglib-app").display(),
            root.join("target/release/bundle/appimage").display()
        )
    );
    assert_eq!(
        told(Os::Windows),
        format!(
            "Desktop GUI not found at: {}",
            root.join("target/release/gglib-app.exe").display()
        )
    );
}

/// The system the rules are applied for is the one this binary runs on.
#[test]
fn the_system_is_the_one_this_binary_was_built_for() {
    let built_for = match std::env::consts::OS {
        "macos" => Some(Os::Mac),
        "linux" => Some(Os::Linux),
        "windows" => Some(Os::Windows),
        _ => None,
    };

    assert_eq!(Os::current(), built_for);
}

/// A Linux artifact that is there and cannot be run says how to fix it; the
/// same failure on Windows is passed on as it came.
#[cfg(unix)]
#[test]
fn a_linux_artifact_that_is_not_executable_names_the_chmod_that_fixes_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let artifact = made(dir.path(), "gglib-app");

    let on_linux = launch(Os::Linux, &artifact).expect_err("not executable");
    let on_windows = launch(Os::Windows, &artifact).expect_err("not executable");

    let hint = format!("try: chmod +x \"{}\"", artifact.display());
    assert!(on_linux.to_string().contains(&hint), "{on_linux}");
    assert!(!on_windows.to_string().contains("chmod"), "{on_windows}");
}
