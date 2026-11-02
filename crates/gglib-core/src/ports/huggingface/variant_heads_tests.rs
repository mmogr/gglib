//! Tests for what a variant is, read from its head, and the label a stored
//! model is looked up by.

use std::path::{Path, PathBuf};

use chrono::Utc;

use super::*;
use crate::domain::{GgufCapabilities, GgufMetadata, NewModel, TensorTable};
use crate::ports::GgufParseError;
use crate::ports::huggingface::fake_hub::{FakeHub, hub_file};
use crate::ports::huggingface::variants_of;

/// Reads a head that says `draft`, `projector`, `model` or `open` as a
/// header whose pairs settle that role (none for `open`), and any other
/// bytes as no GGUF header.
struct RoleParser;

impl GgufParserPort for RoleParser {
    fn parse(&self, _file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        unimplemented!("only heads are read here")
    }
    fn detect_capabilities(&self, _metadata: &GgufMetadata) -> GgufCapabilities {
        GgufCapabilities::empty()
    }
    fn tensor_table(&self, _path: &Path) -> Result<TensorTable, GgufParseError> {
        unimplemented!("only heads are read here")
    }
    fn tensor_table_of_head(&self, _head: &[u8]) -> Result<TensorTable, GgufParseError> {
        unimplemented!("only heads are read here")
    }
    fn parse_head(&self, head: &[u8], _file_name: &str) -> Result<HeadMetadata, GgufParseError> {
        let role = match head {
            b"draft" => Some(GgufFileRole::Sidecar),
            b"projector" => Some(GgufFileRole::Projector),
            b"model" => Some(GgufFileRole::Weights),
            b"open" => None,
            _ => return Err(GgufParseError::InvalidFormat("no magic".to_owned())),
        };
        Ok(HeadMetadata {
            metadata: GgufMetadata::default(),
            reached_tokenizer: false,
            complete: role.is_some(),
            bytes_used: 0,
            tensor_count: 0,
            role,
        })
    }
}

fn variant(label: &str) -> HfQuantInfo {
    HfQuantInfo {
        label: label.to_owned(),
        role: None,
        file_paths: vec![format!("{label}.gguf")],
        total_size: 1,
    }
}

fn hub() -> FakeHub {
    let head = |label: &str, bytes: &[u8]| (format!("{label}.gguf"), bytes.to_vec());
    FakeHub {
        heads: vec![
            head("FastMTP", b"draft"),
            head("mmproj-F16", b"projector"),
            head("Q4_K_P", b"model"),
            head("Q8_K_P", b"open"),
            head("junk", b"not a gguf"),
        ],
        ..Default::default()
    }
}

/// A head that settles a draft or a projector says so; every other head, a
/// head that is no GGUF header and a head that cannot be read is a model's.
#[tokio::test]
async fn each_variant_is_what_its_head_settles_and_else_a_model() {
    let hub = hub();
    let labels = [
        "FastMTP",
        "mmproj-F16",
        "Q4_K_P",
        "Q8_K_P",
        "junk",
        "unlisted",
    ];

    let read = read_roles(&hub, &RoleParser, "o/r", labels.map(variant).to_vec()).await;

    let roles: Vec<(&str, Option<GgufFileRole>)> =
        read.iter().map(|v| (v.label.as_str(), v.role)).collect();
    assert_eq!(
        roles,
        [
            ("FastMTP", Some(GgufFileRole::Sidecar)),
            ("mmproj-F16", Some(GgufFileRole::Projector)),
            ("Q4_K_P", Some(GgufFileRole::Weights)),
            ("Q8_K_P", Some(GgufFileRole::Weights)),
            ("junk", Some(GgufFileRole::Weights)),
            ("unlisted", Some(GgufFileRole::Weights)),
        ]
    );
    let all_at_the_head_size = hub
        .heads_asked
        .lock()
        .unwrap()
        .iter()
        .all(|(_, _, len)| *len == VARIANT_HEAD_BYTES);
    assert!(all_at_the_head_size);
}

/// A role known already is kept, and no head is read for it.
#[tokio::test]
async fn a_known_role_is_not_read_again() {
    let hub = hub();
    let known = HfQuantInfo {
        role: Some(GgufFileRole::Sidecar),
        ..variant("Q4_K_P")
    };

    assert_eq!(
        variant_role(&hub, &RoleParser, "o/r", &known).await,
        GgufFileRole::Sidecar
    );
    assert!(hub.heads_asked.lock().unwrap().is_empty());
}

fn stored(variant_label: Option<&str>, hf_filename: Option<&str>, path: &str) -> Model {
    let mut new = NewModel::new("m".to_owned(), PathBuf::from(path), 7.0, Utc::now());
    new.variant_label = variant_label.map(str::to_owned);
    new.hf_filename = hf_filename.map(str::to_owned);
    Model::stored(1, &new)
}

/// The stored label while the listing has it; else, a stale label as much
/// as none, the variant holding the model's file, by its stored path first
/// and by its Hub file name else; else none.
#[tokio::test]
async fn a_stored_model_is_looked_up_by_its_label_else_by_its_file() {
    let hub = FakeHub {
        weights: vec![
            hub_file("BF16/X-BF16-00001-of-00002.gguf", 1, "a"),
            hub_file("BF16/X-BF16-00002-of-00002.gguf", 1, "b"),
            hub_file("X-Q8_0.gguf", 1, "c"),
        ],
        ..Default::default()
    };
    let hub = &hub;
    let label = |model: Model| async move { stored_label(hub, "o/r", &model).await.unwrap() };
    let shard = "X-BF16-00001-of-00002.gguf";
    let elsewhere = "/elsewhere/m.gguf";

    assert_eq!(
        label(stored(Some("q8_0"), Some(shard), elsewhere)).await,
        Some("q8_0".to_owned()),
        "a label the listing still has is kept as stored"
    );
    assert_eq!(
        label(stored(Some("UD-Q4_K_XL"), Some(shard), elsewhere)).await,
        Some("BF16".to_owned()),
        "a stale label falls back to the file"
    );
    assert_eq!(
        label(stored(None, Some(shard), elsewhere)).await,
        Some("BF16".to_owned())
    );
    assert_eq!(
        label(stored(None, None, &format!("/models/o/r/BF16/{shard}"))).await,
        None,
        "neither a label nor a Hub file name"
    );
    assert_eq!(
        label(stored(None, Some("X-Q4_0.gguf"), elsewhere)).await,
        None
    );
}

/// One file name in two folders names neither variant: the stored path's
/// folder says which, and with no folder to tell them apart, none is taken.
#[tokio::test]
async fn a_file_name_two_variants_hold_is_told_apart_by_its_folder_or_refused() {
    let hub = FakeHub {
        weights: vec![
            hub_file("Q4_K_M/model.gguf", 1, "a"),
            hub_file("Q8_0/model.gguf", 1, "b"),
        ],
        ..Default::default()
    };
    let hub = &hub;
    let label = |model: Model| async move { stored_label(hub, "o/r", &model).await.unwrap() };

    assert_eq!(
        label(stored(
            None,
            Some("model.gguf"),
            "/models/o/r/Q8_0/model.gguf"
        ))
        .await,
        Some("Q8_0".to_owned())
    );
    assert_eq!(
        label(stored(None, Some("model.gguf"), "/models/o/r/model.gguf")).await,
        None
    );
}

/// A nested file and a root file of one name both end the stored path; the
/// longer, the nested one, is the model's.
#[tokio::test]
async fn the_longest_path_the_stored_one_ends_with_is_the_models() {
    let hub = FakeHub {
        weights: vec![
            hub_file("Q8_0/model.gguf", 1, "a"),
            hub_file("model.gguf", 1, "b"),
        ],
        ..Default::default()
    };

    let model = stored(None, Some("model.gguf"), "/models/o/r/Q8_0/model.gguf");
    let label = stored_label(&hub, "o/r", &model).await.unwrap();

    let nested = variants_of(hub.weights.iter().map(|f| (f.path.as_str(), f.size)))
        .into_iter()
        .find(|v| v.file_paths == ["Q8_0/model.gguf"])
        .unwrap();
    assert_eq!(label, Some(nested.label));
}
