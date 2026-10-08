//! What a core error is to a caller, and that each surface's operation
//! answers with that kind rather than with `Internal`.

use std::sync::Arc;

use gglib_core::CoreError;
use gglib_core::ports::{NoopEmitter, NoopGgufParser, NoopModelRuntime, RepositoryError};
use gglib_core::settings::SettingsError;

use super::*;
use crate::models::{ModelDeps, ModelOps};
use crate::settings::{SettingsDeps, SettingsOps};
use crate::test_support::{MockDownloadManager, MockSystemProbePort, test_core};
use crate::types::UpdateSettingsRequest;

#[test]
fn a_missing_row_is_not_found() {
    let missing = CoreError::Repository(RepositoryError::NotFound("Model with ID 7".to_owned()));

    assert!(
        matches!(
            GuiError::from(missing),
            GuiError::NotFound { ref id, .. } if id == "Model with ID 7"
        ),
        "a missing row is the caller's to hear about, by what was missed"
    );
}

#[test]
fn a_duplicate_is_a_conflict() {
    let duplicate = CoreError::Repository(RepositoryError::AlreadyExists("dup".to_owned()));

    assert!(matches!(
        GuiError::from(duplicate),
        GuiError::Conflict(ref said) if said == "dup"
    ));
}

#[test]
fn a_rejected_input_is_a_validation_failure() {
    let input = CoreError::Validation("no such file".to_owned());
    let setting = CoreError::Settings(SettingsError::InvalidPort(80));
    let constraint = CoreError::Repository(RepositoryError::Constraint("unique".to_owned()));

    assert!(matches!(
        GuiError::from(input),
        GuiError::ValidationFailed(ref said) if said == "no such file"
    ));
    assert!(matches!(
        GuiError::from(setting),
        GuiError::ValidationFailed(ref said) if said.contains("80")
    ));
    assert!(matches!(
        GuiError::from(constraint),
        GuiError::ValidationFailed(ref said) if said == "unique"
    ));
}

#[test]
fn only_a_failure_of_the_store_is_internal() {
    let storage = CoreError::Repository(RepositoryError::Storage("disk".to_owned()));
    let serialization = CoreError::Repository(RepositoryError::Serialization("json".to_owned()));

    assert!(matches!(
        GuiError::from(storage),
        GuiError::Internal(ref said) if said == "Storage error: disk"
    ));
    assert!(matches!(
        GuiError::from(serialization),
        GuiError::Internal(_)
    ));
}

/// A step's name leads the message and the kind survives it, so a refusal
/// reported with context is still a refusal.
#[test]
fn context_leads_the_message_and_keeps_the_kind() {
    let refused = GuiError::from(CoreError::Validation("blank".to_owned())).context("storing");
    let duplicate = GuiError::Conflict("dup".to_owned()).context("adding");
    let broken = GuiError::Internal("disk".to_owned()).context("writing");
    let missing = GuiError::NotFound {
        entity: "model",
        id: "7".to_owned(),
    }
    .context("reading");

    assert!(matches!(refused, GuiError::ValidationFailed(ref said) if said == "storing: blank"));
    assert!(matches!(duplicate, GuiError::Conflict(ref said) if said == "adding: dup"));
    assert!(matches!(broken, GuiError::Internal(ref said) if said == "writing: disk"));
    assert!(matches!(missing, GuiError::NotFound { entity: "model", ref id } if id == "7"));
}

/// A settings update the service refuses is the caller's mistake: over HTTP
/// that is a 400, where `Internal` was a 500.
#[tokio::test]
async fn a_refused_settings_update_is_a_validation_failure() {
    let ops = SettingsOps::new(SettingsDeps {
        core: test_core().await,
        downloads: Arc::new(MockDownloadManager::new()),
        system_probe: Arc::new(MockSystemProbePort::default()),
    });

    let refused = ops
        .update(UpdateSettingsRequest {
            proxy_port: Some(Some(80)),
            ..UpdateSettingsRequest::default()
        })
        .await
        .expect_err("a privileged port is refused");

    assert!(
        matches!(refused, GuiError::ValidationFailed(ref said) if said.contains("80")),
        "{refused:?}"
    );
}

/// A tag for a model that is not there misses in the repository, not in a
/// lookup made first, and is still not found.
#[tokio::test]
async fn a_tag_for_a_missing_model_is_not_found() {
    let ops = ModelOps::new(ModelDeps {
        core: test_core().await,
        runtime: Arc::new(NoopModelRuntime),
        gguf_parser: Arc::new(NoopGgufParser),
        emitter: Arc::new(NoopEmitter::new()),
    });

    let missed = ops
        .add_tag(404, "chat".to_owned())
        .await
        .expect_err("there is no model 404");

    assert!(matches!(missed, GuiError::NotFound { .. }), "{missed:?}");
}

/// A system tag is not the caller's to remove, and saying so is a refusal
/// of the request, not a fault of the daemon.
#[tokio::test]
async fn removing_a_system_tag_is_a_validation_failure() {
    let ops = ModelOps::new(ModelDeps {
        core: test_core().await,
        runtime: Arc::new(NoopModelRuntime),
        gguf_parser: Arc::new(NoopGgufParser),
        emitter: Arc::new(NoopEmitter::new()),
    });
    let tag = "format:qwen-xml".to_owned();
    assert!(gglib_core::domain::is_system_tag(&tag));

    let refused = ops
        .remove_tag(1, tag)
        .await
        .expect_err("a system tag stays");

    assert!(
        matches!(refused, GuiError::ValidationFailed(_)),
        "{refused:?}"
    );
}

/// The GUI shows this path and command as the way out, so they are the path
/// gglib looks for llama-server at and the constant the CLI's own test parses
/// as its install command.
#[test]
fn the_install_prompt_names_the_managed_path_and_the_install_command() {
    let root = gglib_core::paths::isolate_data_root();

    let GuiError::LlamaServerNotInstalled {
        expected_path,
        suggested_command,
        reason,
    } = GuiError::llama_server_not_installed("not found")
    else {
        panic!("the constructor builds the install prompt");
    };

    let managed = gglib_core::paths::llama_server_path().expect("the isolated root resolves");
    assert!(managed.starts_with(root), "{managed:?} is under {root:?}");
    assert_eq!(expected_path, managed.display().to_string());
    assert_eq!(suggested_command, gglib_core::paths::LLAMA_INSTALL_COMMAND);
    assert_eq!(reason, "not found");
}
