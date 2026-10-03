use super::*;
use gglib_core::domain::ModelCapabilities;
use gglib_core::settings::DEFAULT_CONTEXT_SIZE;

// =========================================================================
// ModelsResponse tests
// =========================================================================

#[test]
fn models_response_from_empty_summaries() {
    let resp = ModelsResponse::from_summaries(vec![], Some(DEFAULT_CONTEXT_SIZE), true);
    assert_eq!(resp.object, "list");
    assert!(resp.data.is_empty());
}

#[test]
fn models_response_from_summaries_maps_fields() {
    let summaries = vec![
        ModelSummary {
            dialect: None,
            template_caps: None,
            id: 1,
            name: "llama-3-8b-q4".into(),
            tags: vec!["chat".into()],
            capabilities: ModelCapabilities::empty(),
            image_input: false,
            param_count: "8B".into(),
            quantization: Some("Q4_K_M".into()),
            architecture: Some("llama".into()),
            created_at: 1700000000,
            file_size: 4_000_000_000,
            context_length: Some(8192),
            inference_defaults: None,
            defaults_origin: None,
            server_defaults: None,
        },
        ModelSummary {
            dialect: None,
            template_caps: None,
            id: 2,
            name: "mistral-7b-q8".into(),
            tags: vec![],
            capabilities: ModelCapabilities::empty(),
            image_input: false,
            param_count: "7B".into(),
            quantization: Some("Q8_0".into()),
            architecture: Some("mistral".into()),
            created_at: 1700000001,
            file_size: 7_000_000_000,
            context_length: None,
            inference_defaults: None,
            defaults_origin: None,
            server_defaults: None,
        },
    ];

    let resp = ModelsResponse::from_summaries(summaries, Some(DEFAULT_CONTEXT_SIZE), true);
    assert_eq!(resp.data.len(), 2);
    assert_eq!(resp.data[0].id, "llama-3-8b-q4");
    assert_eq!(resp.data[0].object, "model");
    assert_eq!(resp.data[0].owned_by, "gglib");
    assert_eq!(resp.data[0].created, 1700000000);
    assert!(resp.data[0].description.is_some());
}

#[test]
fn models_response_serializes_to_openai_format() {
    let resp = ModelsResponse::from_summaries(
        vec![ModelSummary {
            dialect: None,
            template_caps: None,
            id: 1,
            name: "test-model".into(),
            tags: vec![],
            capabilities: ModelCapabilities::empty(),
            image_input: false,
            param_count: "7B".into(),
            quantization: None,
            architecture: None,
            created_at: 0,
            file_size: 0,
            context_length: None,
            inference_defaults: None,
            defaults_origin: None,
            server_defaults: None,
        }],
        Some(DEFAULT_CONTEXT_SIZE),
        true,
    );

    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["object"], "list");
    assert!(json["data"].is_array());
    assert_eq!(json["data"][0]["id"], "test-model");
    assert_eq!(json["data"][0]["object"], "model");
}

// =========================================================================
// ModelInfo capabilities
// =========================================================================

fn summary_with_tags(name: &str, tags: &[&str]) -> ModelSummary {
    ModelSummary {
        dialect: None,
        template_caps: None,
        id: 1,
        name: name.into(),
        tags: tags.iter().map(|t| (*t).to_string()).collect(),
        capabilities: ModelCapabilities::empty(),
        image_input: false,
        param_count: "1B".into(),
        quantization: None,
        architecture: None,
        created_at: 0,
        file_size: 0,
        context_length: None,
        inference_defaults: None,
        defaults_origin: None,
        server_defaults: None,
    }
}

#[test]
fn an_embedding_tagged_model_advertises_the_embeddings_capability() {
    let resp = ModelsResponse::from_summaries(
        vec![summary_with_tags("bge-small", &["embedding"])],
        Some(DEFAULT_CONTEXT_SIZE),
        true,
    );
    assert_eq!(
        resp.data[0].capabilities.as_deref(),
        Some(["embeddings".to_string()].as_slice())
    );
}

/// A chat model's entry has to stay exactly what it was before this field
/// existed — a picker built from it must not change shape.
#[test]
fn a_chat_model_omits_the_capabilities_field_entirely() {
    let resp = ModelsResponse::from_summaries(
        vec![summary_with_tags("qwen3", &["agent", "reasoning"])],
        Some(DEFAULT_CONTEXT_SIZE),
        true,
    );
    assert!(resp.data[0].capabilities.is_none());

    let json = serde_json::to_value(&resp.data[0]).unwrap();
    assert!(
        json.get("capabilities").is_none(),
        "an absent capability set must not serialize as [] or null: {json}"
    );
}

#[test]
fn an_embedding_models_capabilities_survive_serialization() {
    let resp = ModelsResponse::from_summaries(
        vec![summary_with_tags("bge-small", &["embedding"])],
        Some(DEFAULT_CONTEXT_SIZE),
        true,
    );
    let json = serde_json::to_value(&resp.data[0]).unwrap();
    assert_eq!(json["capabilities"], serde_json::json!(["embeddings"]));
}

/// A model linked to a projector says so, in the one spelling a client reads.
#[test]
fn a_model_that_reads_images_advertises_vision() {
    let sees = ModelSummary {
        image_input: true,
        ..summary_with_tags("qwen3-vl", &["agent"])
    };
    let resp = ModelsResponse::from_summaries(vec![sees], Some(DEFAULT_CONTEXT_SIZE), true);
    assert_eq!(VISION_CAPABILITY, "vision");
    let json = serde_json::to_value(&resp.data[0]).unwrap();
    assert_eq!(json["capabilities"], serde_json::json!(["vision"]));
}

/// Neither capability hides the other.
#[test]
fn an_embedding_model_linked_to_a_projector_advertises_both() {
    let both = ModelSummary {
        image_input: true,
        ..summary_with_tags("colpali", &["embedding"])
    };
    let resp = ModelsResponse::from_summaries(vec![both], Some(DEFAULT_CONTEXT_SIZE), true);
    let json = serde_json::to_value(&resp.data[0]).unwrap();
    assert_eq!(
        json["capabilities"],
        serde_json::json!(["embeddings", "vision"])
    );
}

// =========================================================================
// Catalogue ids and the reading side
// =========================================================================

/// The entry carries the catalogue id beside the name a client sends; a
/// base entry names no profile, and the machine is named by the endpoint.
#[test]
fn an_entry_carries_its_catalogue_id() {
    let mut summary = summary_with_tags("qwen3", &[]);
    summary.id = 42;
    let resp = ModelsResponse::from_summaries(vec![summary], Some(DEFAULT_CONTEXT_SIZE), true);

    assert_eq!(resp.data[0].id, "qwen3");
    assert_eq!(resp.data[0].gglib_id, 42);
    assert_eq!(resp.data[0].profile, None);
    assert_eq!(resp.machine_name, None);

    let json = serde_json::to_value(&resp).unwrap();
    assert_eq!(json["data"][0]["gglib_id"], 42);
    assert!(json["data"][0].get("profile").is_none(), "{json}");
    assert!(json.get("machine_name").is_none(), "{json}");
}

/// The list reads back into the type it was written from, and a list with
/// no catalogue ids — an older build — does not read as one.
#[test]
fn the_list_reads_back_and_an_entry_without_an_id_does_not() {
    let mut resp = ModelsResponse::from_summaries(
        vec![summary_with_tags("qwen3", &[])],
        Some(DEFAULT_CONTEXT_SIZE),
        true,
    );
    resp.machine_name = Some("desk".to_owned());
    let read: ModelsResponse = serde_json::from_value(serde_json::to_value(&resp).unwrap())
        .expect("the list deserializes");
    assert_eq!(read.machine_name.as_deref(), Some("desk"));
    assert_eq!(read.data[0].gglib_id, 1);

    let older = serde_json::json!({
        "object": "list",
        "data": [{ "id": "qwen3", "object": "model", "created": 0, "owned_by": "gglib" }],
    });
    assert!(serde_json::from_value::<ModelsResponse>(older).is_err());
}
