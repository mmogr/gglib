//! `llama-config.json`: the two shapes on users' disks, and what is written
//! now.

use super::*;
use chrono::TimeZone;
use tempfile::tempdir;

/// A source build's file, key for key as one on disk holds them.
const BUILT: &str = r#"{
  "version": "abc1234",
  "commit_sha": "abc1234def5678",
  "build_date": "2026-08-10T12:34:56.789012Z",
  "acceleration": "Metal",
  "cmake_flags": [
    "-DGGML_METAL=ON"
  ]
}"#;

/// A pre-built download's file, as the installer wrote it before this type
/// read it.
const PREBUILT: &str = r#"{
  "version": "b10327",
  "platform": "macOS ARM64 (Metal)",
  "install_type": "prebuilt",
  "installed_at": "2026-10-01T09:08:07.654321+00:00"
}"#;

fn file(contents: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("llama-config.json");
    fs::write(&path, contents).unwrap();
    (dir, path)
}

#[test]
fn a_source_builds_file_reads_to_every_value_it_holds() {
    let (_dir, path) = file(BUILT);

    let InstallRecord::Built(config) = InstallRecord::load(&path).unwrap() else {
        panic!("a source build's file is a build's record");
    };

    assert_eq!(config.version, "abc1234");
    assert_eq!(config.commit_sha, "abc1234def5678");
    assert_eq!(
        config.build_date,
        Utc.with_ymd_and_hms(2026, 8, 10, 12, 34, 56).unwrap()
            + chrono::Duration::microseconds(789_012)
    );
    assert_eq!(config.acceleration, "Metal");
    assert_eq!(config.cmake_flags, ["-DGGML_METAL=ON"]);
}

#[test]
fn a_downloads_file_reads_to_every_value_it_holds() {
    let (_dir, path) = file(PREBUILT);

    let InstallRecord::Prebuilt(record) = InstallRecord::load(&path).unwrap() else {
        panic!("a download's file is a download's record");
    };

    assert_eq!(
        record,
        PrebuiltRecord {
            version: "b10327".into(),
            platform: "macOS ARM64 (Metal)".into(),
            install_type: "prebuilt".into(),
            installed_at: "2026-10-01T09:08:07.654321+00:00".into(),
        }
    );
}

/// The readers that ask what a build recorded: a download's file is not an
/// error to them, and not a build either.
#[test]
fn only_a_source_builds_file_names_a_build_and_its_acceleration() {
    let (_built_dir, built) = file(BUILT);
    assert_eq!(
        recorded_build(&built).unwrap().map(|c| c.version),
        Some("abc1234".to_string())
    );
    assert_eq!(recorded_acceleration(&built).as_deref(), Some("Metal"));

    let (_prebuilt_dir, prebuilt) = file(PREBUILT);
    assert!(recorded_build(&prebuilt).unwrap().is_none());
    assert_eq!(recorded_acceleration(&prebuilt), None);

    let absent = prebuilt.with_file_name("absent.json");
    assert!(recorded_build(&absent).unwrap().is_none());
    assert_eq!(recorded_acceleration(&absent), None);
}

/// A file that is neither shape is an error to a reader that reports one,
/// and no backend to a launch.
#[test]
fn a_file_of_neither_shape_is_an_error_and_names_no_acceleration() {
    for contents in ["{", "{}", r#"{"version": "abc1234"}"#] {
        let (_dir, path) = file(contents);
        let error = recorded_build(&path).expect_err(contents);
        assert_eq!(error.to_string(), "Failed to parse config file");
        assert_eq!(recorded_acceleration(&path), None, "{contents}");
    }
}

/// Each install writes the keys its shape has always had, so a file written
/// now is one an older gglib reads too, and the launch that follows a source
/// build reads the acceleration that build wrote.
#[test]
fn what_an_install_writes_now_is_what_is_read_back() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("llama-config.json");

    let built = BuildConfig::new("b1234".into(), "abc123def456".into(), Acceleration::Vulkan);
    InstallRecord::Built(built.clone()).save(&path).unwrap();
    assert_eq!(keys(&path), keys_of(BUILT));
    let read = recorded_build(&path).unwrap().expect("a build's record");
    assert_eq!(read.version, built.version);
    assert_eq!(read.commit_sha, built.commit_sha);
    assert_eq!(read.build_date, built.build_date);
    assert_eq!(read.cmake_flags, ["-DGGML_VULKAN=ON"]);
    assert_eq!(recorded_acceleration(&path).as_deref(), Some("Vulkan"));

    let downloaded = PrebuiltRecord::new("b10327", "Windows x64 (Vulkan)");
    InstallRecord::Prebuilt(downloaded.clone())
        .save(&path)
        .unwrap();
    assert_eq!(keys(&path), keys_of(PREBUILT));
    assert_eq!(downloaded.install_type, "prebuilt");
    let InstallRecord::Prebuilt(read) = InstallRecord::load(&path).unwrap() else {
        panic!("a download's record was written");
    };
    assert_eq!(read, downloaded);
    assert_eq!(recorded_acceleration(&path), None);
}

/// Reading is all a start does with the file: every reader leaves both
/// shapes as it found them, the same bytes and never written again.
#[test]
fn reading_a_file_leaves_it_as_it_was() {
    for contents in [BUILT, PREBUILT] {
        let (_dir, path) = file(contents);
        let written = long_ago(&path);

        InstallRecord::load(&path).unwrap();
        recorded_build(&path).unwrap();
        let _ = recorded_acceleration(&path);

        assert_eq!(fs::read_to_string(&path).unwrap(), contents);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), written);
    }
}

/// Date `path` to a time long past, so that a write would show, and return
/// that time.
fn long_ago(path: &std::path::Path) -> std::time::SystemTime {
    let then = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000);
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(then)
        .unwrap();
    then
}

fn keys(path: &std::path::Path) -> Vec<String> {
    keys_of(&fs::read_to_string(path).unwrap())
}

fn keys_of(json: &str) -> Vec<String> {
    let value: serde_json::Value = serde_json::from_str(json).unwrap();
    value.as_object().unwrap().keys().cloned().collect()
}
