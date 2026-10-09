//! What an update request writes onto a row: on a model in memory, and
//! through [`ModelOps::update`] into the stored one.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use gglib_core::NewModel;
use gglib_core::domain::ServerConfig;
use gglib_core::ports::{NoopEmitter, NoopGgufParser, NoopModelRuntime};

use super::*;
use crate::models::{ModelDeps, ModelOps};
use crate::test_support::test_core;

/// A row with every field a request can write already holding a value.
fn row() -> Model {
    let mut new = NewModel::new(
        "qwen".to_owned(),
        PathBuf::from("/models/qwen.gguf"),
        7.0,
        chrono::Utc::now(),
    );
    new.architecture = Some("qwen3".to_owned());
    new.quantization = Some("Q8_0".to_owned());
    new.context_length = Some(4096);
    new.metadata = HashMap::from([("general.name".to_owned(), "Qwen".to_owned())]);
    new.inference_defaults = Some(sampling(0.2, Some(40)));
    new.defaults_origin = Some(DefaultsOrigin::AutoDetected);
    new.server_defaults = Some(ServerConfig::default());
    Model::stored(1, &new)
}

fn sampling(temperature: f32, top_k: Option<i32>) -> InferenceConfig {
    InferenceConfig {
        temperature: Some(temperature),
        top_k,
        ..InferenceConfig::default()
    }
}

/// The row as JSON, so two rows compare whole.
fn json(model: &Model) -> serde_json::Value {
    serde_json::to_value(model).expect("a model serializes")
}

#[test]
fn a_request_that_names_nothing_leaves_the_row_as_it_was() {
    let before = row();
    let mut model = before.clone();

    UpdateModelRequest::default().apply_to(&mut model);

    assert_eq!(json(&model), json(&before));
}

#[test]
fn each_field_a_request_names_is_written() {
    let mut model = row();
    let metadata = HashMap::from([("note".to_owned(), "mine".to_owned())]);

    UpdateModelRequest {
        name: Some("renamed".to_owned()),
        quantization: Some("Q4_K_M".to_owned()),
        file_path: Some("/models/moved.gguf".to_owned()),
        param_count_b: Some(13.0),
        architecture: Some("mistral".to_owned()),
        context_length: Some(8192),
        metadata: Some(metadata.clone()),
        inference_defaults: Some(sampling(0.9, None)),
        server_defaults: Some(None),
        projector_path: None,
        components: None,
    }
    .apply_to(&mut model);

    assert_eq!(model.name, "renamed");
    assert_eq!(model.quantization.as_deref(), Some("Q4_K_M"));
    assert_eq!(model.file_path, PathBuf::from("/models/moved.gguf"));
    assert!((model.param_count_b - 13.0).abs() < f64::EPSILON);
    assert_eq!(model.architecture.as_deref(), Some("mistral"));
    assert_eq!(model.context_length, Some(8192));
    assert_eq!(model.metadata, metadata, "the map is replaced whole");
    assert_eq!(model.inference_defaults, Some(sampling(0.9, None)));
    assert_eq!(model.server_defaults, None, "an explicit null clears");
}

/// Defaults a person sends are theirs from then on, whoever wrote the ones
/// they replace.
#[test]
fn sampling_defaults_a_request_carries_are_user_set() {
    let mut model = row();

    UpdateModelRequest {
        inference_defaults: Some(sampling(0.2, Some(40))),
        ..UpdateModelRequest::default()
    }
    .apply_to(&mut model);

    assert_eq!(model.defaults_origin, Some(DefaultsOrigin::User));
}

/// An empty config is no config: it clears the defaults, and their origin
/// with them.
#[test]
fn an_empty_sampling_config_returns_the_model_to_inherit() {
    let mut model = row();

    UpdateModelRequest {
        inference_defaults: Some(InferenceConfig::default()),
        ..UpdateModelRequest::default()
    }
    .apply_to(&mut model);

    assert_eq!(model.inference_defaults, None);
    assert_eq!(
        model.defaults_origin, None,
        "no value left to have an origin"
    );
}

/// The projector link is `ModelOps::link_projector`'s to write, after it has
/// read the file.
#[test]
fn the_projector_link_is_not_written_by_the_merge() {
    let mut model = row();

    UpdateModelRequest {
        projector_path: Some(Some("/models/mmproj.gguf".to_owned())),
        ..UpdateModelRequest::default()
    }
    .apply_to(&mut model);

    assert_eq!(model.projector_path, None);
}

/// An image model's component links are `ModelOps::link_components`'s to
/// write, after it has read each file.
#[test]
fn the_component_links_are_not_written_by_the_merge() {
    let mut model = row();

    UpdateModelRequest {
        components: Some(BTreeMap::from([(
            ComponentRole::Vae,
            Some("/models/ae.safetensors".to_owned()),
        )])),
        ..UpdateModelRequest::default()
    }
    .apply_to(&mut model);

    assert!(model.components.is_empty());
}

/// The wire names a component by its role: a path, `null`, or the key left
/// out; a role that is no role's name is refused by name.
#[test]
fn components_are_read_by_role_and_an_unknown_role_is_refused() {
    let request: UpdateModelRequest =
        serde_json::from_str(r#"{"components": {"vae": "/m/ae.safetensors", "clip_l": null}}"#)
            .unwrap();
    assert_eq!(
        request.components,
        Some(BTreeMap::from([
            (ComponentRole::Vae, Some("/m/ae.safetensors".to_owned())),
            (ComponentRole::ClipL, None),
        ]))
    );
    let absent: UpdateModelRequest = serde_json::from_str(r#"{"name": "x"}"#).unwrap();
    assert_eq!(absent.components, None);

    let unknown = serde_json::from_str::<UpdateModelRequest>(r#"{"components": {"vea": null}}"#)
        .unwrap_err()
        .to_string();
    assert!(unknown.contains("vea"), "{unknown}");
}

/// The inspector clears a model's defaults by sending `{}`. What is stored
/// is a model that inherits, not an empty user-set row.
#[tokio::test]
async fn the_inspectors_empty_defaults_are_stored_as_inherit() {
    let core = test_core().await;
    let seeded = core
        .models()
        .add({
            let mut new = NewModel::new(
                "qwen".to_owned(),
                PathBuf::from("/models/qwen.gguf"),
                7.0,
                chrono::Utc::now(),
            );
            new.inference_defaults = Some(sampling(0.2, Some(40)));
            new.defaults_origin = Some(DefaultsOrigin::User);
            new
        })
        .await
        .expect("the model is stored");
    let ops = ModelOps::new(ModelDeps {
        core: Arc::clone(&core),
        runtime: Arc::new(NoopModelRuntime),
        gguf_parser: Arc::new(NoopGgufParser),
        emitter: Arc::new(NoopEmitter::new()),
    });
    let cleared: UpdateModelRequest =
        serde_json::from_str(r#"{"inferenceDefaults": {}}"#).expect("the inspector's body");

    let answered = ops.update(seeded.id, cleared).await.expect("the update");

    let stored = core
        .models()
        .get_by_id(seeded.id)
        .await
        .expect("the row reads")
        .expect("the row is there");
    assert_eq!(stored.inference_defaults, None);
    assert_eq!(stored.defaults_origin, None);
    assert_eq!(answered.inference_defaults, None);
    assert_eq!(answered.defaults_origin, None);
}

/// The fields `gglib model update` can set travel under the names the row is
/// read back by.
#[test]
fn the_wire_names_are_the_ones_a_model_is_read_by() {
    let request: UpdateModelRequest = serde_json::from_str(
        r#"{"paramCountB": 13.0, "architecture": "mistral", "contextLength": 8192,
            "metadata": {"note": "mine"}}"#,
    )
    .expect("the body parses");

    assert_eq!(request.param_count_b, Some(13.0));
    assert_eq!(request.architecture.as_deref(), Some("mistral"));
    assert_eq!(request.context_length, Some(8192));
    assert_eq!(
        request.metadata,
        Some(HashMap::from([("note".to_owned(), "mine".to_owned())]))
    );
}
