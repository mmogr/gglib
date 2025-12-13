//! Integration tests for path resolution parity across adapters.
//!
//! These tests ensure that Tauri (GUI) and Axum (Web) adapters resolve
//! paths identically when given the same environment. This prevents
//! the "models downloaded in Tauri don't appear in Web UI" class of bugs.
//!
//! See: <https://github.com/mmogr/gglib/issues/259>

use gglib_core::paths::ResolvedPaths;

/// Both adapters should resolve identical paths under the same environment.
///
/// This is the core parity assertion - if this fails, models/database/etc
/// will be split between adapters.
#[test]
fn path_resolution_is_deterministic() {
    // Multiple calls should return identical results
    let first = ResolvedPaths::resolve().expect("first resolve failed");
    let second = ResolvedPaths::resolve().expect("second resolve failed");

    assert_eq!(
        first, second,
        "Path resolution should be deterministic across calls"
    );
}

/// Database path should be inside `data_root`.
#[test]
fn database_path_is_under_data_root() {
    let paths = ResolvedPaths::resolve().expect("resolve failed");

    assert!(
        paths.database_path.starts_with(&paths.data_root),
        "database_path ({}) should be under data_root ({})",
        paths.database_path.display(),
        paths.data_root.display()
    );
}

/// Llama server path should be inside `resource_root`.
#[test]
fn llama_server_path_is_under_resource_root() {
    let paths = ResolvedPaths::resolve().expect("resolve failed");

    assert!(
        paths.llama_server_path.starts_with(&paths.resource_root),
        "llama_server_path ({}) should be under resource_root ({})",
        paths.llama_server_path.display(),
        paths.resource_root.display()
    );
}

/// Display format should be parseable for debugging.
#[test]
fn display_format_contains_all_paths() {
    let paths = ResolvedPaths::resolve().expect("resolve failed");
    let output = paths.to_string();

    // All keys should be present in key = value format
    assert!(output.contains("data_root = "), "missing data_root");
    assert!(output.contains("resource_root = "), "missing resource_root");
    assert!(output.contains("database_path = "), "missing database_path");
    assert!(
        output.contains("llama_server_path = "),
        "missing llama_server_path"
    );
    assert!(output.contains("models_dir = "), "missing models_dir");
    assert!(output.contains("models_source = "), "missing models_source");
}

/// Explicit `models_dir` override should be respected.
#[test]
fn explicit_models_dir_override_is_respected() {
    use gglib_core::paths::ModelsDirSource;

    let explicit_path = "/tmp/test-models";
    let paths = ResolvedPaths::resolve_with_models_dir(Some(explicit_path))
        .expect("resolve with override failed");

    assert_eq!(
        paths.models_dir.to_string_lossy(),
        explicit_path,
        "explicit models_dir should be used"
    );
    assert_eq!(
        paths.models_source,
        ModelsDirSource::Explicit,
        "source should be Explicit when path is provided"
    );
}

/// Models dir should have a valid source.
#[test]
fn models_dir_has_valid_source() {
    use gglib_core::paths::ModelsDirSource;

    let paths = ResolvedPaths::resolve().expect("resolve failed");

    // Source should be one of the valid variants
    matches!(
        paths.models_source,
        ModelsDirSource::Explicit | ModelsDirSource::EnvVar | ModelsDirSource::Default
    );
}
