//! Tests for grouping display rows into sections.
//!
//! Its own sibling rather than more of `settings_display_tests.rs`, which is
//! itself already at the size budget: `settings_to_sections` is a separate
//! function with a separate contract, and splitting on that line is what
//! keeps either file readable.

use super::settings_to_sections;

#[test]
fn settings_to_sections_groups_correctly() {
    let flat = vec![
        ("default-context-size".to_owned(), "4096".to_owned()),
        (
            "inference-defaults.temperature".to_owned(),
            "0.75".to_owned(),
        ),
        ("inference-defaults.top-k".to_owned(), "20".to_owned()),
        ("proxy-port".to_owned(), "8080".to_owned()),
    ];

    let sections = settings_to_sections(&flat);

    assert_eq!(sections.len(), 2);

    let general = &sections[0];
    assert_eq!(general.title, "General");
    assert!(
        general
            .rows
            .iter()
            .any(|(k, _)| k == "default-context-size")
    );
    assert!(general.rows.iter().any(|(k, _)| k == "proxy-port"));

    // Inference prefix stripped.
    let inference = &sections[1];
    assert_eq!(inference.title, "Inference Defaults");
    assert!(inference.rows.iter().any(|(k, _)| k == "temperature"));
    assert!(inference.rows.iter().any(|(k, _)| k == "top-k"));
}

#[test]
fn settings_to_sections_inference_null_shows_placeholder() {
    let flat = vec![
        ("default-context-size".to_owned(), "4096".to_owned()),
        ("inference-defaults".to_owned(), "None".to_owned()),
    ];

    let sections = settings_to_sections(&flat);
    let inference = sections
        .iter()
        .find(|s| s.title == "Inference Defaults")
        .expect("Inference Defaults section should be present");

    assert_eq!(inference.rows.len(), 1);
    assert_eq!(inference.rows[0].0, "(none configured)");
    assert_eq!(inference.rows[0].1, "");
}
