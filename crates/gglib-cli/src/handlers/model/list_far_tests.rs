//! The far list as a person reads it: one row per model by its id there,
//! its variants' profiles folded into that row, and how to send it a turn.

use gglib_proxy::models::ModelInfo;

use super::{footer, render, rows};

/// Entries as the far proxy lists them, `(id, gglib_id, profile, context)`.
fn listed(entries: &[(&str, i64, Option<&str>, Option<u64>)]) -> Vec<ModelInfo> {
    entries
        .iter()
        .map(|(id, gglib_id, profile, context)| {
            serde_json::from_value(serde_json::json!({
                "id": id,
                "gglib_id": gglib_id,
                "profile": profile,
                "object": "model",
                "created": 1,
                "owned_by": "gglib",
                "context_window": context,
            }))
            .unwrap()
        })
        .collect()
}

#[test]
fn a_models_variants_are_its_profiles_not_rows_of_their_own() {
    let models = listed(&[
        ("qwen3", 3, None, Some(30_000)),
        ("qwen3:coding", 3, Some("coding"), Some(30_000)),
        ("qwen3:fast", 3, Some("fast"), None),
        ("llama", 7, None, None),
    ]);

    let rows = rows(&models);
    let table = render(&rows);

    assert_eq!(rows.len(), 2, "one row per id: {table}");
    let lines: Vec<&str> = table.lines().collect();
    assert_eq!(
        lines[0].split_whitespace().collect::<Vec<_>>(),
        ["ID", "NAME", "CONTEXT", "PROFILES"]
    );
    assert_eq!(
        lines[2].split_whitespace().collect::<Vec<_>>(),
        ["3", "qwen3", "30000", "coding,", "fast"]
    );
    assert!(lines[2].ends_with("coding, fast"), "{table}");
    assert_eq!(
        lines[3].split_whitespace().collect::<Vec<_>>(),
        ["7", "llama", "-", "-"],
        "no context and no profiles are both a '-'"
    );
}

/// The ID column is as wide as the widest id, so a four-digit id does not
/// push its row's name out of line with the rest.
#[test]
fn the_id_column_is_as_wide_as_the_widest_id() {
    let models = listed(&[("small", 3, None, None), ("big", 1000, None, None)]);

    let table = render(&rows(&models));

    let lines: Vec<&str> = table.lines().collect();
    assert_eq!(lines[0].find("NAME"), lines[2].find("small"), "{table}");
    assert_eq!(lines[2].find("small"), lines[3].find("big"), "{table}");
    assert!(lines[2].starts_with("   3  "), "{table}");
    assert!(lines[3].starts_with("1000  "), "{table}");
}

/// A variant listed without its base — a pinned endpoint can list only what
/// it serves — still names its model, by the name before the profile.
#[test]
fn a_variant_without_its_base_names_the_model_it_is_of() {
    let models = listed(&[("org/qwen3:coding", 4, Some("coding"), None)]);

    let rows = rows(&models);

    assert_eq!(rows[0].name, "org/qwen3");
    assert_eq!(rows[0].profiles, ["coding"]);
}

#[test]
fn the_footer_says_how_to_chat_with_one_by_id() {
    let plain = listed(&[("qwen3", 3, None, None)]);
    let profiled = listed(&[
        ("qwen3", 3, None, None),
        ("qwen3:coding", 3, Some("coding"), None),
    ]);

    let without = footer(&rows(&plain));
    let with = footer(&rows(&profiled));

    assert!(
        without.contains("Chat with one: gglib chat <id> --remote"),
        "{without}"
    );
    assert!(!without.contains("<profile>"), "{without}");
    assert!(
        with.contains("gglib chat <id>:<profile> --remote"),
        "{with}"
    );
}
