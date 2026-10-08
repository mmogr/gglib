//! A GGUF file written without a model, for a test of whatever reads one.

use std::path::Path;

use crate::format::GGUF_MAGIC;

/// The metadata value type of a string.
const STRING: u32 = 8;

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
    let string = |text: &str| {
        let mut bytes = (text.len() as u64).to_le_bytes().to_vec();
        bytes.extend_from_slice(text.as_bytes());
        bytes
    };
    let mut bytes = GGUF_MAGIC.to_vec();
    bytes.extend_from_slice(&3_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u64.to_le_bytes());
    bytes.extend_from_slice(&(pairs.len() as u64).to_le_bytes());
    for (key, value) in pairs {
        bytes.extend(string(key));
        bytes.extend_from_slice(&STRING.to_le_bytes());
        bytes.extend(string(value));
    }
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
