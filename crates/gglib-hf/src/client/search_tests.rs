//! The Hub's search: its pages, its filters, and each hit's parameter
//! count checked against the files it lists.

use super::*;
use crate::client::tests::{fake_model_json, test_config};
use crate::http::testing::{CannedResponse, FakeBackend};
use serde_json::json;

/// A search asks the Hub for the page it was asked for, returns every hit
/// of it, and carries on from the cursor the Hub gave.
#[tokio::test]
async fn a_search_pages_by_the_hubs_cursor() {
    let backend = FakeBackend::new()
        .with_response(
            "limit=7",
            CannedResponse {
                json: json!([
                    fake_model_json("Org/Model1-GGUF", 1000),
                    fake_model_json("Org/Model2-GGUF", 2000),
                ]),
                next_cursor: Some("page2".to_string()),
            },
        )
        .with_response(
            "cursor=page2",
            CannedResponse::of(json!([fake_model_json("Org/Model3-GGUF", 500)])),
        );
    let client = HfClient::with_backend(test_config(), backend);
    let ids = |found: &HfSearchResult| -> Vec<String> {
        found.items.iter().map(|m| m.model_id.clone()).collect()
    };

    let first = HfSearchOptions::new().with_query("llama").with_limit(7);
    let first = client.search_models_page(&first).await.unwrap();

    assert_eq!(ids(&first), ["Org/Model1-GGUF", "Org/Model2-GGUF"]);
    assert_eq!(first.next_cursor.as_deref(), Some("page2"));

    let next = HfSearchOptions::new()
        .with_query("llama")
        .with_limit(8)
        .with_cursor(first.next_cursor.unwrap());
    let next = client.search_models_page(&next).await.unwrap();

    assert_eq!(ids(&next), ["Org/Model3-GGUF"]);
    assert_eq!(next.next_cursor, None);
}

/// Every hit of the Hub's page that passes the filters comes back, even
/// past the limit: a hit cut here would never be shown on any page.
#[tokio::test]
async fn a_search_cuts_no_hit_of_the_hubs_page() {
    let backend = FakeBackend::new().with_default(CannedResponse::of(json!([
        fake_model_json("Org/Model1-GGUF", 1000),
        fake_model_json("Org/Model2-GGUF", 2000),
    ])));
    let client = HfClient::with_backend(test_config(), backend);

    let query = HfSearchOptions::new().with_limit(1);
    let found = client.search_models_page(&query).await.unwrap();

    assert_eq!(found.items.len(), 2);
}

#[tokio::test]
async fn test_search_models_page_filters_by_params() {
    let backend = FakeBackend::new().with_response(
        "huggingface.co",
        CannedResponse::of(json!([
            {
                "id": "Org/Small-GGUF",
                "downloads": 1000,
                "siblings": [{"rfilename": "model.gguf"}],
                "gguf": {"total": 1_000_000_000_u64}  // 1B params
            },
            {
                "id": "Org/Large-GGUF",
                "downloads": 2000,
                "siblings": [{"rfilename": "model.gguf"}],
                "gguf": {"total": 70_000_000_000_u64}  // 70B params
            },
        ])),
    );

    let client = HfClient::with_backend(test_config(), backend);

    // Filter for models between 5B and 100B params
    let query = HfSearchOptions::new().with_params_filter(Some(5.0), Some(100.0));

    let response = client.search_models_page(&query).await.unwrap();

    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].model_id, "Org/Large-GGUF");
}

const HAUHAUCS: &str = "HauhauCS/Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-MTP-GGUF";
const QWOPUS: &str = "Jackrong/Qwopus3.8-27B-Flash-V2-GGUF";
const GEMMA: &str = "unsloth/gemma-3-4b-it-GGUF";
const IMAGE_EDIT: &str = "Org/Qwen-Image-Edit-2511-GGUF";
const UNLISTED: &str = "Org/Unlisted-GGUF";

/// A hit of the Hub's search naming `files`, with the `gguf` block the Hub
/// read from one of them.
fn hit_counted(id: &str, files: &[&str], total: u64, context_length: u64) -> serde_json::Value {
    let siblings: Vec<serde_json::Value> = files.iter().map(|f| json!({"rfilename": f})).collect();
    json!({
        "id": id,
        "siblings": siblings,
        "gguf": {"total": total, "context_length": context_length, "architecture": "qwen35"},
    })
}

/// The paths of the GGUF files of a recorded tree.
fn tree_paths(tree: &serde_json::Value) -> Vec<String> {
    tree.as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap().to_owned())
        .collect()
}

/// A Hub whose search answers, as the Hub counted each on 2026-11-12:
/// `HauhauCS` at 1,863,907,840 (its `FastMTP` sidecar's count) and
/// Jackrong's Qwopus at 460,730,096 (a draft's), both named 27B; unsloth's
/// gemma-3-4b-it at 3,880,263,168, named 4b; an image editor named for its
/// date (`2511`) at 20.4B; and a repository named 7B, counted 1B, whose files
/// cannot be sized. The two drafts' files are sized as their trees record.
fn counted_hub() -> FakeBackend {
    let hauhaucs_tree: serde_json::Value = serde_json::from_str(include_str!(
        "../variants_fixtures/HauhauCS_Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-MTP-GGUF.json"
    ))
    .unwrap();
    let hauhaucs_files = tree_paths(&hauhaucs_tree);
    let hauhaucs_files: Vec<&str> = hauhaucs_files.iter().map(String::as_str).collect();
    let qwopus = |quant: &str| format!("Qwopus3.8-27B-Flash-V2-MTP-{quant}.gguf");
    let (qwopus_bf16, qwopus_q4) = (qwopus("BF16"), qwopus("Q4_K_M"));
    FakeBackend::new()
        .with_response(
            "limit=",
            CannedResponse {
                json: json!([
                    hit_counted(HAUHAUCS, &hauhaucs_files, 1_863_907_840, 262_144),
                    hit_counted(
                        QWOPUS,
                        &[&qwopus_bf16, &qwopus_q4, "mmproj-F32.gguf"],
                        460_730_096,
                        262_144
                    ),
                    hit_counted(
                        GEMMA,
                        &[
                            "gemma-3-4b-it-BF16.gguf",
                            "gemma-3-4b-it-Q4_K_M.gguf",
                            "mmproj-F16.gguf"
                        ],
                        3_880_263_168,
                        131_072
                    ),
                    hit_counted(
                        IMAGE_EDIT,
                        &[
                            "qwen-image-edit-2511-BF16.gguf",
                            "qwen-image-edit-2511-Q4_K_M.gguf"
                        ],
                        20_430_401_088,
                        0
                    ),
                    hit_counted(UNLISTED, &["Unlisted-7B-Q4_K_M.gguf"], 1_000_000_000, 4_096),
                ]),
                next_cursor: Some("page2".to_string()),
            },
        )
        .with_response(
            &format!("{HAUHAUCS}/paths-info/main"),
            CannedResponse::of(hauhaucs_tree),
        )
        .with_response(
            &format!("{QWOPUS}/paths-info/main"),
            CannedResponse::of(json!([
                {"type": "file", "path": qwopus_bf16, "size": 54_657_733_568_u64},
                {"type": "file", "path": qwopus_q4, "size": 16_810_714_048_u64},
                {"type": "file", "path": "mmproj-F32.gguf", "size": 1_842_940_992_u64},
            ])),
        )
}

/// The Hub's count for `HauhauCS` and Qwopus is a draft's, which their names
/// contradict: their files are sized, and their largest weights hold the
/// 27B the names state, so that is their count, with no trained context
/// from the draft. A hit whose files could not be sized keeps the Hub's.
#[tokio::test]
async fn a_count_the_hub_read_from_a_draft_is_not_a_hits() {
    let client = HfClient::with_backend(test_config(), counted_hub());

    let found = client
        .search_models_page(&HfSearchOptions::new().with_query("q"))
        .await
        .unwrap();

    let summary = |id: &str| found.items.iter().find(|m| m.model_id == id).unwrap();
    for drafted in [HAUHAUCS, QWOPUS] {
        let hit = summary(drafted);
        assert_eq!(hit.parameters_b, Some(27.0), "{drafted}");
        assert_eq!(hit.context_length, None, "{drafted}");
        assert_eq!(hit.architecture.as_deref(), Some("qwen35"));
    }
    assert_eq!(summary(QWOPUS).file_paths.len(), 3);

    let unlisted = summary(UNLISTED);
    assert_eq!(unlisted.parameters_b, Some(1.0));
    assert_eq!(unlisted.context_length, Some(4_096));

    assert_eq!(found.next_cursor.as_deref(), Some("page2"));
}

/// A hit whose names agree with the Hub, or state no size (a date, a
/// precision), keeps the Hub's count, and its files are never asked for;
/// a hit whose files are is asked once.
#[tokio::test]
async fn a_hit_whose_names_agree_is_not_listed() {
    let client = HfClient::with_backend(test_config(), counted_hub());

    let found = client
        .search_models_page(&HfSearchOptions::new().with_query("q"))
        .await
        .unwrap();

    let summary = |id: &str| found.items.iter().find(|m| m.model_id == id).unwrap();
    assert_eq!(summary(GEMMA).parameters_b, Some(3.880_263_168));
    assert_eq!(summary(GEMMA).context_length, Some(131_072));
    assert_eq!(summary(IMAGE_EDIT).parameters_b, Some(20.430_401_088));
    let listed: Vec<String> = client.backend.got();
    assert_eq!(listed.len(), 3, "{listed:?}");
    assert!(
        listed.iter().all(|url| url.ends_with("/paths-info/main")),
        "{listed:?}"
    );
    for quiet in [GEMMA, IMAGE_EDIT] {
        assert!(!listed.iter().any(|url| url.contains(quiet)), "{listed:?}");
    }
}

/// The size filters judge the checked count, and the page still carries on
/// from the Hub's cursor: `HauhauCS` and Qwopus, which the Hub put under 2B,
/// are kept by a filter for models of 20B and more.
#[tokio::test]
async fn the_size_filters_judge_the_checked_count() {
    let client = HfClient::with_backend(test_config(), counted_hub());
    let ids = |found: &HfSearchResult| -> Vec<String> {
        found.items.iter().map(|m| m.model_id.clone()).collect()
    };

    let large = HfSearchOptions::new().with_params_filter(Some(20.0), None);
    let large = client.search_models_page(&large).await.unwrap();
    assert_eq!(ids(&large), [HAUHAUCS, QWOPUS, IMAGE_EDIT]);
    assert_eq!(large.next_cursor.as_deref(), Some("page2"));

    let small = HfSearchOptions::new().with_params_filter(None, Some(2.0));
    let small = client.search_models_page(&small).await.unwrap();
    assert_eq!(ids(&small), [UNLISTED]);
}
