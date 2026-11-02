//! The role rule against the headers of real files: each fixture below is
//! the role-deciding keys of a file read off the Hub, named for it.

use gglib_core::domain::gguf::GgufValue;
use gglib_core::{GgufParseError, GgufParserPort};

use super::*;
use crate::{GgufParser, write_string_gguf};

/// A tensor count no test's block count reaches: the file holds its trunk.
const WHOLE: u64 = 10_000;

fn header(pairs: &[(&str, &str)]) -> RawMetadata {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), GgufValue::String((*value).to_owned())))
        .collect()
}

#[test]
fn a_header_typed_mmproj_is_a_projector() {
    let raw = header(&[("general.type", "mmproj"), ("general.architecture", "clip")]);
    assert_eq!(file_role(&raw, WHOLE), GgufFileRole::Projector);
}

/// The type alone is the statement, whatever the architecture is called.
#[test]
fn a_header_typed_mmproj_under_another_architecture_is_a_projector() {
    let raw = header(&[
        ("general.type", "mmproj"),
        ("general.architecture", "siglip"),
    ]);
    assert_eq!(file_role(&raw, WHOLE), GgufFileRole::Projector);
}

/// The fallback: an older projector with no `general.type` at all.
#[test]
fn a_clip_header_with_no_type_is_a_projector() {
    let raw = header(&[("general.architecture", "clip")]);
    assert_eq!(file_role(&raw, WHOLE), GgufFileRole::Projector);
}

#[test]
fn a_model_header_is_weights() {
    let raw = header(&[("general.type", "model"), ("general.architecture", "qwen3")]);
    assert_eq!(file_role(&raw, WHOLE), GgufFileRole::Weights);
    assert_eq!(file_role(&header(&[]), WHOLE), GgufFileRole::Weights);
}

/// The values are matched whole and as strings: a name that merely
/// mentions the word, or a number, is not the statement. A type that is not
/// `mmproj` is no projector, and not being `model` either, no weights.
#[test]
fn a_near_miss_is_no_projector() {
    let raw = header(&[
        ("general.type", "mmproj-adapter"),
        ("general.name", "mmproj"),
    ]);
    assert_eq!(file_role(&raw, WHOLE), GgufFileRole::Sidecar);
    let named = header(&[("general.type", "model"), ("general.name", "mmproj")]);
    assert_eq!(file_role(&named, WHOLE), GgufFileRole::Weights);
    let mut numeric = RawMetadata::new();
    numeric.insert("general.type".to_owned(), GgufValue::U32(1));
    assert_eq!(file_role(&numeric, WHOLE), GgufFileRole::Weights);
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

// =============================================================================
// Sidecars, from the keys of files on the Hub
// =============================================================================

fn count(n: u32) -> GgufValue {
    GgufValue::U32(n)
}

fn layers(of: &[i32]) -> GgufValue {
    GgufValue::Array(of.iter().copied().map(GgufValue::I32).collect())
}

/// A header of `architecture`, typed `model`, with `pairs` under it.
fn model(architecture: &str, pairs: &[(&str, GgufValue)]) -> RawMetadata {
    let mut raw = header(&[
        ("general.architecture", architecture),
        ("general.type", "model"),
    ]);
    for (key, value) in pairs {
        raw.insert((*key).to_owned(), value.clone());
    }
    raw
}

/// `HauhauCS`'s `…-FastMTP-32K.gguf`: Qwen 3.8's MTP block cut out, 19 tensors.
fn qwen_mtp_head() -> RawMetadata {
    model(
        "qwen35",
        &[
            ("qwen35.block_count", count(65)),
            ("qwen35.nextn_predict_layers", count(1)),
        ],
    )
}

#[test]
fn an_mtp_head_cut_out_of_a_model_is_a_sidecar() {
    assert_eq!(file_role(&qwen_mtp_head(), 19), GgufFileRole::Sidecar);
}

/// The model it came from declares the same keys and every block.
#[test]
fn a_model_that_keeps_its_mtp_head_is_weights() {
    assert_eq!(file_role(&qwen_mtp_head(), 866), GgufFileRole::Weights);
}

/// `HauhauCS`'s `mtp-gemma-4-26B-A4B-it.gguf`: four blocks, all MTP.
#[test]
fn an_assistant_whose_every_block_predicts_ahead_is_a_sidecar() {
    let raw = model(
        "gemma4-assistant",
        &[
            ("gemma4-assistant.block_count", count(4)),
            ("gemma4-assistant.nextn_predict_layers", count(4)),
        ],
    );
    assert_eq!(file_role(&raw, 49), GgufFileRole::Sidecar);
}

/// `EAGLE3`, `EAGLE4` and `DFlash` drafts name the target's layers they read.
#[test]
fn a_draft_that_names_its_targets_layers_is_a_sidecar() {
    for architecture in ["eagle3", "eagle4", "dflash"] {
        let raw = model(
            architecture,
            &[
                (&format!("{architecture}.block_count"), count(1)),
                (
                    &format!("{architecture}.target_layers"),
                    layers(&[2, 32, 61]),
                ),
            ],
        );
        assert_eq!(file_role(&raw, 13), GgufFileRole::Sidecar, "{architecture}");
    }
}

/// `NOhhh`'s Eagle-Lite has no `general.type` and no blocks at all.
#[test]
fn a_draft_with_no_type_is_still_a_sidecar() {
    let mut raw = header(&[("general.architecture", "dflash")]);
    raw.insert("dflash.block_count".to_owned(), count(0));
    raw.insert("dflash.target_layers".to_owned(), layers(&[63]));
    assert_eq!(file_role(&raw, 11), GgufFileRole::Sidecar);
}

#[test]
fn a_header_typed_as_anything_but_a_model_is_a_sidecar() {
    for kind in ["imatrix", "adapter"] {
        assert_eq!(
            file_role(&header(&[("general.type", kind)]), 1020),
            GgufFileRole::Sidecar,
            "{kind}"
        );
    }
}

/// ggml-org's 1.5B speculative draft is a whole model that runs alone.
#[test]
fn a_small_whole_model_used_as_a_draft_is_weights() {
    let raw = model("qwen2", &[("qwen2.block_count", count(28))]);
    assert_eq!(file_role(&raw, 339), GgufFileRole::Weights);
}

/// A tensor per block is enough for a trunk: the test is strictly fewer.
#[test]
fn a_file_with_exactly_a_tensor_per_block_is_weights() {
    assert_eq!(file_role(&qwen_mtp_head(), 65), GgufFileRole::Weights);
}

/// Short of blocks with no MTP key is no draft: a vocab-only file.
#[test]
fn a_file_short_of_blocks_without_an_mtp_key_is_weights() {
    let raw = model("llama", &[("llama.block_count", count(32))]);
    assert_eq!(file_role(&raw, 0), GgufFileRole::Weights);
}

/// `split.count` of one says the file is whole, so short of blocks it is a
/// head cut out.
#[test]
fn a_file_split_into_one_short_of_blocks_is_a_sidecar() {
    let mut raw = qwen_mtp_head();
    raw.insert("split.count".to_owned(), GgufValue::U16(1));
    assert_eq!(file_role(&raw, 19), GgufFileRole::Sidecar);
}

/// A shard holds fewer tensors than its model has blocks, and says so.
#[test]
fn a_shard_of_a_model_with_an_mtp_head_is_weights() {
    let mut raw = model(
        "deepseek2",
        &[
            ("deepseek2.block_count", count(61)),
            ("deepseek2.nextn_predict_layers", count(1)),
        ],
    );
    raw.insert("split.count".to_owned(), GgufValue::U16(30));
    assert_eq!(file_role(&raw, 34), GgufFileRole::Weights);
}

// =============================================================================
// What a head settles
// =============================================================================

/// An EAGLE3 draft whose head ends after its block count and before
/// `target_layers`, and a model with every block: neither is weights yet.
#[test]
fn a_head_never_settles_weights_before_the_whole_header() {
    let draft = model("eagle3", &[("eagle3.block_count", count(1))]);
    assert_eq!(head_role(&draft, 13, false), None);
    assert_eq!(head_role(&qwen_mtp_head(), 866, false), None);
    assert_eq!(
        head_role(&qwen_mtp_head(), 866, true),
        Some(GgufFileRole::Weights)
    );
}

/// The MTP head's own writer puts the tokenizer first: a head of it holds
/// the architecture and nothing after.
#[test]
fn a_head_that_has_not_reached_the_blocks_is_unsettled() {
    let raw = header(&[("general.architecture", "qwen35")]);
    assert_eq!(head_role(&raw, 19, false), None);
    assert_eq!(head_role(&raw, 19, true), Some(GgufFileRole::Weights));
}

/// Short of blocks, it could be a shard whose `split.count` lies further on.
#[test]
fn a_head_short_of_blocks_without_a_split_count_is_unsettled() {
    assert_eq!(head_role(&qwen_mtp_head(), 19, false), None);
    assert_eq!(
        head_role(&qwen_mtp_head(), 19, true),
        Some(GgufFileRole::Sidecar)
    );
}

#[test]
fn a_head_settles_a_sidecar_as_soon_as_it_reads_one_sign() {
    let raw = model("eagle3", &[("eagle3.target_layers", layers(&[2]))]);
    assert_eq!(head_role(&raw, 13, false), Some(GgufFileRole::Sidecar));
}
