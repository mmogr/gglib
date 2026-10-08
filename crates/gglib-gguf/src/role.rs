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
    use gglib_core::domain::gguf::GgufValue;
    use gglib_core::{GgufParseError, GgufParserPort};

    use super::*;
    use crate::{GgufParser, write_string_gguf};

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

    /// Through the port, from bytes on disk: the name says one thing and the
    /// header the other, and the parsed role is the header's.
    #[test]
    fn the_parser_reports_the_role_the_file_header_states() {
        let dir = tempfile::tempdir().unwrap();
        let projector = dir.path().join("tower.Q8_0.gguf");
        write_string_gguf(
            &projector,
            &[("general.architecture", "clip"), ("general.type", "mmproj")],
        );
        let weights = dir.path().join("mmproj-F16.gguf");
        write_string_gguf(
            &weights,
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
