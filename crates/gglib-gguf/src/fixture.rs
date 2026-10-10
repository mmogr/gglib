//! Weights files written without a model, for a test of whatever reads one:
//! a GGUF file of string metadata and tensor entries, and a safetensors file
//! of tensor entries. Neither holds a byte of tensor data.

use std::path::Path;

use crate::format::GGUF_MAGIC;

/// The metadata value type of a string.
const STRING: u32 = 8;

/// The ggml type every fixture tensor declares (`F32`); nothing reads it.
const F32: u32 = 0;

/// Write a GGUF v3 file at `path` holding only `pairs` as string metadata,
/// and no tensors.
///
/// The layout is the one the parser reads, little-endian throughout: the
/// magic, the version as `u32`, the tensor count and the metadata count as
/// `u64`, then per pair the key, the value type as `u32` and the value. A
/// string is its length as `u64` and then its UTF-8 bytes.
///
/// # Panics
///
/// When the file cannot be written.
pub fn write_string_gguf(path: &Path, pairs: &[(&str, &str)]) {
    write_tensor_gguf(path, pairs, &[]);
}

/// Write a GGUF v3 file at `path` holding `pairs` as string metadata and an
/// entry per tensor in `tensors`, each a name and a shape outermost first.
///
/// The layout is [`write_string_gguf`]'s, with the tensor count set and the
/// tensor-info table after the metadata: per tensor its name, its dimension
/// count as `u32`, its dimensions as `u64` innermost first (ggml's `ne`, so
/// `shape` reversed), its type as `u32` and its data offset as `u64`. No
/// tensor data follows.
///
/// # Panics
///
/// When the file cannot be written.
pub fn write_tensor_gguf(path: &Path, pairs: &[(&str, &str)], tensors: &[(&str, &[u64])]) {
    let string = |text: &str| {
        let mut bytes = (text.len() as u64).to_le_bytes().to_vec();
        bytes.extend_from_slice(text.as_bytes());
        bytes
    };
    let mut bytes = GGUF_MAGIC.to_vec();
    bytes.extend_from_slice(&3_u32.to_le_bytes());
    bytes.extend_from_slice(&(tensors.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&(pairs.len() as u64).to_le_bytes());
    for (key, value) in pairs {
        bytes.extend(string(key));
        bytes.extend_from_slice(&STRING.to_le_bytes());
        bytes.extend(string(value));
    }
    for (name, shape) in tensors {
        bytes.extend(string(name));
        let n_dims = u32::try_from(shape.len()).expect("a fixture shape is short");
        bytes.extend_from_slice(&n_dims.to_le_bytes());
        for dim in shape.iter().rev() {
            bytes.extend_from_slice(&dim.to_le_bytes());
        }
        bytes.extend_from_slice(&F32.to_le_bytes());
        bytes.extend_from_slice(&0_u64.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("the fixture is written");
}

/// Write a safetensors file at `path` whose header names each tensor in
/// `tensors`, a name and a shape outermost first, beside a `__metadata__`
/// entry as real files carry.
///
/// The layout is the header's length as a little-endian `u64`, then the
/// header's JSON. Every tensor claims an empty data range and no data follows.
///
/// # Panics
///
/// When the file cannot be written.
pub fn write_safetensors(path: &Path, tensors: &[(&str, &[u64])]) {
    let mut header = serde_json::Map::new();
    header.insert(
        "__metadata__".to_owned(),
        serde_json::json!({ "format": "pt" }),
    );
    for (name, shape) in tensors {
        header.insert(
            (*name).to_owned(),
            serde_json::json!({ "dtype": "F16", "shape": shape, "data_offsets": [0, 0] }),
        );
    }
    let json = serde_json::Value::Object(header).to_string();
    let mut bytes = (json.len() as u64).to_le_bytes().to_vec();
    bytes.extend_from_slice(json.as_bytes());
    std::fs::write(path, bytes).expect("the fixture is written");
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use gglib_core::GgufParserPort;

    use super::*;
    use crate::GgufParser;

    /// The parser reads back the pairs the file was written from, and
    /// nothing else.
    #[test]
    fn the_parser_reads_back_the_pairs_a_fixture_was_written_from() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.gguf");
        let pairs = [
            ("general.architecture", "qwen3"),
            ("general.name", "Qwen3 8B 多语言"),
            ("general.description", ""),
        ];

        write_string_gguf(&path, &pairs);

        let read = GgufParser::new().parse(&path).unwrap().metadata;
        let written: HashMap<String, String> = pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        assert_eq!(read, written);
    }

    #[test]
    fn a_fixture_of_no_pairs_parses_as_a_file_with_no_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.gguf");

        write_string_gguf(&path, &[]);

        let read = GgufParser::new().parse(&path).unwrap().metadata;
        assert!(read.is_empty(), "{read:?}");
    }
}
