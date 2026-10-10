//! Tests for the projector link through [`ModelOps::update`] and for the
//! picker's choices.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gglib_core::GgufFileRole;
use gglib_core::domain::{ModelListQuery, NewModelFile};
use gglib_core::ports::{
    GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort, NoopEmitter, NoopModelRuntime,
    TensorTable,
};
use gglib_core::services::{AppCore, ImportMode};
use gglib_db::{CoreFactory, setup_test_database};
use tempfile::TempDir;

use super::*;
use crate::models::ModelDeps;
use crate::test_support::test_core;
use crate::types::{AddModelRequest, GuiModel};

/// Reads a file's role from its first bytes, as the real parser reads it from
/// the header: `projector` is one, anything else is a model's weights.
struct FirstBytesParser;

impl GgufParserPort for FirstBytesParser {
    fn parse(&self, file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        let bytes = std::fs::read(file_path).map_err(|e| GgufParseError::Io(e.to_string()))?;
        let role = if bytes.starts_with(b"projector") {
            GgufFileRole::Projector
        } else {
            GgufFileRole::Weights
        };
        Ok(GgufMetadata {
            role,
            ..GgufMetadata::default()
        })
    }

    fn detect_capabilities(&self, _metadata: &GgufMetadata) -> GgufCapabilities {
        GgufCapabilities::empty()
    }

    fn tensor_table(&self, _path: &Path) -> Result<TensorTable, GgufParseError> {
        Err(GgufParseError::InvalidFormat("no tensor table".to_owned()))
    }
    fn tensor_table_of_head(&self, _head: &[u8]) -> Result<TensorTable, GgufParseError> {
        Err(GgufParseError::InvalidFormat("no tensor table".to_owned()))
    }
}

async fn ops() -> ModelOps {
    ops_over(test_core().await)
}

fn ops_over(core: Arc<AppCore>) -> ModelOps {
    ModelOps::new(ModelDeps {
        core,
        runtime: Arc::new(NoopModelRuntime),
        gguf_parser: Arc::new(FirstBytesParser),
        emitter: Arc::new(NoopEmitter::new()),
    })
}

/// Writes `name` in `dir` holding `bytes`, and answers its canonical path.
fn file(dir: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    path.canonicalize().unwrap()
}

async fn add(ops: &ModelOps, dir: &TempDir, name: &str) -> GuiModel {
    let file_path = file(dir, name, b"weights").to_string_lossy().into_owned();
    ops.add(AddModelRequest { file_path }, None, ImportMode::Fresh)
        .await
        .unwrap()
}

fn link(path: &Path) -> UpdateModelRequest {
    UpdateModelRequest {
        projector_path: Some(Some(path.to_string_lossy().into_owned())),
        ..Default::default()
    }
}

#[tokio::test]
async fn a_new_model_does_not_read_images() {
    let ops = ops().await;
    let dir = tempfile::tempdir().unwrap();
    let model = add(&ops, &dir, "qwen.gguf").await;

    assert!(!model.image_input);
    let detail = ops.get_detail(model.id).await.unwrap();
    assert!(!detail.image_input);
    assert_eq!(detail.projector_path, None);
}

#[tokio::test]
async fn an_update_links_the_projector_and_every_dto_says_the_model_reads_images() {
    let ops = ops().await;
    let dir = tempfile::tempdir().unwrap();
    let model = add(&ops, &dir, "qwen.gguf").await;
    let projector = file(&dir, "mmproj-F16.gguf", b"projector");

    let updated = ops.update(model.id, link(&projector)).await.unwrap();

    assert!(updated.image_input);
    assert!(ops.get(model.id).await.unwrap().image_input);
    let listed = ops.list_with_query(ModelListQuery::default()).await;
    assert!(listed.unwrap()[0].image_input);
    let detail = ops.get_detail(model.id).await.unwrap();
    assert!(detail.image_input);
    assert_eq!(
        detail.projector_path.as_deref(),
        Some(projector.to_string_lossy().as_ref())
    );
}

/// The three states of the key on the wire: a path links, an absent key
/// leaves the link, `null` clears it.
#[tokio::test]
async fn an_absent_key_leaves_the_link_and_null_clears_it() {
    let ops = ops().await;
    let dir = tempfile::tempdir().unwrap();
    let model = add(&ops, &dir, "qwen.gguf").await;
    let projector = file(&dir, "mmproj-F16.gguf", b"projector");
    ops.update(model.id, link(&projector)).await.unwrap();

    let renamed: UpdateModelRequest = serde_json::from_str(r#"{"name": "Renamed"}"#).unwrap();
    let kept = ops.update(model.id, renamed).await.unwrap();
    assert_eq!(kept.name, "Renamed");
    assert!(
        kept.image_input,
        "an update that names no projector unlinked"
    );

    let clear: UpdateModelRequest = serde_json::from_str(r#"{"projectorPath": null}"#).unwrap();
    let cleared = ops.update(model.id, clear).await.unwrap();
    assert!(!cleared.image_input);
    let detail = ops.get_detail(model.id).await.unwrap();
    assert_eq!(detail.projector_path, None);
}

/// The refusal is `set_projector`'s, by name, and it comes before any other
/// field of the same update is written.
#[tokio::test]
async fn a_weights_file_is_refused_and_the_update_writes_nothing() {
    let ops = ops().await;
    let dir = tempfile::tempdir().unwrap();
    let model = add(&ops, &dir, "qwen.gguf").await;
    let other = file(&dir, "other.gguf", b"weights");

    let refused = ops
        .update(
            model.id,
            UpdateModelRequest {
                name: Some("Renamed".to_owned()),
                ..link(&other)
            },
        )
        .await
        .unwrap_err();

    let GuiError::ValidationFailed(message) = refused else {
        panic!("expected a validation failure, got {refused:?}");
    };
    assert!(message.contains("other.gguf"), "{message}");
    assert!(message.contains("not a projector"), "{message}");
    let stored = ops.get(model.id).await.unwrap();
    assert_eq!(stored.name, model.name);
    assert!(!stored.image_input);
}

#[tokio::test]
async fn linking_an_unknown_model_is_not_found() {
    let ops = ops().await;
    let dir = tempfile::tempdir().unwrap();
    let projector = file(&dir, "mmproj-F16.gguf", b"projector");

    let refused = ops.update(999, link(&projector)).await.unwrap_err();

    assert!(
        matches!(
            refused,
            GuiError::NotFound {
                entity: "model",
                ..
            }
        ),
        "{refused:?}"
    );
}

/// A projector one model loads is offered to every other, by path and name.
#[tokio::test]
async fn the_picker_offers_a_projector_another_model_loads() {
    let ops = ops().await;
    let dir = tempfile::tempdir().unwrap();
    let linked = add(&ops, &dir, "qwen-a.gguf").await;
    let other = add(&ops, &dir, "qwen-b.gguf").await;
    let projector = file(&dir, "mmproj-F16.gguf", b"projector");
    assert!(ops.projector_choices(other.id).await.unwrap().is_empty());

    ops.update(linked.id, link(&projector)).await.unwrap();

    let expected = vec![ProjectorChoice {
        path: projector.to_string_lossy().into_owned(),
        name: "mmproj-F16.gguf".to_owned(),
    }];
    assert_eq!(ops.projector_choices(other.id).await.unwrap(), expected);
    assert_eq!(ops.projector_choices(linked.id).await.unwrap(), expected);
}

/// A projector downloaded with a model is one of that model's file rows:
/// the picker offers it though no model is linked to it.
#[tokio::test]
async fn the_picker_offers_a_projector_among_the_models_own_files() {
    gglib_core::paths::isolate_data_root();
    let repos = CoreFactory::build_repos(setup_test_database().await.unwrap());
    let ops = ops_over(Arc::new(AppCore::bare(repos.clone())));
    let dir = tempfile::tempdir().unwrap();
    let model = add(&ops, &dir, "qwen.gguf").await;
    let projector = file(&dir, "mmproj-F16.gguf", b"projector");
    let row = NewModelFile::new(model.id, "mmproj-F16.gguf".to_owned(), 1, 9, None);
    repos.model_files.insert(&row).await.unwrap();

    let offered = ops.projector_choices(model.id).await.unwrap();

    let expected = vec![ProjectorChoice {
        path: projector.to_string_lossy().into_owned(),
        name: "mmproj-F16.gguf".to_owned(),
    }];
    assert_eq!(offered, expected);
}

#[tokio::test]
async fn the_picker_for_an_unknown_model_is_not_found() {
    let refused = ops().await.projector_choices(999).await.unwrap_err();

    assert!(
        matches!(
            refused,
            GuiError::NotFound {
                entity: "model",
                ..
            }
        ),
        "{refused:?}"
    );
}
