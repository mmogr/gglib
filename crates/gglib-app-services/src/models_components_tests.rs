//! Tests for an image model's component links through [`ModelOps::update`],
//! for the picker's choices, and for what the list row and the detail say
//! about them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gglib_core::domain::{ComponentRole, ImageFamily, ModelListQuery, TensorInfo, WeightsFormat};
use gglib_core::ports::{
    GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort, NoopEmitter, NoopModelRuntime,
    TensorTable,
};
use gglib_core::services::ImportMode;
use tempfile::TempDir;

use crate::error::GuiError;
use crate::models::{ModelDeps, ModelOps};
use crate::test_support::test_core;
use crate::types::{AddModelRequest, GuiModel, UpdateModelRequest};

/// Reads a file by its first bytes, as the real parser reads a header and a
/// tensor table: weights that start `flux` are a Flux.1 model's, any other
/// weights chat; a file that starts `vae` holds a Flux VAE's tensors, any
/// other a table with nothing in it.
struct FirstBytesParser;

fn bytes(path: &Path) -> Result<Vec<u8>, GgufParseError> {
    std::fs::read(path).map_err(|e| GgufParseError::Io(e.to_string()))
}

impl GgufParserPort for FirstBytesParser {
    fn parse(&self, file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        let image_family = bytes(file_path)?
            .starts_with(b"flux")
            .then_some(ImageFamily::Flux1);
        Ok(GgufMetadata {
            image_family,
            ..GgufMetadata::default()
        })
    }

    fn detect_capabilities(&self, _metadata: &GgufMetadata) -> GgufCapabilities {
        GgufCapabilities::empty()
    }

    fn tensor_table(&self, path: &Path) -> Result<TensorTable, GgufParseError> {
        let tensor = |name: &str, shape: &[u64]| TensorInfo {
            name: name.to_owned(),
            shape: shape.to_vec(),
        };
        let tensors = if bytes(path)?.starts_with(b"vae") {
            vec![
                tensor("decoder.conv_in.weight", &[512, 16, 3, 3]),
                tensor("encoder.conv_in.weight", &[128, 3, 3, 3]),
            ]
        } else {
            Vec::new()
        };
        Ok(TensorTable {
            format: WeightsFormat::Safetensors,
            architecture: None,
            tensors,
        })
    }

    fn tensor_table_of_head(&self, _head: &[u8]) -> Result<TensorTable, GgufParseError> {
        Err(GgufParseError::InvalidFormat(
            "no head is read here".to_owned(),
        ))
    }
}

async fn ops() -> ModelOps {
    ModelOps::new(ModelDeps {
        core: test_core().await,
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

async fn add(ops: &ModelOps, dir: &TempDir, name: &str, bytes: &[u8]) -> GuiModel {
    let file_path = file(dir, name, bytes).to_string_lossy().into_owned();
    ops.add(AddModelRequest { file_path }, None, ImportMode::Fresh)
        .await
        .unwrap()
}

fn request(json: &str) -> UpdateModelRequest {
    serde_json::from_str(json).unwrap()
}

/// `{"components": {"vae": <path>}}` links the VAE under its canonical
/// path, and the list row and the detail both stop naming it missing.
#[tokio::test]
async fn an_update_links_a_component_and_the_dtos_say_what_is_still_missing() {
    let ops = ops().await;
    let dir = tempfile::tempdir().unwrap();
    let model = add(&ops, &dir, "flux1-schnell-q8_0.gguf", b"flux").await;
    let vae = file(&dir, "ae.safetensors", b"vae");
    assert_eq!(model.image_family, Some(ImageFamily::Flux1));
    assert_eq!(
        model.missing_components,
        [
            ComponentRole::Vae,
            ComponentRole::ClipL,
            ComponentRole::T5xxl
        ]
    );

    // Asked by a spelling that is not the canonical one.
    let spelled = dir.path().join(".").join("ae.safetensors");
    let body = serde_json::json!({ "components": { "vae": spelled } }).to_string();
    let updated = ops.update(model.id, request(&body)).await.unwrap();

    assert_eq!(
        updated.missing_components,
        [ComponentRole::ClipL, ComponentRole::T5xxl]
    );
    let listed = ops
        .list_with_query(ModelListQuery::default())
        .await
        .unwrap();
    assert_eq!(listed[0].missing_components, updated.missing_components);
    let detail = ops.get_detail(model.id).await.unwrap();
    assert_eq!(detail.image_family, Some(ImageFamily::Flux1));
    assert_eq!(detail.components.len(), 1);
    assert_eq!(detail.components[0].role, ComponentRole::Vae);
    assert_eq!(
        detail.components[0].path.as_deref(),
        Some(vae.to_string_lossy().as_ref())
    );
    assert!(detail.components[0].present);
    assert_eq!(
        detail.missing_components,
        [ComponentRole::ClipL, ComponentRole::T5xxl]
    );
}

/// The three states on the wire: an absent key and an absent role leave a
/// link, `null` for the role clears it.
#[tokio::test]
async fn an_absent_role_keeps_its_link_and_null_clears_it() {
    let ops = ops().await;
    let dir = tempfile::tempdir().unwrap();
    let model = add(&ops, &dir, "flux.gguf", b"flux").await;
    let vae = file(&dir, "ae.safetensors", b"vae");
    let body = serde_json::json!({ "components": { "vae": vae } }).to_string();
    ops.update(model.id, request(&body)).await.unwrap();

    let renamed = ops
        .update(model.id, request(r#"{"name": "Renamed"}"#))
        .await
        .unwrap();
    assert!(!renamed.missing_components.contains(&ComponentRole::Vae));
    let other_role = ops
        .update(model.id, request(r#"{"components": {}}"#))
        .await
        .unwrap();
    assert!(!other_role.missing_components.contains(&ComponentRole::Vae));

    let cleared = ops
        .update(model.id, request(r#"{"components": {"vae": null}}"#))
        .await
        .unwrap();
    assert!(cleared.missing_components.contains(&ComponentRole::Vae));
    assert!(
        ops.get_detail(model.id)
            .await
            .unwrap()
            .components
            .is_empty()
    );
}

/// A file whose tensors are not the role's is refused in `set_component`'s
/// words, before any other field of the update is written.
#[tokio::test]
async fn a_file_that_is_not_the_role_is_refused_and_nothing_is_written() {
    let ops = ops().await;
    let dir = tempfile::tempdir().unwrap();
    let model = add(&ops, &dir, "flux.gguf", b"flux").await;
    let clip = file(&dir, "clip_l.safetensors", b"clip");
    let body = serde_json::json!({ "name": "Renamed", "components": { "vae": clip } }).to_string();

    let refused = ops.update(model.id, request(&body)).await.unwrap_err();

    let GuiError::ValidationFailed(message) = refused else {
        panic!("expected a validation failure, got {refused:?}");
    };
    assert!(message.contains("clip_l.safetensors"), "{message}");
    assert!(message.contains("is not a vae"), "{message}");
    let stored = ops.get(model.id).await.unwrap();
    assert_eq!(stored.name, model.name);
    assert!(stored.missing_components.contains(&ComponentRole::Vae));
}

/// A model that chats takes no component, and the refusal says why.
#[tokio::test]
async fn a_chat_model_is_refused_a_component() {
    let ops = ops().await;
    let dir = tempfile::tempdir().unwrap();
    let model = add(&ops, &dir, "qwen.gguf", b"weights").await;
    let vae = file(&dir, "ae.safetensors", b"vae");
    assert_eq!(model.image_family, None);
    assert!(model.missing_components.is_empty());
    let body = serde_json::json!({ "components": { "vae": vae } }).to_string();

    let refused = ops.update(model.id, request(&body)).await.unwrap_err();

    let GuiError::ValidationFailed(message) = refused else {
        panic!("expected a validation failure, got {refused:?}");
    };
    assert!(message.contains("not an image model"), "{message}");
}

#[tokio::test]
async fn a_component_for_an_unknown_model_is_not_found() {
    let ops = ops().await;

    let refused = ops
        .update(999, request(r#"{"components": {"vae": null}}"#))
        .await
        .unwrap_err();

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

/// The VAE one Flux model links is offered to another, by path and name,
/// under its role; a chat model's picker has no roles.
#[tokio::test]
async fn the_picker_offers_a_component_another_model_of_the_family_links() {
    let ops = ops().await;
    let dir = tempfile::tempdir().unwrap();
    let linked = add(&ops, &dir, "flux-a.gguf", b"flux").await;
    let other = add(&ops, &dir, "flux-b.gguf", b"flux").await;
    let chat = add(&ops, &dir, "qwen.gguf", b"weights").await;
    let vae = file(&dir, "ae.safetensors", b"vae");
    let body = serde_json::json!({ "components": { "vae": vae } }).to_string();
    ops.update(linked.id, request(&body)).await.unwrap();

    let choices = ops.component_choices(other.id).await.unwrap();

    let roles: Vec<_> = choices.iter().map(|choice| choice.role).collect();
    assert_eq!(
        roles,
        [
            ComponentRole::Vae,
            ComponentRole::ClipL,
            ComponentRole::T5xxl
        ]
    );
    assert_eq!(choices[0].files.len(), 1);
    assert_eq!(choices[0].files[0].name, "ae.safetensors");
    assert_eq!(choices[0].files[0].path, vae.to_string_lossy());
    assert!(choices[1].files.is_empty());
    assert!(ops.component_choices(chat.id).await.unwrap().is_empty());
    assert!(matches!(
        ops.component_choices(999).await,
        Err(GuiError::NotFound { .. })
    ));
}
