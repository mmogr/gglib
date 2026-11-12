//! Unit tests for [`super`].

use super::*;
use crate::paths::test_utils::{ENV_LOCK, EnvVarGuard};

#[test]
fn test_default_models_dir_platform_path() {
    let dir = default_models_dir().unwrap();
    let path_str = dir.to_string_lossy();
    // On Windows the path should be under %LOCALAPPDATA% and use native
    // separators throughout — no forward-slash fragments.
    #[cfg(target_os = "windows")]
    {
        assert!(
            path_str.contains("llama_models"),
            "Expected 'llama_models' in path: {path_str}"
        );
        assert!(
            !path_str.contains('/'),
            "Path must not contain forward slashes on Windows: {path_str}"
        );
    }
    // On non-Windows the path should sit under ~/.local/share/llama_models.
    #[cfg(not(target_os = "windows"))]
    assert!(
        path_str.contains(DEFAULT_MODELS_DIR_RELATIVE),
        "Expected '{DEFAULT_MODELS_DIR_RELATIVE}' in path: {path_str}"
    );
}

#[test]
fn test_resolve_models_dir_prefers_explicit() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _env = EnvVarGuard::set("GGLIB_MODELS_DIR", "/tmp/env-value");
    let resolved = resolve_models_dir(Some("/tmp/explicit")).unwrap();
    assert_eq!(resolved.source, ModelsDirSource::Explicit);
    assert!(resolved.path.ends_with("explicit"));
}

#[test]
fn test_resolve_models_dir_env_value() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _env = EnvVarGuard::set("GGLIB_MODELS_DIR", "/tmp/from-env");
    let resolved = resolve_models_dir(None).unwrap();
    assert_eq!(resolved.source, ModelsDirSource::EnvVar);
    assert!(resolved.path.ends_with("from-env"));
}

/// The save that a later run reads back: in a process whose environment
/// names no directory, the stored one is what resolves, and it reads as a
/// configured directory, not the default.
#[test]
fn a_stored_models_directory_is_what_a_later_resolve_returns() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let _data = EnvVarGuard::set("GGLIB_DATA_DIR", temp.path().to_string_lossy().as_ref());
    let _models = EnvVarGuard::unset("GGLIB_MODELS_DIR");
    let chosen = temp.path().join("my models");
    assert_eq!(
        resolve_models_dir(None).unwrap().source,
        ModelsDirSource::Default,
        "nothing is stored yet"
    );

    let stored = set_models_dir(
        chosen.to_string_lossy().as_ref(),
        DirectoryCreationStrategy::AutoCreate,
    )
    .unwrap();

    assert_eq!(stored.path, chosen);
    assert!(chosen.is_dir(), "the directory is created");
    let resolved = resolve_models_dir(None).unwrap();
    assert_eq!(resolved.path, chosen);
    assert_eq!(resolved.source, ModelsDirSource::EnvVar);
}

/// A directory that may not be created is refused before it is stored.
#[test]
fn a_models_directory_that_is_refused_is_not_stored() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let _data = EnvVarGuard::set("GGLIB_DATA_DIR", temp.path().to_string_lossy().as_ref());
    let _models = EnvVarGuard::unset("GGLIB_MODELS_DIR");
    let absent = temp.path().join("absent");

    let refused = set_models_dir(
        absent.to_string_lossy().as_ref(),
        DirectoryCreationStrategy::Disallow,
    );

    assert!(
        matches!(refused, Err(PathError::DirectoryNotFound(_))),
        "{refused:?}"
    );
    assert!(!absent.exists());
    assert_eq!(
        resolve_models_dir(None).unwrap().source,
        ModelsDirSource::Default
    );
}

/// The environment a process was started with outranks the stored
/// directory, as it does for every key a `.env` file holds.
#[test]
fn the_environment_outranks_a_stored_models_directory() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let _data = EnvVarGuard::set("GGLIB_DATA_DIR", temp.path().to_string_lossy().as_ref());
    let _models = EnvVarGuard::set("GGLIB_MODELS_DIR", "/tmp/from-env");
    let chosen = temp.path().join("stored");

    set_models_dir(
        chosen.to_string_lossy().as_ref(),
        DirectoryCreationStrategy::AutoCreate,
    )
    .unwrap();

    assert!(resolve_models_dir(None).unwrap().path.ends_with("from-env"));
}

/// A library that never chose a folder has the default stored, so a later
/// build with another default still finds what was downloaded here.
#[test]
fn the_default_models_directory_is_stored_when_nothing_names_one() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let _data = EnvVarGuard::set("GGLIB_DATA_DIR", temp.path().to_string_lossy().as_ref());
    let _models = EnvVarGuard::unset("GGLIB_MODELS_DIR");

    store_default_models_dir().unwrap();

    let resolved = resolve_models_dir(None).unwrap();
    assert_eq!(resolved.path, default_models_dir().unwrap());
    assert_eq!(
        resolved.source,
        ModelsDirSource::EnvVar,
        "read back as stored"
    );
}

/// A folder the environment names is that process's, not a choice to keep.
#[test]
fn a_models_directory_the_environment_names_is_not_stored() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let _data = EnvVarGuard::set("GGLIB_DATA_DIR", temp.path().to_string_lossy().as_ref());
    let _models = EnvVarGuard::set("GGLIB_MODELS_DIR", "/tmp/from-env");

    store_default_models_dir().unwrap();

    assert!(!temp.path().join(".env").exists());
}

/// A line the loader refuses is left alone: it is the only record of the
/// folder a user chose, written bare by a build before quoting.
#[test]
fn a_stored_line_the_loader_refuses_is_left_as_it_is() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let _data = EnvVarGuard::set("GGLIB_DATA_DIR", temp.path().to_string_lossy().as_ref());
    let _models = EnvVarGuard::unset("GGLIB_MODELS_DIR");
    let env = temp.path().join(".env");
    let line = "GGLIB_MODELS_DIR=/Volumes/My Drive/models\n";
    std::fs::write(&env, line).unwrap();
    assert_eq!(
        resolve_models_dir(None).unwrap().source,
        ModelsDirSource::Default,
        "the loader refuses the bare line"
    );

    store_default_models_dir().unwrap();

    assert_eq!(std::fs::read_to_string(&env).unwrap(), line);
}

/// A stored folder stays the one stored.
#[test]
fn a_stored_models_directory_is_left_as_it_is() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let _data = EnvVarGuard::set("GGLIB_DATA_DIR", temp.path().to_string_lossy().as_ref());
    let _models = EnvVarGuard::unset("GGLIB_MODELS_DIR");
    let chosen = temp.path().join("chosen");
    set_models_dir(
        chosen.to_string_lossy().as_ref(),
        DirectoryCreationStrategy::AutoCreate,
    )
    .unwrap();

    store_default_models_dir().unwrap();

    assert_eq!(resolve_models_dir(None).unwrap().path, chosen);
}

/// Two spellings of one file resolve to one answer. This is the property
/// the model key, the stored column and the duplicate lookup all lean on;
/// if it stops holding, a re-add silently merges two models into one row.
#[test]
fn canonical_model_path_agrees_across_spellings_of_one_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("Model.gguf");
    std::fs::File::create(&file).unwrap();

    let direct = canonical_model_path(&file).unwrap();
    let indirect = canonical_model_path(&dir.path().join(".").join("Model.gguf")).unwrap();

    assert_eq!(direct, indirect);
}

/// The fallible form reports a path it cannot resolve instead of handing
/// back the literal one. A caller that treats "cannot resolve" as "not a
/// duplicate" reinstates the silent overwrite, so the error has to be
/// reachable.
#[test]
fn canonical_model_path_reports_a_path_that_does_not_resolve() {
    let dir = tempfile::tempdir().unwrap();
    assert!(canonical_model_path(&dir.path().join("Absent.gguf")).is_err());
}

/// The string form is the one the database column stores, and it keeps
/// the literal path when the file is gone so an existing row still
/// round-trips.
#[test]
fn canonical_model_path_string_falls_back_to_the_literal_path() {
    let dir = tempfile::tempdir().unwrap();
    let absent = dir.path().join("Absent.gguf");

    assert_eq!(
        canonical_model_path_string(&absent),
        absent.to_string_lossy()
    );
}

/// A repository's files go in one folder named for it, the `/` made `_`, so
/// an owner's name never becomes a directory of its own.
#[test]
fn a_repository_dir_is_its_name_with_the_slash_made_an_underscore() {
    assert_eq!(
        repository_dir(Path::new("/models"), "unsloth/FLUX.1-schnell"),
        Path::new("/models/unsloth_FLUX.1-schnell")
    );
}
