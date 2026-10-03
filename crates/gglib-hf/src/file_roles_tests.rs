//! Tests that a projector is listed apart from the quantizations.

use gglib_core::download::{Quantization, choose_projector};

use super::*;
use crate::models::{HfEntryType, HfQuantization};
use crate::parsing::{aggregate_quantizations, filter_files_by_quantization, parse_tree_entries};

/// The GGUF files of `unsloth/Qwen3.8-27B-GGUF` as its tree API listed them
/// on 2026-10-04: path, size and LFS OID of each.
const QWEN_TREE: &str = include_str!("file_roles_fixture.json");

fn qwen_files() -> Vec<HfFileEntry> {
    let tree: serde_json::Value = serde_json::from_str(QWEN_TREE).unwrap();
    parse_tree_entries(&tree).unwrap()
}

fn gguf(path: &str, size: u64) -> HfFileEntry {
    HfFileEntry {
        path: path.to_string(),
        entry_type: HfEntryType::File,
        size,
        oid: None,
    }
}

fn names(quantizations: &[HfQuantization]) -> Vec<&str> {
    quantizations.iter().map(|q| q.name.as_str()).collect()
}

fn paths(files: &[HfFileEntry]) -> Vec<&str> {
    files.iter().map(|f| f.path.as_str()).collect()
}

// ── The real listing ─────────────────────────────────────────────────────

#[test]
fn the_real_listing_holds_two_projectors_listed_apart() {
    let projectors = projectors_among(&qwen_files());

    assert_eq!(paths(&projectors), ["mmproj-BF16.gguf", "mmproj-F16.gguf"]);
    assert_eq!(projectors[0].size, 931_146_432);
    assert_eq!(projectors[1].size, 927_607_488);
    assert_eq!(
        projectors[1].oid.as_deref(),
        Some("cbb841a9ee0636b2ec172f5bb8df2ea8dfeb01e90fe7c6126581d662a0b4e43e")
    );
}

#[test]
fn no_projector_of_the_real_listing_is_a_quantization_or_a_shard() {
    let quantizations = aggregate_quantizations(&qwen_files());

    for quantization in &quantizations {
        for path in &quantization.paths {
            assert!(
                !path.contains("mmproj"),
                "{path} is listed under {}",
                quantization.name
            );
        }
    }
    // The repository has no F16 weights: `mmproj-F16.gguf` was the whole of
    // the "F16" quantization.
    assert!(!names(&quantizations).contains(&"F16"));
    // BF16 is the two weights shards, without `mmproj-BF16.gguf` as a third.
    let bf16 = quantizations.iter().find(|q| q.name == "BF16").unwrap();
    assert_eq!(
        bf16.paths,
        [
            "BF16/Qwen3.8-27B-BF16-00001-of-00002.gguf",
            "BF16/Qwen3.8-27B-BF16-00002-of-00002.gguf"
        ]
    );
    assert_eq!(bf16.shard_count, 2);
    assert_eq!(bf16.total_size, 49_986_159_616 + 4_671_576_000);
    let q8 = quantizations.iter().find(|q| q.name == "Q8_0").unwrap();
    assert_eq!(q8.paths, ["Qwen3.8-27B-Q8_0.gguf"]);
}

#[test]
fn the_files_of_a_quantization_of_the_real_listing_hold_no_projector() {
    let files = qwen_files();

    assert_eq!(
        paths(&filter_files_by_quantization(&files, "BF16")),
        [
            "BF16/Qwen3.8-27B-BF16-00001-of-00002.gguf",
            "BF16/Qwen3.8-27B-BF16-00002-of-00002.gguf"
        ]
    );
    assert!(filter_files_by_quantization(&files, "F16").is_empty());
}

/// What a `Q8_0` download of the real repository fetches with its weights.
#[test]
fn a_q8_download_of_the_real_listing_takes_the_f16_projector() {
    let projectors = projectors_among(&qwen_files());

    let chosen = choose_projector(
        Quantization::Q8_0,
        projectors.iter().map(|p| p.path.as_str()),
    );

    assert_eq!(chosen, Some("mmproj-F16.gguf"));
}

// ── The shapes a repository takes ────────────────────────────────────────

#[test]
fn a_projector_beside_one_quantization_leaves_one_quantization() {
    let files = [
        gguf("model-Q4_K_M.gguf", 4_000),
        gguf("mmproj-F16.gguf", 900),
    ];

    let quantizations = aggregate_quantizations(&files);

    assert_eq!(names(&quantizations), ["Q4_K_M"]);
    assert_eq!(quantizations[0].paths, ["model-Q4_K_M.gguf"]);
    assert_eq!(quantizations[0].total_size, 4_000);
    assert_eq!(paths(&projectors_among(&files)), ["mmproj-F16.gguf"]);
}

#[test]
fn f16_weights_beside_an_f16_projector_are_the_whole_f16_quantization() {
    let files = [gguf("model-F16.gguf", 16_000), gguf("mmproj-F16.gguf", 900)];

    let quantizations = aggregate_quantizations(&files);

    assert_eq!(names(&quantizations), ["F16"]);
    assert_eq!(quantizations[0].paths, ["model-F16.gguf"]);
    assert_eq!(quantizations[0].shard_count, 1);
    assert!(!quantizations[0].is_sharded());
    assert_eq!(quantizations[0].total_size, 16_000);
    assert_eq!(
        paths(&filter_files_by_quantization(&files, "F16")),
        ["model-F16.gguf"]
    );
}

/// The shape a library downloaded before the split holds: the projector was
/// stored as the second shard of `Q8_0`.
#[test]
fn a_projector_of_the_same_quantization_is_not_a_second_shard() {
    let files = [gguf("X.Q8_0.gguf", 8_000), gguf("X.mmproj-Q8_0.gguf", 600)];

    let quantizations = aggregate_quantizations(&files);

    assert_eq!(names(&quantizations), ["Q8_0"]);
    assert_eq!(quantizations[0].paths, ["X.Q8_0.gguf"]);
    assert_eq!(quantizations[0].shard_count, 1);
    assert_eq!(
        paths(&filter_files_by_quantization(&files, "Q8_0")),
        ["X.Q8_0.gguf"]
    );
    assert_eq!(paths(&projectors_among(&files)), ["X.mmproj-Q8_0.gguf"]);
}

#[test]
fn a_repository_of_only_projectors_has_no_quantizations() {
    let files = [gguf("mmproj-F16.gguf", 900), gguf("mmproj-Q8_0.gguf", 600)];

    assert!(aggregate_quantizations(&files).is_empty());
    assert!(filter_files_by_quantization(&files, "F16").is_empty());
    assert_eq!(
        paths(&projectors_among(&files)),
        ["mmproj-F16.gguf", "mmproj-Q8_0.gguf"]
    );
}

#[test]
fn projectors_are_listed_by_path_whatever_order_the_repository_gives() {
    let files = [
        gguf("mmproj-Q8_0.gguf", 600),
        gguf("model-Q8_0.gguf", 8_000),
        gguf("mmproj-F16.gguf", 900),
    ];

    assert_eq!(
        paths(&projectors_among(&files)),
        ["mmproj-F16.gguf", "mmproj-Q8_0.gguf"]
    );
}

/// A file is weights or a projector only when it is a GGUF file at all.
#[test]
fn a_directory_or_another_file_type_is_neither() {
    let directory = HfFileEntry {
        entry_type: HfEntryType::Directory,
        ..gguf("mmproj.gguf", 0)
    };
    let readme = gguf("mmproj-notes.md", 10);

    for entry in [directory, readme] {
        assert!(!entry.is_weights());
        assert!(!entry.is_projector());
    }
}

#[test]
fn the_port_view_keeps_path_size_and_oid() {
    let entry = HfFileEntry {
        oid: Some("abc".to_string()),
        ..gguf("mmproj-F16.gguf", 900)
    };

    let info = to_file_info(entry);

    assert_eq!(info.path, "mmproj-F16.gguf");
    assert_eq!(info.size, 900);
    assert_eq!(info.oid.as_deref(), Some("abc"));
    assert!(info.is_gguf);
}
