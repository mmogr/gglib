//! Tests for [`super`]: a safetensors header read as a tensor table, and the
//! headers it refuses before reserving for them.

use gglib_core::domain::{TensorInfo, WeightsFormat};
use gglib_core::{GgufParseError, GgufParserPort};

use super::*;
use crate::{GgufParser, write_safetensors};

/// A file of the header length `declared`, then `json`.
fn file(declared: u64, json: &str) -> Vec<u8> {
    [&declared.to_le_bytes()[..], json.as_bytes()].concat()
}

/// A file whose header length is exactly `json`'s.
fn whole(json: &str) -> Vec<u8> {
    file(json.len() as u64, json)
}

fn refusal(bytes: &[u8]) -> String {
    match GgufParser::new().tensor_table_of_head(bytes) {
        Err(GgufParseError::InvalidFormat(message)) => message,
        other => panic!("the header is not refused as malformed: {other:?}"),
    }
}

/// Every tensor reads back with its shape as written, outermost first; the
/// `__metadata__` entry real files carry is not a tensor.
#[test]
fn a_header_reads_back_the_tensors_it_was_written_with() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clip_l.safetensors");
    write_safetensors(
        &path,
        &[
            (
                "text_model.embeddings.token_embedding.weight",
                &[49408, 768],
            ),
            ("text_model.final_layer_norm.bias", &[768]),
        ],
    );

    let mut table = GgufParser::new().tensor_table(&path).unwrap();
    table.tensors.sort_by(|a, b| a.name.cmp(&b.name));

    assert_eq!(table.format, WeightsFormat::Safetensors);
    assert_eq!(table.architecture, None);
    assert_eq!(
        table.tensors,
        vec![
            TensorInfo {
                name: "text_model.embeddings.token_embedding.weight".to_owned(),
                shape: vec![49408, 768],
            },
            TensorInfo {
                name: "text_model.final_layer_norm.bias".to_owned(),
                shape: vec![768],
            },
        ]
    );
}

#[test]
fn the_metadata_entry_is_skipped_whatever_it_holds() {
    let bytes = whole(r#"{"__metadata__":{"format":"pt","shape":"x"},"a":{"shape":[2,3]}}"#);
    let table = GgufParser::new().tensor_table_of_head(&bytes).unwrap();
    let names: Vec<&str> = table.tensors.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["a"]);
}

/// A length over the cap is refused on its word, whatever the file holds.
#[test]
fn a_header_length_over_the_cap_is_refused() {
    let message = refusal(&file(MAX_SAFETENSORS_HEADER + 1, "{}"));
    assert_eq!(
        message,
        "safetensors: header length 100000001 is over the 100000000 bytes a header may take"
    );
}

/// A length under the cap that the rest of the file cannot hold is refused
/// before anything is reserved for it.
#[test]
fn a_header_length_over_the_file_is_refused() {
    let message = refusal(&file(1000, "{}"));
    assert_eq!(
        message,
        "safetensors header length 1000 is more than the 2 bytes left can hold"
    );
}

#[test]
fn a_header_that_is_not_an_object_is_refused() {
    assert_eq!(
        refusal(&whole("[1,2]")),
        "safetensors: header is not a JSON object"
    );
    assert!(refusal(&whole("{nope")).starts_with("safetensors: header is not JSON"));
}

#[test]
fn a_tensor_without_a_shape_of_integers_is_refused() {
    for json in [
        r#"{"a":{"dtype":"F16"}}"#,
        r#"{"a":{"shape":[-1]}}"#,
        r#"{"a":{"shape":"2x3"}}"#,
        r#"{"a":7}"#,
    ] {
        assert_eq!(
            refusal(&whole(json)),
            "safetensors: tensor a has no shape of integers",
            "{json}"
        );
    }
}

/// A head that stops inside the header is an error, never a shorter table.
#[test]
fn a_cut_head_errs_wherever_it_is_cut() {
    let bytes = whole(r#"{"a":{"shape":[2,3]},"b":{"shape":[4]}}"#);
    let parser = GgufParser::new();
    assert!(parser.tensor_table_of_head(&bytes).is_ok());
    for cut in 0..bytes.len() {
        assert!(
            parser.tensor_table_of_head(&bytes[..cut]).is_err(),
            "a head cut at {cut} was read"
        );
    }
}

/// The length is all eight bytes: one whose high half is set is over the
/// cap, however small its low half reads.
#[test]
fn a_header_length_with_its_high_half_set_is_refused() {
    let declared = (1_u64 << 32) | 2;
    let message = refusal(&file(declared, "{}"));
    assert_eq!(
        message,
        format!(
            "safetensors: header length {declared} is over the 100000000 bytes a header may take"
        )
    );
}
