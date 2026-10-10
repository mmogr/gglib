//! What is installed: `sd-config.json`'s two shapes, the status read from
//! it and from the binary, and the uninstall.

use std::path::Path;

use super::record::{SdBuildRecord, SdInstallRecord};
use super::status::{commit_of, sd_status_at};
use super::uninstall::uninstall_sd_at;
use crate::binary_install::PrebuiltRecord;
use crate::llama::Acceleration;

/// The two shapes of `sd-config.json` read back as what was written, each
/// as its own kind, with the keys each has.
#[test]
fn both_record_shapes_round_trip_and_keep_their_keys() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let path = tmp.path().join("sd-config.json");

    let built = SdInstallRecord::Built(SdBuildRecord::new(
        "master-948-228c707".into(),
        "228c707abcdef".into(),
        Acceleration::Metal,
        vec!["-DSD_METAL=ON".into()],
    ));
    built.save(&path).unwrap();
    assert_eq!(SdInstallRecord::load(&path).unwrap(), built);
    assert_eq!(
        keys(&path),
        [
            "acceleration",
            "build_date",
            "cmake_flags",
            "commit_sha",
            "version"
        ]
    );

    let downloaded = SdInstallRecord::Prebuilt(PrebuiltRecord::new(
        "master-948-228c707",
        "macOS universal (Metal)",
    ));
    downloaded.save(&path).unwrap();
    assert_eq!(SdInstallRecord::load(&path).unwrap(), downloaded);
    assert_eq!(
        keys(&path),
        ["install_type", "installed_at", "platform", "version"]
    );
}

fn keys(path: &Path) -> Vec<String> {
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

#[test]
fn nothing_installed_is_a_status_not_an_error() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let status = sd_status_at(
        &tmp.path().join("sd-server"),
        &tmp.path().join("sd-config.json"),
    );
    assert!(!status.installed);
    assert_eq!(status.pinned_release, "master-948-228c707");
    assert_eq!(
        (
            status.install_type,
            status.version_line,
            status.record_error
        ),
        (None, None, None)
    );
}

/// The status reads the record and asks the binary itself; a stand-in
/// script prints what sd-server 228c707 prints for `--version`.
#[cfg(unix)]
#[test]
fn the_status_reads_the_record_and_the_binarys_own_version() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempfile::tempdir().expect("a temp dir");
    let binary = tmp.path().join("sd-server");
    std::fs::write(
        &binary,
        "#!/bin/sh\necho 'stable-diffusion.cpp version master-948-228c707, commit 228c707'\n",
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = tmp.path().join("sd-config.json");
    SdInstallRecord::Prebuilt(PrebuiltRecord::new(
        "master-948-228c707",
        "macOS universal (Metal)",
    ))
    .save(&config)
    .unwrap();

    let status = sd_status_at(&binary, &config);

    assert!(status.installed);
    assert_eq!(status.install_type.as_deref(), Some("prebuilt"));
    assert_eq!(status.release.as_deref(), Some("master-948-228c707"));
    assert_eq!(status.platform.as_deref(), Some("macOS universal (Metal)"));
    assert_eq!(
        status.version_line.as_deref(),
        Some("stable-diffusion.cpp version master-948-228c707, commit 228c707")
    );
    assert_eq!(status.commit.as_deref(), Some("228c707"));
}

#[test]
fn an_unreadable_record_is_reported_not_raised() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let config = tmp.path().join("sd-config.json");
    std::fs::write(&config, "{").unwrap();
    let status = sd_status_at(&tmp.path().join("sd-server"), &config);
    assert_eq!(
        status.record_error.as_deref(),
        Some("Failed to parse config file")
    );
}

#[test]
fn the_commit_is_read_from_the_version_line() {
    assert_eq!(
        commit_of("stable-diffusion.cpp version master-948-228c707, commit 228c707").as_deref(),
        Some("228c707")
    );
    // The line the pinned macOS asset prints (log-0018).
    assert_eq!(
        commit_of("stable-diffusion.cpp version unknown, commit 228c707").as_deref(),
        Some("228c707")
    );
    assert_eq!(commit_of("stable-diffusion.cpp version unknown"), None);
    assert_eq!(commit_of("x, commit "), None);
}

#[test]
fn uninstalling_removes_the_directory_whole_and_says_what_it_removed() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let sd = tmp.path().join(".sd");
    std::fs::create_dir_all(sd.join("bin")).unwrap();
    std::fs::write(sd.join("bin").join("sd-server"), b"x").unwrap();
    std::fs::write(sd.join("sd-config.json"), b"{}").unwrap();

    let outcome = uninstall_sd_at(&sd).unwrap();

    assert!(outcome.was_installed);
    assert_eq!(outcome.removed_paths, [sd.display().to_string()]);
    assert!(!sd.exists());

    let again = uninstall_sd_at(&sd).unwrap();
    assert!(!again.was_installed);
    assert!(again.removed_paths.is_empty());
}
