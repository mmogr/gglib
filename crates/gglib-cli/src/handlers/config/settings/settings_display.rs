//! Display and formatting logic for CLI settings output.
//!
//! Extracted from `settings.rs` to keep command-routing code lean.
//! All presentation concerns — row generation, section grouping, and
//! column alignment — live here.

use gglib_core::Settings;

/// Keys that are purely internal bookkeeping and must not appear in
/// user-facing output.
const HIDDEN_KEYS: &[&str] = &["setup-completed"];

/// A labeled group of display rows used by [`print_sections`].
pub(super) struct DisplaySection {
    pub title: &'static str,
    pub rows: Vec<(String, String)>,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Convert a camelCase identifier to kebab-case.
///
/// `topK` → `top-k`, `maxTokens` → `max-tokens`, `topP` → `top-p`
fn camel_to_kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i != 0 {
            out.push('-');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

/// Format a [`serde_json::Value`] leaf as a human-readable string.
fn format_leaf(val: serde_json::Value) -> String {
    match val {
        serde_json::Value::Null => "None".to_owned(),
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    }
}

/// Recursively collect `(key, value)` rows from a JSON value.
///
/// - **Scalars** (null, bool, number, string) emit one row with the parent key.
/// - **Objects** recurse: each child key is converted from camelCase to
///   kebab-case and prefixed with `{parent_key}.`.
/// - **Arrays** are formatted as their JSON string representation.
fn collect_rows(parent_key: &str, val: serde_json::Value, rows: &mut Vec<(String, String)>) {
    match val {
        serde_json::Value::Object(map) => {
            for (child_key, child_val) in map {
                let kebab_child = camel_to_kebab(&child_key);
                let full_key = format!("{parent_key}.{kebab_child}");
                collect_rows(&full_key, child_val, rows);
            }
        }
        leaf => rows.push((parent_key.to_owned(), format_leaf(leaf))),
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Build display rows for a [`Settings`] value as `(key, value)` pairs.
///
/// Keys are kebab-case. Nested structs are expanded into dot-notation sub-rows
/// (e.g. `inference-defaults.temperature`). Rows for [`HIDDEN_KEYS`] are
/// silently dropped.
///
/// `default-model-id` is substituted with the pre-resolved `model_display`
/// string (or `"None"`) to avoid a DB round-trip inside this pure function.
pub(super) fn settings_display_rows(
    settings: &Settings,
    model_display: Option<String>,
) -> Vec<(String, String)> {
    let obj = match serde_json::to_value(settings) {
        Ok(serde_json::Value::Object(m)) => m,
        _ => return Vec::new(),
    };

    let mut rows: Vec<(String, String)> = Vec::new();

    for (snake_key, val) in obj {
        let kebab_key = snake_key.replace('_', "-");

        if HIDDEN_KEYS.contains(&kebab_key.as_str()) {
            continue;
        }

        if kebab_key == "default-model-id" {
            let display = model_display.clone().unwrap_or_else(|| "None".to_owned());
            rows.push((kebab_key, display));
        } else {
            collect_rows(&kebab_key, val, &mut rows);
        }
    }

    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}

/// Group flat display rows into labeled [`DisplaySection`]s.
///
/// Grouping rules (applied in order):
/// - `inference-defaults.*` → **Inference Defaults** (prefix stripped from key)
/// - Bare `inference-defaults` (null) → **Inference Defaults**, shown as
///   `(none configured)`
/// - Anything else → **General**
pub(super) fn settings_to_sections(flat_rows: &[(String, String)]) -> Vec<DisplaySection> {
    let mut general: Vec<(String, String)> = Vec::new();
    let mut inference: Vec<(String, String)> = Vec::new();

    for (key, val) in flat_rows {
        if let Some(sub) = key.strip_prefix("inference-defaults.") {
            inference.push((sub.to_owned(), val.clone()));
        } else if key == "inference-defaults" {
            // The whole nested struct is unset; show a placeholder.
            inference.push(("(none configured)".to_owned(), String::new()));
        } else {
            general.push((key.clone(), val.clone()));
        }
    }

    let mut sections = Vec::new();
    if !general.is_empty() {
        sections.push(DisplaySection {
            title: "General",
            rows: general,
        });
    }
    if !inference.is_empty() {
        sections.push(DisplaySection {
            title: "Inference Defaults",
            rows: inference,
        });
    }
    sections
}

/// Print labeled sections with a separator and per-section column alignment.
///
/// Each section is preceded by a blank line so the output breathes.
pub(super) fn print_sections(sections: &[DisplaySection]) {
    for section in sections {
        let max_key_len = section.rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
        let sep_width = (max_key_len + 4).max(40);

        println!();
        println!("  {}", section.title);
        println!("  {}", "─".repeat(sep_width));

        for (key, val) in &section.rows {
            if val.is_empty() {
                // Placeholder row such as "(none configured)".
                println!("  {key}");
            } else {
                println!("  {key:<max_key_len$}  {val}");
            }
        }
    }
}

/// Print flat rows with dynamic column alignment.
///
/// Used by the `settings set` confirmation to show only the changed rows
/// without section headers (sectioning one or two rows would be noisy).
pub(super) fn print_display_rows(rows: &[(String, String)]) {
    let max_key_len = rows.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, val) in rows {
        println!("  {key:<max_key_len$}  {val}");
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use gglib_core::Settings;
    use gglib_core::domain::InferenceConfig;

    use super::{camel_to_kebab, settings_display_rows, settings_to_sections};

    // ── camel_to_kebab ────────────────────────────────────────────────────────

    #[test]
    fn camel_to_kebab_converts_correctly() {
        assert_eq!(camel_to_kebab("topK"), "top-k");
        assert_eq!(camel_to_kebab("maxTokens"), "max-tokens");
        assert_eq!(camel_to_kebab("repeatPenalty"), "repeat-penalty");
        assert_eq!(camel_to_kebab("temperature"), "temperature");
        assert_eq!(camel_to_kebab("topP"), "top-p");
    }

    // ── settings_display_rows ─────────────────────────────────────────────────

    #[test]
    fn settings_display_rows_uses_kebab_case_keys() {
        let settings = Settings::default();
        let rows = settings_display_rows(&settings, None);

        assert!(!rows.is_empty(), "should produce at least one row");

        for (key, _) in &rows {
            assert!(
                key.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.'),
                "key {key:?} must contain only [a-z0-9-.] characters"
            );
            assert!(
                !key.contains('_'),
                "key {key:?} must not contain underscores"
            );
        }

        // No duplicate keys.
        let mut seen = std::collections::BTreeSet::new();
        for (key, _) in &rows {
            assert!(seen.insert(key.clone()), "duplicate key {key:?}");
        }
    }

    #[test]
    fn setup_completed_is_hidden() {
        let settings = Settings::default();
        let rows = settings_display_rows(&settings, None);
        assert!(
            rows.iter().all(|(k, _)| k != "setup-completed"),
            "setup-completed must not appear in display rows"
        );
    }

    #[test]
    fn settings_display_rows_model_display_override() {
        let settings = Settings {
            default_model_id: Some(42),
            ..Default::default()
        };
        let rows = settings_display_rows(&settings, Some("42 (TestModel)".to_owned()));
        let model_row = rows
            .iter()
            .find(|(k, _)| k == "default-model-id")
            .expect("default-model-id row should be present");
        assert_eq!(model_row.1, "42 (TestModel)");
    }

    #[test]
    fn settings_display_rows_null_displays_as_none() {
        let settings = Settings {
            default_download_path: None,
            ..Default::default()
        };
        let rows = settings_display_rows(&settings, None);
        let row = rows
            .iter()
            .find(|(k, _)| k == "default-download-path")
            .expect("default-download-path should be present");
        assert_eq!(row.1, "None");
    }

    #[test]
    fn inference_defaults_null_emits_bare_none_row() {
        let settings = Settings {
            inference_defaults: None,
            ..Default::default()
        };
        let rows = settings_display_rows(&settings, None);
        let row = rows
            .iter()
            .find(|(k, _)| k == "inference-defaults")
            .expect("bare inference-defaults row should be present when field is null");
        assert_eq!(row.1, "None");
    }

    #[test]
    fn inference_defaults_expanded_to_sub_rows() {
        let settings = Settings {
            inference_defaults: Some(InferenceConfig {
                temperature: Some(0.75),
                top_k: Some(20),
                ..Default::default()
            }),
            ..Default::default()
        };
        let rows = settings_display_rows(&settings, None);

        // No bare "inference-defaults" row when the field is set.
        assert!(
            rows.iter().all(|(k, _)| k != "inference-defaults"),
            "bare inference-defaults row must not appear when the field is set"
        );

        let temp = rows
            .iter()
            .find(|(k, _)| k == "inference-defaults.temperature")
            .expect("inference-defaults.temperature should be present");
        assert_eq!(temp.1, "0.75");

        let topk = rows
            .iter()
            .find(|(k, _)| k == "inference-defaults.top-k")
            .expect("inference-defaults.top-k should be present");
        assert_eq!(topk.1, "20");

        let maxtok = rows
            .iter()
            .find(|(k, _)| k == "inference-defaults.max-tokens")
            .expect("inference-defaults.max-tokens should be present");
        assert_eq!(maxtok.1, "None");
    }

    // ── settings_to_sections ──────────────────────────────────────────────────

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
}
