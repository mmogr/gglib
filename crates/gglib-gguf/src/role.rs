//! What a GGUF header says its file is: a model's weights, or a projector.
//!
//! llama.cpp's converter writes `general.type = "mmproj"` into a projector.
//! Projectors converted before that key existed carry no `general.type`, and
//! are told by `general.architecture = "clip"`, which no model's weights use.

use gglib_core::GgufFileRole;
use gglib_core::domain::gguf::RawMetadata;

/// The role the header's own keys state.
pub(crate) fn file_role(raw: &RawMetadata) -> GgufFileRole {
    let states = |key: &str, value: &str| raw.get(key).and_then(|v| v.as_str()) == Some(value);
    if states("general.type", "mmproj") || states("general.architecture", "clip") {
        GgufFileRole::Projector
    } else {
        GgufFileRole::Weights
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use gglib_core::domain::gguf::GgufValue;
    use gglib_core::{GgufParseError, GgufParserPort};

    use super::*;
    use crate::GgufParser;

    fn header(pairs: &[(&str, &str)]) -> RawMetadata {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), GgufValue::String((*value).to_owned())))
            .collect()
    }

    #[test]
    fn a_header_typed_mmproj_is_a_projector() {
        let raw = header(&[("general.type", "mmproj"), ("general.architecture", "clip")]);
        assert_eq!(file_role(&raw), GgufFileRole::Projector);
    }

    /// The type alone is the statement, whatever the architecture is called.
    #[test]
    fn a_header_typed_mmproj_under_another_architecture_is_a_projector() {
        let raw = header(&[
            ("general.type", "mmproj"),
            ("general.architecture", "siglip"),
        ]);
        assert_eq!(file_role(&raw), GgufFileRole::Projector);
    }

    /// The fallback: an older projector with no `general.type` at all.
    #[test]
    fn a_clip_header_with_no_type_is_a_projector() {
        let raw = header(&[("general.architecture", "clip")]);
        assert_eq!(file_role(&raw), GgufFileRole::Projector);
    }

    #[test]
    fn a_model_header_is_weights() {
        let raw = header(&[("general.type", "model"), ("general.architecture", "qwen3")]);
        assert_eq!(file_role(&raw), GgufFileRole::Weights);
        assert_eq!(file_role(&header(&[])), GgufFileRole::Weights);
    }

    /// The values are matched whole and as strings: a name that merely
    /// mentions the word, or a number, is not the statement.
    #[test]
    fn a_near_miss_is_weights() {
        let raw = header(&[
            ("general.type", "mmproj-adapter"),
            ("general.name", "mmproj"),
        ]);
        assert_eq!(file_role(&raw), GgufFileRole::Weights);
        let mut numeric = RawMetadata::new();
        numeric.insert("general.type".to_owned(), GgufValue::U32(1));
        assert_eq!(file_role(&numeric), GgufFileRole::Weights);
    }

    /// A GGUF v3 file holding only `pairs` as string metadata, no tensors.
    fn write_gguf(dir: &Path, name: &str, pairs: &[(&str, &str)]) -> PathBuf {
        let string = |text: &str| {
            let mut bytes = (text.len() as u64).to_le_bytes().to_vec();
            bytes.extend_from_slice(text.as_bytes());
            bytes
        };
        let mut bytes = b"GGUF".to_vec();
        bytes.extend_from_slice(&3_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u64.to_le_bytes());
        bytes.extend_from_slice(&(pairs.len() as u64).to_le_bytes());
        for (key, value) in pairs {
            bytes.extend(string(key));
            bytes.extend_from_slice(&8_u32.to_le_bytes());
            bytes.extend(string(value));
        }
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    /// Through the port, from bytes on disk: the name says one thing and the
    /// header the other, and the parsed role is the header's.
    #[test]
    fn the_parser_reports_the_role_the_file_header_states() {
        let dir = tempfile::tempdir().unwrap();
        let projector = write_gguf(
            dir.path(),
            "tower.Q8_0.gguf",
            &[("general.architecture", "clip"), ("general.type", "mmproj")],
        );
        let weights = write_gguf(
            dir.path(),
            "mmproj-F16.gguf",
            &[("general.architecture", "qwen3"), ("general.type", "model")],
        );

        let parser = GgufParser::new();
        assert_eq!(
            parser.parse(&projector).unwrap().role,
            GgufFileRole::Projector
        );
        assert_eq!(parser.parse(&weights).unwrap().role, GgufFileRole::Weights);
    }

    #[test]
    fn a_file_that_is_not_a_gguf_has_no_role_to_report() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mmproj-F16.gguf");
        std::fs::write(&path, b"<html>not a model</html>").unwrap();

        let refused = GgufParser::new().parse(&path).unwrap_err();
        assert!(
            matches!(refused, GgufParseError::InvalidFormat(_)),
            "{refused:?}"
        );
    }
}
