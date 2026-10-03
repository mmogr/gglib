//! `gglib model list` as a person reads it: the table, its ID column, and
//! the line about the paired machine, on a full library and an empty one.

use super::*;

/// A catalogue row with id `id`, called `name`.
fn model(id: i64, name: &str) -> GuiModel {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "name": name,
        "filePath": format!("/models/{name}.gguf"),
        "paramCountB": 8.0,
        "architecture": "qwen3",
        "quantization": "Q4_K_M",
        "contextLength": 32768,
        "addedAt": "2026-10-01 12:00:00",
        "hfRepoId": null,
    }))
    .unwrap()
}

#[test]
fn test_truncate_string_no_truncation_needed() {
    let result = truncate_string("short", 10);
    assert_eq!(result, "short");
}

#[test]
fn test_truncate_string_exact_length() {
    let result = truncate_string("exactly10c", 10);
    assert_eq!(result, "exactly10c");
}

#[test]
fn test_truncate_string_needs_truncation() {
    let result = truncate_string("this is a very long string", 10);
    // 9 chars of content + single-char ellipsis = 10 chars total
    assert_eq!(result, "this is a\u{2026}");
}

/// The ID column is as wide as the widest id, so a four-digit id keeps its
/// row's name in line with the rest.
#[test]
fn the_id_column_is_as_wide_as_the_widest_id() {
    let table = render_table(&[model(3, "small"), model(1000, "big")]);

    let lines: Vec<&str> = table.lines().collect();
    assert_eq!(lines[0].find("Name"), lines[2].find("small"), "{table}");
    assert_eq!(lines[2].find("small"), lines[3].find("big"), "{table}");
    assert_eq!(lines[1].len(), 112 + 4, "the rule spans the wider column");
    assert!(lines[2].starts_with("3    "), "{table}");
    assert!(lines[3].starts_with("1000 "), "{table}");
}

/// Short ids get a column three wide.
#[test]
fn short_ids_get_a_column_three_wide() {
    let table = render_table(&[model(3, "small")]);

    assert!(table.starts_with("ID  Name"), "{table}");
    assert_eq!(table.lines().nth(1).map(str::len), Some(115));
}

/// The paired machine's line follows the table.
#[test]
fn the_paired_line_follows_the_table() {
    let out = listing(&[model(3, "small")], Some("desk: not connected"));

    assert!(out.starts_with("Found 1 model(s):"), "{out}");
    assert!(out.ends_with("\ndesk: not connected\n"), "{out}");
}

/// An empty library still says where the models are: a laptop with none of
/// its own, paired with a desktop that has them, is what the line is for.
#[test]
fn an_empty_library_still_names_the_paired_machine() {
    let paired = "Paired with desk (direct). Its models: gglib model list --remote";

    let out = listing(&[], Some(paired));

    assert!(out.starts_with("No models found."), "{out}");
    assert!(out.ends_with(&format!("\n{paired}\n")), "{out}");
    assert_eq!(
        listing(&[], None),
        "No models found.\nUse 'gglib model add <file_path>' to add your first model.\n"
    );
}
