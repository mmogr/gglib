//! Tests that a projector and a sidecar are their own variants, never a
//! shard of the weights, and that the projectors are listed apart.

use gglib_core::download::GgufFileRole;
use gglib_core::ports::huggingface::HfQuantInfo;

use super::*;
use crate::models::HfEntryType;
use crate::parsing::{parse_tree_entries, variant_files, variants};

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

fn labels(variants: &[HfQuantInfo]) -> Vec<&str> {
    variants.iter().map(|q| q.label.as_str()).collect()
}

fn paths(files: &[HfFileEntry]) -> Vec<&str> {
    files.iter().map(|f| f.path.as_str()).collect()
}

fn files_of<'a>(variants: &'a [HfQuantInfo], label: &str) -> &'a [String] {
    &variants
        .iter()
        .find(|q| q.label == label)
        .unwrap_or_else(|| panic!("no {label} variant"))
        .file_paths
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

/// Each projector is a variant of its own, and `BF16` is its two weights
/// shards without `mmproj-BF16.gguf` as a third.
#[test]
fn no_projector_of_the_real_listing_is_a_shard_of_the_weights() {
    let variants = variants(&qwen_files());

    assert_eq!(files_of(&variants, "mmproj-BF16"), ["mmproj-BF16.gguf"]);
    assert_eq!(files_of(&variants, "mmproj-F16"), ["mmproj-F16.gguf"]);
    assert_eq!(
        files_of(&variants, "BF16"),
        [
            "BF16/Qwen3.8-27B-BF16-00001-of-00002.gguf",
            "BF16/Qwen3.8-27B-BF16-00002-of-00002.gguf"
        ]
    );
    let bf16 = variants.iter().find(|q| q.label == "BF16").unwrap();
    assert_eq!(bf16.shard_count(), 2);
    assert_eq!(bf16.total_size, 49_986_159_616 + 4_671_576_000);
    assert_eq!(files_of(&variants, "Q8_0"), ["Qwen3.8-27B-Q8_0.gguf"]);
}

/// The importance matrix and the MTP head are variants of their own, named
/// whole: their names take no part in the prefix the others share.
#[test]
fn no_sidecar_of_the_real_listing_is_a_shard_of_the_weights() {
    let variants = variants(&qwen_files());

    assert_eq!(files_of(&variants, "Q4_0"), ["Qwen3.8-27B-Q4_0.gguf"]);
    assert_eq!(files_of(&variants, "UD-Q6_K"), ["Qwen3.8-27B-UD-Q6_K.gguf"]);
    assert_eq!(
        files_of(&variants, "UD-Q6_K_M"),
        ["Qwen3.8-27B-UD-Q6_K_M.gguf"]
    );
    assert_eq!(
        files_of(&variants, "imatrix_unsloth"),
        ["imatrix_unsloth.gguf"]
    );
    assert_eq!(
        files_of(&variants, "mtp-Qwen3.8-27B-Q4_0"),
        ["MTP/mtp-Qwen3.8-27B-Q4_0.gguf"]
    );
    assert_eq!(variants.len(), 30 - 1, "the two BF16 shards are one");
}

#[test]
fn the_files_a_label_names_in_the_real_listing() {
    let files = qwen_files();

    assert_eq!(
        paths(&variant_files(&files, "bf16").unwrap()),
        [
            "BF16/Qwen3.8-27B-BF16-00001-of-00002.gguf",
            "BF16/Qwen3.8-27B-BF16-00002-of-00002.gguf"
        ]
    );
    assert_eq!(
        paths(&variant_files(&files, "Q4_K_XL").unwrap()),
        ["Qwen3.8-27B-UD-Q4_K_XL.gguf"]
    );
    assert_eq!(
        paths(&variant_files(&files, "F16").unwrap()),
        ["mmproj-F16.gguf"],
        "the one label holding F16 is the projector's, which its header refuses"
    );
    assert!(variant_files(&files, "Q4_K").is_none(), "several hold it");
}

/// The sidecars' names, which the role a header gives replaces wherever a
/// variant is judged.
#[test]
fn the_real_listing_names_two_sidecars() {
    let sidecars: Vec<HfFileEntry> = qwen_files()
        .into_iter()
        .filter(|file| GgufFileRole::classify(Path::new(&file.path)) == GgufFileRole::Sidecar)
        .collect();

    assert_eq!(
        paths(&sidecars),
        ["MTP/mtp-Qwen3.8-27B-Q4_0.gguf", "imatrix_unsloth.gguf"]
    );
    assert!(sidecars.iter().all(|sidecar| !sidecar.is_projector()));
}

// ── The shapes a repository takes ────────────────────────────────────────

#[test]
fn a_projector_beside_one_model_is_a_variant_apart() {
    let files = [
        gguf("model-Q4_K_M.gguf", 4_000),
        gguf("mmproj-F16.gguf", 900),
    ];

    let variants = variants(&files);

    assert_eq!(labels(&variants), ["mmproj-F16", "model-Q4_K_M"]);
    assert_eq!(files_of(&variants, "model-Q4_K_M"), ["model-Q4_K_M.gguf"]);
    assert_eq!(paths(&projectors_among(&files)), ["mmproj-F16.gguf"]);
}

/// The shape a library downloaded before the split holds: the projector was
/// stored as the second shard of `Q8_0`.
#[test]
fn a_projector_of_the_same_quantization_is_not_a_second_shard() {
    let files = [gguf("X.Q8_0.gguf", 8_000), gguf("X.mmproj-Q8_0.gguf", 600)];

    let variants = variants(&files);

    assert_eq!(files_of(&variants, "X.Q8_0"), ["X.Q8_0.gguf"]);
    assert_eq!(
        paths(&variant_files(&files, "X.Q8_0").unwrap()),
        ["X.Q8_0.gguf"]
    );
    assert_eq!(paths(&projectors_among(&files)), ["X.mmproj-Q8_0.gguf"]);
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

/// A file is a projector only when it is a GGUF file at all.
#[test]
fn a_directory_or_another_file_type_is_no_projector() {
    let directory = HfFileEntry {
        entry_type: HfEntryType::Directory,
        ..gguf("mmproj.gguf", 0)
    };
    let readme = gguf("mmproj-notes.md", 10);

    for entry in [directory, readme] {
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
