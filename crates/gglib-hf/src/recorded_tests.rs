//! The Hub's recorded answers, read through the port: a search page and a
//! model info give the same repository the same summary.

use serde_json::json;

use super::*;
use crate::http::testing::{CannedResponse, FakeBackend};
use crate::models::HfConfig;
use crate::parsing::summary_tests::{Shown, shown, tags};

/// The Hub's answer to a search for `phi-4 mini`, sorted by downloads and
/// limited to three, as its API gave it on 2026-10-07.
const SEARCH_PAGE: &str = include_str!("search_fixture.json");

/// The model info of `unsloth/Phi-4-mini-instruct-GGUF`, the second hit of
/// [`SEARCH_PAGE`], as the Hub's API gave it the same day.
const MODEL_INFO: &str = include_str!("model_info_fixture.json");

const UNSLOTH: &str = "unsloth/Phi-4-mini-instruct-GGUF";

/// A client whose Hub answers a search with [`SEARCH_PAGE`] and a lookup of
/// [`UNSLOTH`] with [`MODEL_INFO`].
fn client() -> HfClient<FakeBackend> {
    let answer = |recorded: &str, has_more| CannedResponse {
        json: serde_json::from_str(recorded).expect("a recorded answer is JSON"),
        has_more,
    };
    let backend = FakeBackend::new()
        .with_response("search=", answer(SEARCH_PAGE, true))
        .with_response(&format!("models/{UNSLOTH}"), answer(MODEL_INFO, false));
    HfClient::with_backend(HfConfig::default(), backend)
}

fn unsloth_tags() -> Vec<String> {
    tags(&[
        "transformers",
        "gguf",
        "phi3",
        "text-generation",
        "phi",
        "phi4",
        "unsloth",
        "nlp",
        "code",
        "microsoft",
        "math",
        "chat",
        "conversational",
        "custom_code",
        "multilingual",
        "base_model:microsoft/Phi-4-mini-instruct",
        "base_model:quantized:microsoft/Phi-4-mini-instruct",
        "license:mit",
        "endpoints_compatible",
        "region:us",
    ])
}

/// The fields of each hit of the recorded page are the ones the search
/// parser this function replaced gave for the same page.
#[tokio::test]
async fn a_search_page_gives_each_hit_the_fields_the_browser_showed() {
    let found = HfClientPort::search(&client(), &HfSearchOptions::new().with_query("phi-4 mini"))
        .await
        .expect("a page");

    assert!(found.has_more);
    assert_eq!(found.page, 0);
    let hit = |id: &str, downloads, likes, tags| Shown {
        id: id.to_string(),
        name: id.rsplit('/').next().unwrap().to_string(),
        author: id.split('/').next().map(str::to_string),
        downloads,
        likes,
        last_modified: None,
        parameters_b: Some(3.836_021_856),
        description: None,
        tags,
    };
    assert_eq!(
        found.items.into_iter().map(shown).collect::<Vec<_>>(),
        [
            hit(
                "MaziyarPanahi/Phi-4-mini-instruct-GGUF",
                139_847,
                16,
                tags(&[
                    "gguf",
                    "mistral",
                    "quantized",
                    "2-bit",
                    "3-bit",
                    "4-bit",
                    "5-bit",
                    "6-bit",
                    "8-bit",
                    "GGUF",
                    "text-generation",
                    "base_model:microsoft/Phi-4-mini-instruct",
                    "base_model:quantized:microsoft/Phi-4-mini-instruct",
                    "region:us",
                    "conversational",
                ]),
            ),
            hit(UNSLOTH, 117_114, 153, unsloth_tags()),
            hit(
                "bartowski/microsoft_Phi-4-mini-instruct-GGUF",
                75_435,
                47,
                tags(&[
                    "gguf",
                    "text-generation",
                    "base_model:microsoft/Phi-4-mini-instruct",
                    "base_model:quantized:microsoft/Phi-4-mini-instruct",
                    "endpoints_compatible",
                    "region:us",
                    "imatrix",
                    "conversational",
                ]),
            ),
        ]
    );
}

/// A repository looked up by its ID has the parameter count its search hit
/// has. The lookup's own parser read `safetensors.total` and
/// `config.num_parameters`, which a GGUF repository does not carry, and
/// answered none for this one.
#[tokio::test]
async fn a_repository_looked_up_by_its_id_shows_what_its_search_hit_shows() {
    let client = client();

    let info = HfClientPort::get_model_info(&client, UNSLOTH)
        .await
        .expect("the model info");

    assert_eq!(
        info.chat_template.as_deref().map(str::len),
        Some(398),
        "the template tool support is detected from"
    );
    assert_eq!(
        shown(info),
        Shown {
            id: UNSLOTH.to_string(),
            name: "Phi-4-mini-instruct-GGUF".to_string(),
            author: Some("unsloth".to_string()),
            downloads: 117_114,
            likes: 153,
            last_modified: Some("2025-03-03T00:53:59.000Z".to_string()),
            parameters_b: Some(3.836_021_856),
            description: None,
            tags: unsloth_tags(),
        }
    );
}

/// A model info that names no ID is not a model: the lookup fails, and does
/// not answer a summary under the ID that was asked for.
#[tokio::test]
async fn a_model_info_that_names_no_id_is_an_invalid_response() {
    let backend = FakeBackend::new().with_response(
        "models/o/r",
        CannedResponse {
            json: json!({"downloads": 3, "gguf": {"total": 1}}),
            has_more: false,
        },
    );
    let client = HfClient::with_backend(HfConfig::default(), backend);

    let refused = HfClientPort::get_model_info(&client, "o/r").await;

    assert!(
        matches!(&refused, Err(HfPortError::InvalidResponse { message }) if message.contains("o/r")),
        "{refused:?}"
    );
}

/// Every lookup answers a malformed ID the same way, before asking the Hub.
#[tokio::test]
async fn a_malformed_id_is_refused_by_every_lookup() {
    let client = HfClient::with_backend(HfConfig::default(), FakeBackend::new());
    let malformed = |refused: HfPortError| {
        assert_eq!(
            refused.to_string(),
            "Invalid API response: Invalid model ID format: no-slash"
        );
    };

    malformed(
        HfClientPort::get_model_info(&client, "no-slash")
            .await
            .unwrap_err(),
    );
    malformed(
        HfClientPort::list_quantizations(&client, "no-slash")
            .await
            .unwrap_err(),
    );
    malformed(
        HfClientPort::get_commit_sha(&client, "no-slash")
            .await
            .unwrap_err(),
    );
    malformed(
        HfClientPort::fetch_generation_config(&client, "no-slash")
            .await
            .unwrap_err(),
    );
}
