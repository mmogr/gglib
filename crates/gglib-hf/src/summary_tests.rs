//! The rules a model's summary is read by, whichever endpoint answered.

use serde_json::{Value, json};

use super::*;

pub(crate) fn tags(tags: &[&str]) -> Vec<String> {
    tags.iter().map(|tag| (*tag).to_string()).collect()
}

/// What the browser shows of a summary: every field but the chat template,
/// which its card has no place for.
#[derive(Debug, PartialEq)]
pub(crate) struct Shown {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) author: Option<String>,
    pub(crate) downloads: u64,
    pub(crate) likes: u64,
    pub(crate) last_modified: Option<String>,
    pub(crate) parameters_b: Option<f64>,
    pub(crate) description: Option<String>,
    pub(crate) tags: Vec<String>,
}

pub(crate) fn shown(info: HfRepoInfo) -> Shown {
    Shown {
        id: info.model_id,
        name: info.name,
        author: info.author,
        downloads: info.downloads,
        likes: info.likes,
        last_modified: info.last_modified,
        parameters_b: info.parameters_b,
        description: info.description,
        tags: info.tags,
    }
}

#[test]
fn a_model_object_with_every_field_is_read_whole() {
    let model = repo_info_from_json(&json!({
        "id": "TheBloke/Llama-2-7B-GGUF",
        "downloads": 50000,
        "likes": 42,
        "lastModified": "2024-01-15T10:30:00Z",
        "gguf": {"total": 7_000_000_000_u64, "chat_template": "{{ messages }}"},
        "tags": ["llama", "gguf"],
        "description": "A fine model for testing"
    }))
    .expect("a summary");

    assert_eq!(model.chat_template.as_deref(), Some("{{ messages }}"));
    assert_eq!(
        shown(model),
        Shown {
            id: "TheBloke/Llama-2-7B-GGUF".to_string(),
            name: "Llama-2-7B-GGUF".to_string(),
            author: Some("TheBloke".to_string()),
            downloads: 50_000,
            likes: 42,
            last_modified: Some("2024-01-15T10:30:00Z".to_string()),
            parameters_b: Some(7.0),
            description: Some("A fine model for testing".to_string()),
            tags: tags(&["llama", "gguf"]),
        }
    );
}

/// The count in the GGUF header is the one read when a response has it, and
/// the two a safetensors repository gives are read, in that order, when it
/// has not.
#[test]
fn the_parameter_count_is_the_gguf_headers_before_any_other() {
    let counted = |counts: Value| {
        let mut model = json!({"id": "o/r"});
        model
            .as_object_mut()
            .unwrap()
            .extend(counts.as_object().unwrap().clone());
        repo_info_from_json(&model).unwrap().parameters_b
    };
    let (gguf, safetensors, config) = (
        json!({"total": 4_000_000_000_u64}),
        json!({"total": 8_000_000_000_u64}),
        json!({"num_parameters": 2_000_000_000_u64}),
    );

    let every = json!({"gguf": gguf, "safetensors": safetensors, "config": config});
    assert_eq!(counted(every), Some(4.0));
    assert_eq!(
        counted(json!({"safetensors": safetensors, "config": config})),
        Some(8.0)
    );
    assert_eq!(counted(json!({"config": config})), Some(2.0));
    assert_eq!(counted(json!({})), None);
}

/// The description is the object's own, and the card's summary when it has
/// none.
#[test]
fn the_description_is_the_objects_own_before_the_cards_summary() {
    let described = |model: Value| repo_info_from_json(&model).unwrap().description;

    assert_eq!(
        described(json!({
            "id": "o/r",
            "description": "its own",
            "cardData": {"model_summary": "the card's"}
        }))
        .as_deref(),
        Some("its own")
    );
    assert_eq!(
        described(json!({"id": "o/r", "cardData": {"model_summary": "the card's"}})).as_deref(),
        Some("the card's")
    );
    assert_eq!(described(json!({"id": "o/r"})), None);
}

/// A description of 200 bytes is whole, and a longer one is cut to 200: its
/// first 197 and `...`.
#[test]
fn a_long_description_is_cut_to_200_bytes() {
    let described = |text: String| {
        repo_info_from_json(&json!({"id": "o/r", "description": text}))
            .unwrap()
            .description
            .unwrap()
    };

    assert_eq!(described("a".repeat(200)), "a".repeat(200));
    assert_eq!(
        described("a".repeat(201)),
        format!("{}...", "a".repeat(197))
    );
    // From the card's summary too: a lookup by ID is cut as a search hit is.
    let from_the_card = repo_info_from_json(&json!({
        "id": "o/r",
        "cardData": {"model_summary": "b".repeat(300)}
    }));
    assert_eq!(
        from_the_card.unwrap().description.unwrap(),
        format!("{}...", "b".repeat(197))
    );
}

/// A cut that would land inside a character is made before it. Byte 197 of
/// this description is the second byte of an `é`: a slice there panics.
#[test]
fn a_description_is_never_cut_inside_a_character() {
    let text = format!("{}{}", "a".repeat(196), "é".repeat(10));
    assert!(!text.is_char_boundary(197));

    let model = repo_info_from_json(&json!({"id": "o/r", "description": text}));

    assert_eq!(
        model.unwrap().description.unwrap(),
        format!("{}...", "a".repeat(196))
    );
}

#[test]
fn an_object_that_names_no_id_is_no_summary() {
    assert!(repo_info_from_json(&json!({"downloads": 1000, "likes": 10})).is_none());
    assert!(repo_info_from_json(&json!({"id": "", "downloads": 1000})).is_none());
}

/// A search page keeps the hits that list a `.gguf` file, in the Hub's order.
#[test]
fn a_search_hit_without_a_gguf_file_is_dropped() {
    let hits = [
        json!({"id": "Org/First-GGUF", "siblings": [{"rfilename": "model.Q4_K_M.GGUF"}]}),
        json!({
            "id": "meta-llama/Llama-3.1-8B",
            "siblings": [{"rfilename": "model.safetensors"}, {"rfilename": "config.json"}]
        }),
        json!({"id": "Org/No-Siblings"}),
        json!({"siblings": [{"rfilename": "model.gguf"}]}),
        json!({"id": "Org/Second-GGUF", "siblings": [{"rfilename": "model.gguf"}]}),
    ];

    let ids: Vec<String> = search_hits(&hits).into_iter().map(|h| h.model_id).collect();

    assert_eq!(ids, ["Org/First-GGUF", "Org/Second-GGUF"]);
}
