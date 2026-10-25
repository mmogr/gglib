//! What the status says of an install, and how it is spelled on the wire.

use super::*;
use crate::engine::PinStanding;

/// The selector of a test: the pin as it stands today.
fn pin() -> ReleaseSelector {
    ReleaseSelector::Tag("b11429".to_owned())
}

fn not_installed() -> PinState {
    PinState {
        standing: PinStanding::Missing,
        installed_release: None,
        target_release: Some("b11429".to_owned()),
        built: None,
    }
}

/// The GUI's `LlamaStatus` in `types/setup.ts` is hand-mirrored, so the
/// casing is a contract rather than an implementation detail.
#[test]
fn status_serialises_as_camel_case() {
    let status = LlamaStatus {
        installed: true,
        binary_path: "/tmp/llama-server".into(),
        config_path: "/tmp/llama-config.json".into(),
        healthy: false,
        health_error: Some("not executable".into()),
        build: None,
        build_error: None,
        prebuilt: None,
        runtime: None,
        pin: not_installed(),
    };

    let json = serde_json::to_value(&status).unwrap();
    assert_eq!(json["binaryPath"], "/tmp/llama-server");
    assert_eq!(json["pin"]["targetRelease"], "b11429");
    assert_eq!(json["configPath"], "/tmp/llama-config.json");
    assert_eq!(json["healthError"], "not executable");
    assert!(json.get("binary_path").is_none());
}

/// The runtime block is a projection, not the core type — serialising
/// `RuntimeCapabilities` directly would put `snake_case` keys inside this
/// camelCase payload, which is what the TS mirror silently tripped over.
#[test]
#[allow(
    clippy::default_trait_access,
    reason = "grandfathered at lint inheritance, #1157"
)]
fn runtime_block_is_camel_case() {
    let caps = gglib_core::domain::RuntimeCapabilities {
        build: Some(9656),
        commit: None,
        version_line: "version: 9656 (deadbee)".into(),
        flags: Default::default(),
    };

    let json = serde_json::to_value(LlamaRuntimeInfo::from(&caps)).unwrap();
    assert_eq!(json["versionLine"], "version: 9656 (deadbee)");
    assert_eq!(json["build"], 9656);
    assert!(json.get("version_line").is_none());
    // Rendered, not structural — no consumer branches on flag names.
    assert!(json["flags"].is_string());
}

/// `gglib config llama status` names a shallow clone's build by its commit,
/// and claims nothing about compensation for one it cannot read.
#[test]
fn the_build_line_names_an_unnumbered_source_build() {
    let label = |banner: &str| {
        LlamaRuntimeInfo::from(&RuntimeCapabilities::from_version_output(banner)).build_label()
    };

    assert_eq!(
        label("version: 0.1.0-dev (build 1, commit 098ce52)"),
        "unnumbered source build at commit 098ce52 \
         (shallow clone; the build number is not the upstream one)"
    );
    assert_eq!(
        label("version: 0.6.0 (build 11429, commit d81235049)"),
        "b11429"
    );
    assert_eq!(label("some custom fork"), "unidentified");
}

/// The status of a binary that is there, with the file one of the two
/// installs wrote beside it. The binary is not a program, so the status stops
/// at validating it, after the record is read.
///
/// Asking is all a start does with the file, so it is checked here that the
/// file is left as it was: the same bytes, and never written again.
fn status_beside(record: &str) -> LlamaStatus {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("llama-server");
    std::fs::write(&binary, "not a program").unwrap();
    let config = dir.path().join("llama-config.json");
    std::fs::write(&config, record).unwrap();
    // Dated long past, so that a write would show.
    let written = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000);
    std::fs::File::options()
        .write(true)
        .open(&config)
        .unwrap()
        .set_modified(written)
        .unwrap();

    let status = status_at(&binary, &config, &dir.path().join("llama.cpp"), &pin());

    assert_eq!(std::fs::read_to_string(&config).unwrap(), record);
    assert_eq!(
        std::fs::metadata(&config).unwrap().modified().unwrap(),
        written
    );
    status
}

/// A download's record is where the install came from. It used to be
/// reported as a build record that would not parse.
#[test]
fn a_pre_built_install_is_reported_as_one_and_its_file_is_left_alone() {
    let record = r#"{
  "version": "b10327",
  "platform": "macOS ARM64 (Metal)",
  "install_type": "prebuilt",
  "installed_at": "2026-10-01T09:08:07.654321+00:00"
}"#;

    let status = status_beside(record);

    assert!(status.installed);
    assert_eq!(status.build_error, None);
    assert!(status.build.is_none());
    let json = serde_json::to_value(&status).unwrap();
    assert_eq!(json["prebuilt"]["version"], "b10327");
    assert_eq!(json["prebuilt"]["platform"], "macOS ARM64 (Metal)");
    assert_eq!(
        json["prebuilt"]["installedAt"],
        "2026-10-01T09:08:07.654321+00:00"
    );
}

#[test]
fn a_source_build_is_reported_as_one_and_its_file_is_left_alone() {
    let record = r#"{
  "version": "abc1234",
  "commit_sha": "abc1234def5678",
  "build_date": "2026-08-10T12:34:56Z",
  "acceleration": "Metal",
  "cmake_flags": ["-DGGML_METAL=ON"]
}"#;

    let status = status_beside(record);

    assert_eq!(status.build_error, None);
    assert!(status.prebuilt.is_none());
    let build = status.build.expect("the build's record");
    assert_eq!(build.version, "abc1234");
    assert_eq!(build.acceleration, "Metal");
}

#[test]
fn build_info_carries_both_notions_of_version() {
    let info = LlamaBuildInfo::from(BuildConfig {
        version: "abc1234".into(),
        commit_sha: "abc1234def".into(),
        build_date: chrono::Utc::now(),
        acceleration: "Metal".into(),
        cmake_flags: vec!["-DGGML_METAL=ON".into()],
    });

    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(json["version"], "abc1234");
    assert_eq!(json["commitSha"], "abc1234def");
    assert_eq!(json["cmakeFlags"][0], "-DGGML_METAL=ON");
    // RFC 3339 rather than a locale-formatted string: the GUI parses it.
    assert!(json["buildDate"].as_str().unwrap().contains('T'));
}

/// The status says where the install stands against the pin, read from the
/// same record it reports.
#[test]
fn the_status_compares_the_recorded_release_with_the_pin() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("llama-server");
    std::fs::write(&binary, "not a program").unwrap();
    let config = dir.path().join("llama-config.json");
    InstallRecord::Prebuilt(PrebuiltRecord::new("b10327", "macOS ARM64 (Metal)"))
        .save(&config)
        .unwrap();

    let status = status_at(&binary, &config, &dir.path().join("llama.cpp"), &pin());

    assert_eq!(
        status.pin,
        PinState {
            standing: PinStanding::Older,
            installed_release: Some("b10327".to_owned()),
            target_release: Some("b11429".to_owned()),
            built: Some(false),
        }
    );
}
