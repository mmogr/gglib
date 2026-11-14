use super::match_variant;
use crate::ports::huggingface::{HfQuantInfo, LabelMismatch, variants_of};

/// The label of the variant of `files` that `request` names.
fn named(files: &[&str], request: &str) -> Result<String, LabelMismatch> {
    let variants = variants_of(files.iter().map(|path| (*path, 1)));
    match_variant(&variants, request).map(|variant| variant.label.clone())
}

/// A pasted file URL asks by its file's stem, less a shard's numbering:
/// that names the variant holding the file, plain, sharded or in a folder.
#[test]
fn a_file_stem_names_the_variant_holding_the_file() {
    let mut files: Vec<String> = vec!["DeepSeek-V3.1-Q6_K.gguf".to_owned()];
    for (dir, name) in [
        ("UD-Q4_K_XL/", "DeepSeek-V3.1-UD-Q4_K_XL"),
        ("Q4_K_XL/", "DeepSeek-V3.1-Q4_K_XL"),
        ("", "DeepSeek-V3.1-Q8_0"),
    ] {
        files.extend((1..=2).map(|i| format!("{dir}{name}-0000{i}-of-00002.gguf")));
    }
    let variants = variants_of(files.iter().map(|path| (path.as_str(), 1)));

    for (stem, first) in [
        (
            "DeepSeek-V3.1-UD-Q4_K_XL",
            "UD-Q4_K_XL/DeepSeek-V3.1-UD-Q4_K_XL-00001-of-00002.gguf",
        ),
        (
            "deepseek-v3.1-q4_k_xl",
            "Q4_K_XL/DeepSeek-V3.1-Q4_K_XL-00001-of-00002.gguf",
        ),
        (
            "DeepSeek-V3.1-Q8_0",
            "DeepSeek-V3.1-Q8_0-00001-of-00002.gguf",
        ),
        ("DeepSeek-V3.1-Q6_K", "DeepSeek-V3.1-Q6_K.gguf"),
    ] {
        let named = match_variant(&variants, stem).unwrap();
        assert_eq!(named.file_paths[0], first, "{stem}");
    }
}

/// A projector's label is its whole file name, which holds the model's
/// name as a token; the model's file name, asked whole, is the model's.
#[test]
fn a_model_file_name_is_not_the_projector_that_holds_it() {
    let files = [
        "Qwen2-VL-7B-Instruct-f16.gguf",
        "Qwen2-VL-7B-Instruct-Q4_K_M.gguf",
        "mmproj-Qwen2-VL-7B-Instruct-f16.gguf",
    ];
    let variants = variants_of(files.iter().map(|path| (*path, 1)));
    let labels: Vec<&str> = variants.iter().map(|v| v.label.as_str()).collect();
    assert!(
        labels.contains(&"mmproj-Qwen2-VL-7B-Instruct-F16"),
        "{labels:?}"
    );

    assert_eq!(named(&files, "Qwen2-VL-7B-Instruct-f16").unwrap(), "F16");
}

/// An MTP draft's file named after a model's holds the model's name, as a
/// token of its label (`mtp-Model-Q8_0`) or of its file name
/// (`Model-Q4_K_M-mtp`); the model's file name is the model's.
#[test]
fn a_model_file_name_is_not_the_draft_that_holds_it() {
    let files = [
        "Model-Q4_K_M.gguf",
        "Model-Q8_0.gguf",
        "mtp-Model-Q8_0.gguf",
        "Model-Q4_K_M-mtp.gguf",
    ];

    assert_eq!(named(&files, "Model-Q8_0").unwrap(), "Q8_0");
    assert_eq!(named(&files, "Model-Q4_K_M").unwrap(), "Q4_K_M");
}

/// A file name asked whole names its own variant, not every variant whose
/// name holds it (`Q8_0` beside `Q8_0-4_4`): with its shards' numbering
/// and its folder left out of the name it is asked by.
#[test]
fn a_whole_file_name_beats_a_longer_one_holding_it() {
    assert_eq!(
        named(&["model-Q8_0.gguf", "model-Q8_0-4_4.gguf"], "model-Q8_0").unwrap(),
        "Q8_0"
    );
    let sharded = [
        "Q8_0/model-Q8_0-00001-of-00002.gguf",
        "Q8_0/model-Q8_0-00002-of-00002.gguf",
        "model-Q8_0-4_4.gguf",
    ];
    let label = named(&sharded, "model-Q8_0").unwrap();
    assert!(!label.contains("4_4"), "{label}");
}

/// A variant labelled `label`, of the one file `path`.
fn variant(label: &str, path: &str) -> HfQuantInfo {
    HfQuantInfo {
        label: label.to_owned(),
        role: None,
        file_paths: vec![path.to_owned()],
        total_size: 1,
    }
}

/// A label is looked for before any file name: a request that is one
/// variant's label names that variant, though it is another's file name.
#[test]
fn a_label_names_its_variant_before_another_variants_file_name() {
    let variants = [
        variant("Q8_0", "model-Q8_0.gguf"),
        variant("model-Q8_0", "folder/zeta.gguf"),
    ];

    let named = match_variant(&variants, "model-Q8_0").unwrap();

    assert_eq!(named.file_paths, ["folder/zeta.gguf"]);
}

/// A whole file name names its variant ignoring case, where its tokens
/// would name two.
#[test]
fn a_whole_file_name_is_matched_ignoring_case() {
    let variants = [
        variant("Q4_K_M", "model-Q4_K_M.gguf"),
        variant("Q4_K_M-x", "model-Q4_K_M.x.gguf"),
    ];

    let named = match_variant(&variants, "MODEL-q4_k_m").unwrap();

    assert_eq!(named.label, "Q4_K_M");
}
