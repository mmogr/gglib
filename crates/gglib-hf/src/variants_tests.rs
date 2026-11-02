//! A repository's variants, grouped and labelled from real Hub listings.
//!
//! Each fixture is a repository's tree as
//! `https://huggingface.co/api/models/<repo>/tree/main?recursive=true`
//! answered it on 2026-11-02, kept to each entry's type, path and size.

use gglib_core::ports::huggingface::{HfQuantInfo, match_label};

use super::{parse_tree_entries, variant_files, variants};
use crate::models::{HfEntryType, HfFileEntry};

/// The variants of the listing saved as `name`.
fn listed(tree: &str) -> Vec<HfQuantInfo> {
    let tree: serde_json::Value = serde_json::from_str(tree).unwrap();
    variants(&parse_tree_entries(&tree).unwrap())
}

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("variants_fixtures/", $name, ".json"))
    };
}

fn labels(variants: &[HfQuantInfo]) -> Vec<&str> {
    variants.iter().map(|v| v.label.as_str()).collect()
}

fn shards(variants: &[HfQuantInfo], label: &str) -> usize {
    variants
        .iter()
        .find(|v| v.label == label)
        .unwrap_or_else(|| panic!("no {label}"))
        .shard_count()
}

#[test]
fn hauhaucs_keeps_its_k_p_line_its_iq_line_and_its_draft() {
    let variants = listed(fixture!(
        "HauhauCS_Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-MTP-GGUF"
    ));

    assert_eq!(
        labels(&variants),
        [
            "FastMTP-32K",
            "IQ2_M",
            "IQ3_M",
            "IQ3_XS",
            "IQ4_XS",
            "Q2_K_P",
            "Q3_K_P",
            "Q4_K_P",
            "Q5_K_P",
            "Q6_K_P",
            "Q8_K_P",
            "mmproj-Qwen3.8-27B-Uncensored-HauhauCS-Aggressive-BF16",
        ]
    );
    assert!(variants.iter().all(|v| v.shard_count() == 1));
}

#[test]
fn unsloth_keeps_its_dynamic_quants_apart_from_the_plain_ones() {
    let variants = listed(fixture!("unsloth_Qwen3-30B-A3B-GGUF"));

    let labels = labels(&variants);
    for label in [
        "UD-Q4_K_XL",
        "UD-Q8_K_XL",
        "UD-IQ1_M",
        "Q4_K_M",
        "Q8_0",
        "IQ4_NL",
    ] {
        assert!(labels.contains(&label), "{label} in {labels:?}");
    }
    assert_eq!(shards(&variants, "BF16"), 2);
    assert_eq!(variants.len(), 26);
}

#[test]
fn deepseek_v3_1_is_a_folder_of_shards_per_variant() {
    let variants = listed(fixture!("unsloth_DeepSeek-V3.1-GGUF"));

    assert_eq!(shards(&variants, "UD-Q8_K_XL"), 17);
    assert_eq!(shards(&variants, "BF16"), 30);
    assert_eq!(shards(&variants, "UD-TQ1_0"), 1);
    assert_eq!(shards(&variants, "Q2_K_L"), 5);
    assert_eq!(variants.len(), 27);
    let bf16 = variants.iter().find(|v| v.label == "BF16").unwrap();
    assert_eq!(
        bf16.file_paths.first().map(String::as_str),
        Some("BF16/DeepSeek-V3.1-BF16-00001-of-00030.gguf")
    );
}

#[test]
fn bartowski_labels_every_variant_by_its_quant() {
    let variants = listed(fixture!("bartowski_Qwen_Qwen3-30B-A3B-GGUF"));

    assert_eq!(
        labels(&variants),
        [
            "IQ2_M", "IQ2_S", "IQ2_XS", "IQ2_XXS", "IQ3_M", "IQ3_XS", "IQ3_XXS", "IQ4_NL",
            "IQ4_XS", "Q2_K", "Q2_K_L", "Q3_K_L", "Q3_K_M", "Q3_K_S", "Q3_K_XL", "Q4_0", "Q4_1",
            "Q4_K_L", "Q4_K_M", "Q4_K_S", "Q5_K_L", "Q5_K_M", "Q5_K_S", "Q6_K", "Q6_K_L", "Q8_0",
            "bf16",
        ]
    );
    assert_eq!(shards(&variants, "bf16"), 2);
}

#[test]
fn mradermacher_imatrix_quants_are_labelled_past_the_i1_prefix() {
    let variants = listed(fixture!("mradermacher_Qwen3-30B-A3B-i1-GGUF"));

    assert_eq!(
        labels(&variants),
        [
            "IQ3_M", "IQ3_S", "IQ3_XS", "IQ3_XXS", "IQ4_XS", "Q2_K", "Q3_K_L", "Q3_K_M", "Q3_K_S",
            "Q4_0", "Q4_1", "Q4_K_M", "Q4_K_S", "Q5_K_M", "Q5_K_S", "Q6_K",
        ]
    );
}

#[test]
fn mia_keeps_each_mtp_head_beside_its_model() {
    let variants = listed(fixture!("Mia-AiLab_Gemmable-4-12B-MTP-GGUF"));

    assert_eq!(
        labels(&variants),
        [
            "Q4_K_M",
            "Q4_K_M-mtp",
            "Q6_K",
            "Q6_K-mtp",
            "Q8_0",
            "Q8_0-mtp",
            "fp16-mtp"
        ]
    );
    let labels = labels(&variants);
    assert_eq!(match_label(labels.iter().copied(), "Q8_0"), Ok("Q8_0"));
}

#[test]
fn unsloth_mtp_models_are_labelled_with_their_ud_prefix() {
    let variants = listed(fixture!("unsloth_Qwen3.6-35B-A3B-MTP-GGUF"));

    let labels = labels(&variants);
    for label in [
        "UD-Q4_K_XL",
        "UD-Q6_K",
        "MXFP4_MOE",
        "Q8_0",
        "BF16",
        "mmproj-BF16",
        "mmproj-F16",
        "mmproj-F32",
    ] {
        assert!(labels.contains(&label), "{label} in {labels:?}");
    }
    assert_eq!(shards(&variants, "BF16"), 2);
    assert_eq!(
        match_label(labels.iter().copied(), "ud-q4_k_xl"),
        Ok("UD-Q4_K_XL")
    );
    assert_eq!(
        match_label(labels.iter().copied(), "Q4_K_XL"),
        Ok("UD-Q4_K_XL")
    );
    assert_eq!(match_label(labels.iter().copied(), "bf16"), Ok("BF16"));
}

#[test]
fn a_lone_variant_is_labelled_by_its_whole_name() {
    let variants = listed(fixture!("google_gemma-3-1b-it-qat-q4_0-gguf"));

    assert_eq!(labels(&variants), ["gemma-3-1b-it-q4_0"]);
}

fn gguf(path: &str) -> HfFileEntry {
    HfFileEntry {
        path: path.to_owned(),
        entry_type: HfEntryType::File,
        size: 1,
        oid: None,
    }
}

/// The pairs a quant pattern once merged as shards of one download stay
/// apart, and each is asked for by its own label.
#[test]
fn neighbouring_quants_stay_distinct_variants() {
    let files: Vec<HfFileEntry> = [
        "model-Q6_K.gguf",
        "model-Q6_K_P.gguf",
        "model-Q2_K.gguf",
        "model-Q2_K_S.gguf",
        "model-Q2_K_P.gguf",
        "model-Q4_0.gguf",
        "model-Q4_0_4_4.gguf",
        "model-UD-Q4_K_XL.gguf",
        "model-Q4_K_XL.gguf",
    ]
    .into_iter()
    .map(gguf)
    .collect();

    let variants = variants(&files);

    assert!(variants.iter().all(|v| v.shard_count() == 1));
    for label in [
        "Q6_K",
        "Q6_K_P",
        "Q2_K",
        "Q2_K_S",
        "Q2_K_P",
        "Q4_0",
        "Q4_0_4_4",
        "UD-Q4_K_XL",
        "Q4_K_XL",
    ] {
        let files = variant_files(&files, label).unwrap();
        assert_eq!(files.len(), 1, "{label}");
        assert_eq!(files[0].path, format!("model-{label}.gguf"));
    }
}

/// A repository of just a quant and its longer neighbour: the shared part
/// is the whole of one name, and each keeps its quant whole and is asked
/// for by it, never the other.
#[test]
fn a_two_file_repository_of_neighbouring_quants_keeps_both_whole() {
    for (short, long) in [("Q6_K", "Q6_K_P"), ("Q4_0", "Q4_0_4_4")] {
        let files = [
            gguf(&format!("model-{short}.gguf")),
            gguf(&format!("model-{long}.gguf")),
        ];

        let variants = variants(&files);

        assert_eq!(labels(&variants), [short, long]);
        for label in [short, long] {
            let chosen = variant_files(&files, label).unwrap();
            assert_eq!(chosen[0].path, format!("model-{label}.gguf"));
        }
    }
}
