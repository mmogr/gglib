//! Tests for what a variant is, read from its head.

use std::path::Path;

use super::*;
use crate::domain::{GgufCapabilities, GgufMetadata, TensorTable};
use crate::ports::GgufParseError;
use crate::ports::huggingface::fake_hub::FakeHub;

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
