use super::*;
use crate::ports::huggingface::{LabelMismatch, label_refusal, match_label, match_variant};

fn labels(files: &[&str]) -> Vec<String> {
    variants_of(files.iter().map(|path| (*path, 1)))
        .into_iter()
        .map(|variant| variant.label)
        .collect()
}

#[test]
fn a_variant_is_the_shards_of_one_name_and_nothing_else() {
    let variants = variants_of([
        ("model-Q8_0-00002-of-00002.gguf", 2),
        ("model-Q8_0-00001-of-00002.gguf", 3),
        ("model-Q6_K.gguf", 5),
        ("model-Q6_K_P.gguf", 7),
        ("README.md", 11),
    ]);

    let shapes: Vec<(&str, usize, u64)> = variants
        .iter()
        .map(|v| (v.label.as_str(), v.shard_count(), v.total_size))
        .collect();
    assert_eq!(
        shapes,
        [("Q6_K", 1, 5), ("Q6_K_P", 1, 7), ("Q8_0", 2, 5)],
        "distinct names are never merged, and shards are"
    );
    assert_eq!(
        variants[2].file_paths,
        [
            "model-Q8_0-00001-of-00002.gguf",
            "model-Q8_0-00002-of-00002.gguf"
        ]
    );
    assert!(variants.iter().all(|v| v.role.is_none()));
}

#[test]
fn the_shared_prefix_is_cut_back_to_a_separator() {
    assert_eq!(
        labels(&["model-Q4_K_M.gguf", "model-Q4_K_S.gguf"]),
        ["Q4_K_M", "Q4_K_S"]
    );
    assert_eq!(
        labels(&["model.gguf", "model-Q4.gguf"]),
        ["model", "model-Q4"],
        "a name the shared part would leave empty takes no boundary from it"
    );
    assert_eq!(
        labels(&["model_Q4_K_M.gguf", "model_Q4_K_S.gguf"]),
        ["Q4_K_M", "Q4_K_S"],
        "underscores alone cut at the first"
    );
    assert_eq!(
        labels(&[
            "Phi-4-mini-instruct-Q2_K.gguf",
            "Phi-4-mini-instruct.BF16.gguf"
        ]),
        ["BF16", "Q2_K"],
        "names that part on different separators share all before them"
    );
    assert_eq!(
        labels(&["alpha-Q4.gguf", "beta-Q4.gguf"]),
        ["alpha-Q4", "beta-Q4"],
        "names with nothing in common are their own labels"
    );
    assert_eq!(
        labels(&["model_Q4_K_M.gguf", "model_Q8_0.gguf"]),
        ["Q4_K_M", "Q8_0"],
        "an underscore cuts where no dash or dot does"
    );
}

#[test]
fn a_projector_takes_no_part_in_the_prefix_and_keeps_its_whole_name() {
    assert_eq!(
        labels(&[
            "Qwen-7B-Q4_K_M.gguf",
            "Qwen-7B-Q8_0.gguf",
            "mmproj-Qwen-7B-f16.gguf"
        ]),
        ["Q4_K_M", "Q8_0", "mmproj-Qwen-7B-F16"]
    );
    assert_eq!(
        labels(&["gemma-3-4b-it-Q4_K_M.gguf", "mmproj-model-f16.gguf"]),
        ["gemma-3-4b-it-Q4_K_M", "mmproj-model-F16"],
        "one model variant beside a projector is labelled by its whole name"
    );
}

#[test]
fn a_name_several_folders_hold_is_labelled_by_its_folder() {
    assert_eq!(
        labels(&[
            "Q4_K_M/model.gguf",
            "Q8_0/model-00001-of-00002.gguf",
            "Q8_0/model-00002-of-00002.gguf",
            "model-IQ2_M.gguf"
        ]),
        ["Q4_K_M", "Q8_0", "model-IQ2_M"]
    );
}

#[test]
fn a_lone_variant_is_its_whole_name_and_reserved_characters_become_dashes() {
    assert_eq!(labels(&["gemma-3-1b-it-q4_0.gguf"]), ["gemma-3-1b-it-Q4_0"]);
    assert_eq!(labels(&["a:b@c.gguf"]), ["a-b-c"]);
}

/// A token that spells a quantisation is written in capitals, whoever
/// uploaded it, and every other token as its uploader wrote it.
#[test]
fn a_label_spells_its_quantisation_in_capitals() {
    assert_eq!(
        labels(&[
            "model-INSTRUCT-Q6_k.gguf",
            "model-Thinking-q4_k_s.gguf",
            "model-iq4_xs.gguf",
            "model.bf16.gguf",
            "model-UD-tq1_0.gguf",
        ]),
        [
            "BF16",
            "INSTRUCT-Q6_K",
            "IQ4_XS",
            "Thinking-Q4_K_S",
            "UD-TQ1_0"
        ]
    );
    assert_eq!(
        labels(&["model-FastMTP-32K.gguf", "model-Qwen3-q8_0.gguf"]),
        ["FastMTP-32K", "Qwen3-Q8_0"],
        "a token that only starts like one is kept"
    );
    for kept in [
        "Q",
        "Q4_KXLL",
        "Q_4",
        "IQ",
        "f8",
        "Qwen3",
        "q4_k_m_x_y_zzzz",
    ] {
        assert!(!spells_a_quantization(kept), "{kept}");
    }
}

/// A projector's and an importance matrix's file are not said as
/// quantisations; a draft or MTP head is.
#[test]
fn a_projector_and_an_importance_matrix_are_not_said_as_quantisations() {
    let said: Vec<String> = variants_of(
        [
            "model-Q4_K_M.gguf",
            "model-Q8_0.gguf",
            "mmproj-model-F16.gguf",
            "model.imatrix.gguf",
            "imatrix_unsloth.gguf",
            "model-MTP-Q8_0.gguf",
        ]
        .map(|path| (path, 1)),
    )
    .into_iter()
    .filter(said_as_a_quantization)
    .map(|variant| variant.label)
    .collect();

    assert_eq!(said, ["MTP-Q8_0", "Q4_K_M", "Q8_0"]);
}

#[test]
fn a_request_matches_a_label_exactly_ignoring_case_first() {
    let labels = ["Q2_K", "Q2_K_S", "Q2_K_P", "Q4_0", "Q4_0_4_4"];
    assert_eq!(match_label(labels, "q2_k"), Ok("Q2_K"));
    assert_eq!(match_label(labels, "Q4_0"), Ok("Q4_0"));
    assert_eq!(match_label(labels, "Q4_0_4_4"), Ok("Q4_0_4_4"));
}

#[test]
fn else_the_one_label_that_holds_it_as_a_whole_token() {
    let labels = ["UD-Q4_K_XL", "Q4_K_M", "Q6_K_P", "BF16", "mmproj-F16"];
    assert_eq!(match_label(labels, "q4_k_xl"), Ok("UD-Q4_K_XL"));
    assert_eq!(
        match_label(labels, "Q6_K"),
        Err(LabelMismatch::None),
        "an underscore joins a quant's parts: Q6_K is no token of Q6_K_P"
    );
    assert_eq!(match_label(labels, "F16"), Ok("mmproj-F16"));
    assert_eq!(
        match_label(labels, "Q4_K_X"),
        Err(LabelMismatch::None),
        "a token is whole"
    );
    assert_eq!(match_label(labels, "Q4"), Err(LabelMismatch::None));
    assert_eq!(
        match_label(["x-Q4-a", "x-Q4-b"], "Q4"),
        Err(LabelMismatch::Ambiguous(vec![
            "x-Q4-a".to_owned(),
            "x-Q4-b".to_owned()
        ]))
    );
    assert_eq!(match_label(labels, ""), Err(LabelMismatch::None));
}

#[test]
fn a_model_and_its_projector_holding_the_request_name_the_model() {
    let labels = ["X.Q8_0", "X.mmproj-Q8_0", "Y.Q4_0"];
    assert_eq!(match_label(labels, "Q8_0"), Ok("X.Q8_0"));
    assert_eq!(match_label(labels, "mmproj-Q8_0"), Ok("X.mmproj-Q8_0"));
}

#[test]
fn labels_that_differ_by_case_alone_need_the_exact_one() {
    let labels = ["q8_k", "Q8_K"];
    assert_eq!(match_label(labels, "Q8_K"), Ok("Q8_K"));
    assert_eq!(match_label(labels, "q8_k"), Ok("q8_k"));
    assert!(matches!(
        match_label(labels, "q8_K"),
        Err(LabelMismatch::Ambiguous(_))
    ));
}

#[test]
fn a_refusal_lists_the_labels() {
    let none = label_refusal("o/r", "Q9", &["Q4_K_M", "Q8_0"], &LabelMismatch::None);
    assert_eq!(
        none,
        "No variant of o/r is labelled 'Q9'. Its variants: Q4_K_M, Q8_0"
    );
    let several = label_refusal(
        "o/r",
        "Q4",
        &[],
        &LabelMismatch::Ambiguous(vec!["Q4_0".into(), "Q4_K_M".into()]),
    );
    assert!(several.contains("Q4_0, Q4_K_M"), "{several}");
}

/// A whole name the shared part is cut from takes no boundary from it: the
/// pairs of one quant and its longer neighbour keep their quants whole.
#[test]
fn a_name_that_is_the_whole_shared_part_keeps_its_quant_whole() {
    assert_eq!(
        labels(&["model-Q6_K.gguf", "model-Q6_K_P.gguf"]),
        ["Q6_K", "Q6_K_P"]
    );
    assert_eq!(
        labels(&["model-Q4_0.gguf", "model-Q4_0_4_4.gguf"]),
        ["Q4_0", "Q4_0_4_4"]
    );
}

/// The shard total is part of what a variant is: a file and a set of
/// shards of one name, or two sets of different totals, are two variants,
/// each requestable by its own label.
#[test]
fn sets_of_different_shard_totals_are_different_variants() {
    let variants = variants_of([
        ("model-Q8_0.gguf", 1),
        ("model-Q8_0-00001-of-00002.gguf", 2),
        ("model-Q8_0-00002-of-00002.gguf", 2),
        ("x-00001-of-00002.gguf", 3),
        ("x-00002-of-00002.gguf", 3),
        ("x-00001-of-00003.gguf", 4),
        ("x-00002-of-00003.gguf", 4),
        ("x-00003-of-00003.gguf", 4),
    ]);

    let shapes: Vec<(&str, usize)> = variants
        .iter()
        .map(|v| (v.label.as_str(), v.shard_count()))
        .collect();
    assert_eq!(
        shapes,
        [
            ("model-Q8_0", 1),
            ("model-Q8_0-of-00002", 2),
            ("x-of-00002", 2),
            ("x-of-00003", 3)
        ]
    );
}

/// Two variants whose labels would read alike are each labelled by their
/// whole key, and keys that read alike once written as labels are numbered.
#[test]
fn labels_that_would_clash_are_made_distinct() {
    assert_eq!(
        labels(&["Q8_0/model.gguf", "Q4_0/model.gguf", "Q8_0.gguf"]),
        ["Q4_0", "Q8_0", "Q8_0-model"],
        "a folder's label that is another's name takes its whole key"
    );
    for clashing in [
        labels(&["a:b/model.gguf", "a-b/model.gguf"]),
        labels(&["Q8_0/model.gguf", "q8_0/model.gguf"]),
    ] {
        assert_eq!(clashing.len(), 2);
        assert_ne!(
            clashing[0].to_lowercase(),
            clashing[1].to_lowercase(),
            "{clashing:?}"
        );
    }
}

/// A request no label names is matched against the variants' whole file
/// names.
#[test]
fn a_request_no_label_names_is_matched_against_the_file_names() {
    let variants = variants_of([("Org-Model-Q4_K_M.gguf", 1), ("Org-Model-Q8_0.gguf", 1)]);

    let by_name = match_variant(&variants, "Org-Model-Q8_0").unwrap();
    assert_eq!(by_name.label, "Q8_0");
    assert_eq!(
        match_variant(&variants, "Model").map(|v| v.label.as_str()),
        Err(LabelMismatch::Ambiguous(vec![
            "Q4_K_M".to_owned(),
            "Q8_0".to_owned()
        ]))
    );
}

/// Names joined by underscores alone are cut at the first, so a label
/// keeps the rest of the name; a request for the quant still finds its
/// variant by the end of its file name, and a quant's longer neighbour is
/// still not named by it.
#[test]
fn a_quant_at_the_end_of_an_underscored_name_names_its_variant() {
    let mine = variants_of([("my_model_Q4_K_M.gguf", 1), ("my_model_Q8_0.gguf", 1)]);
    assert_eq!(
        mine.iter().map(|v| v.label.as_str()).collect::<Vec<_>>(),
        ["model_Q4_K_M", "model_Q8_0"]
    );
    assert_eq!(
        match_variant(&mine, "Q4_K_M").unwrap().label,
        "model_Q4_K_M"
    );
    assert_eq!(match_variant(&mine, "q8_0").unwrap().label, "model_Q8_0");

    let llama = variants_of([
        ("llama_3_8b_Q4_K_M.gguf", 1),
        ("llama_3_8b_Q5_K_M.gguf", 1),
        ("llama_3_8b_Q8_0.gguf", 1),
    ]);
    for quant in ["Q4_K_M", "Q5_K_M", "Q8_0"] {
        let chosen = match_variant(&llama, quant).unwrap();
        assert_eq!(chosen.file_paths, [format!("llama_3_8b_{quant}.gguf")]);
    }

    let neighbours = variants_of([("my_model_Q6_K_P.gguf", 1), ("my_model_Q8_0.gguf", 1)]);
    assert_eq!(
        match_variant(&neighbours, "Q6_K").map(|v| v.label.as_str()),
        Err(LabelMismatch::None)
    );
}
