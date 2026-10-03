//! Tests for the `gglib model inspect` formatter.

use super::*;
use std::collections::HashMap;

fn meta(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

/// The ordinary model. Almost no GGUF carries these keys, so the section
/// must cost nothing on the models that do not.
#[test]
fn a_model_publishing_nothing_gets_no_section() {
    let lines = published_sampling_lines(&meta(&[
        ("general.architecture", "qwen3"),
        ("general.quantization_version", "2"),
    ]));
    assert!(lines.is_empty(), "{lines:#?}");
}

/// A published key is shown with the gglib parameter it moves, because
/// `penalty_repeat` and `repeat_penalty` are the same knob under two
/// spellings and nothing else on screen connects them.
#[test]
fn a_published_key_names_the_gglib_parameter_it_moves() {
    let lines = published_sampling_lines(&meta(&[("general.sampling.penalty_repeat", "1.07")]));

    let row = lines
        .iter()
        .find(|l| l.contains("penalty_repeat"))
        .expect("the key is listed");
    assert!(row.contains("= 1.07"), "{row}");
    assert!(row.contains("(repeat_penalty)"), "{row}");
}

/// **The seven keys gglib does not model still move sampling.** Listing
/// only the five it compares would make an `xtc_probability` that is
/// silently reshaping output impossible to find from this command.
#[test]
fn an_unmodelled_key_is_listed_and_marked_as_unmodelled() {
    let lines = published_sampling_lines(&meta(&[
        ("general.sampling.xtc_probability", "0.5"),
        ("general.sampling.mirostat", "2"),
    ]));

    for key in ["xtc_probability", "mirostat"] {
        let row = lines
            .iter()
            .find(|l| l.contains(key))
            .unwrap_or_else(|| panic!("{key} is listed"));
        assert!(row.contains("not modelled by gglib"), "{row}");
    }
}

/// Only this prefix counts. A near-miss key is ordinary metadata and must
/// not be promoted into a section about what the server will do.
#[test]
fn only_the_general_sampling_prefix_is_collected() {
    let lines = published_sampling_lines(&meta(&[
        ("general.sampling.temp", "0.33"),
        ("qwen3.sampling.temp", "0.44"),
        ("sampling.temp", "0.55"),
        ("general.sample_count", "10"),
    ]));

    assert_eq!(
        lines.iter().filter(|l| l.contains("= 0.33")).count(),
        1,
        "{lines:#?}"
    );
    for stray in ["0.44", "0.55", "sample_count"] {
        assert!(
            !lines.iter().any(|l| l.contains(stray)),
            "{stray} must not appear in {lines:#?}"
        );
    }
}

/// The section says what the keys do and hands the override question to
/// `explain`, rather than answering it here with a second implementation.
#[test]
fn the_section_points_at_explain_for_the_override_comparison() {
    let lines = published_sampling_lines(&meta(&[("general.sampling.temp", "0.33")]));
    let text = lines.join("\n");

    assert!(text.contains("every field gglib does not send"), "{text}");
    assert!(text.contains("gglib model explain"), "{text}");
}

/// Keys are aligned and sorted, matching the raw-metadata dump below them.
#[test]
fn keys_are_sorted_and_aligned() {
    let lines = published_sampling_lines(&meta(&[
        ("general.sampling.top_k", "17"),
        ("general.sampling.temp", "0.33"),
    ]));

    let rows: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("general.sampling."))
        .collect();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].contains("temp "), "sorted: {rows:#?}");
    assert!(rows[1].contains("top_k"), "sorted: {rows:#?}");

    let equals: Vec<usize> = rows.iter().filter_map(|r| r.find(" = ")).collect();
    assert_eq!(equals[0], equals[1], "aligned: {rows:#?}");
}

// ── Projector ─────────────────────────────────────────────────────────────────

fn detail(projector: Option<&str>) -> ModelDetailDto {
    let mut new = gglib_core::NewModel::new(
        "qwen".to_owned(),
        std::path::PathBuf::from("/models/qwen.gguf"),
        7.0,
        chrono::Utc::now(),
    );
    new.projector_path = projector.map(std::path::PathBuf::from);
    ModelDetailDto::from_model(gglib_core::Model::stored(1, &new), false, None)
}

#[test]
fn a_linked_model_shows_its_projector_file() {
    assert_eq!(
        projector_line(&detail(Some("/models/mmproj-F16.gguf"))),
        "  Projector      : /models/mmproj-F16.gguf"
    );
}

#[test]
fn an_unlinked_model_says_it_has_none() {
    assert_eq!(
        projector_line(&detail(None)),
        "  Projector      : none (text only)"
    );
}

/// The paired machine's answer carries no path; the line still says the model
/// reads images.
#[test]
fn a_far_linked_model_says_linked_without_a_path() {
    let far = ModelDetailDto {
        projector_path: None,
        ..detail(Some("/models/mmproj-F16.gguf"))
    };

    assert_eq!(projector_line(&far), "  Projector      : linked");
}
