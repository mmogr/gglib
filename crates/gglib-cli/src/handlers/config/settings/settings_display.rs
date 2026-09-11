//! Display and formatting logic for CLI settings output.
//!
//! Extracted from `settings.rs` to keep command-routing code lean.
//! All presentation concerns — row generation, section grouping, and
//! column alignment — live here.

use gglib_core::Settings;

/// Keys that are purely internal bookkeeping and must not appear in
/// user-facing output.
const HIDDEN_KEYS: &[&str] = &["setup-completed"];

/// Keys whose value is shown as set-or-unset rather than printed.
///
/// Matched on the full dotted key, so a leaf inside a nested value can be
/// masked while its siblings print — which is what the stored pairing needs
/// now that its key and its ticket are one record.
///
/// Only the pairing's key, and deliberately not `proxy-api-key`: the two are
/// credentials with opposite recovery stories. The proxy generates its own key
/// and `settings show` is the only place left to read it, which is why
/// `the_proxy_api_key_is_shown_rather_than_masked` defends printing it. This
/// one is *received* from the other machine by `gglib remote join`, and
/// re-pairing replaces it — so printing it costs a credential's confidentiality
/// on a surface people paste into bug reports and buys back nothing.
///
/// The pairing's ticket stays printed: its own doc calls it an address rather
/// than a credential, and it is useless without the key beside it.
const MASKED_KEYS: &[&str] = &["remote-pairing.api-key"];

/// What a masked key renders as when it holds a value.
const MASKED_VALUE: &str = "(set, not shown)";

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
///
/// A leaf named by [`MASKED_KEYS`] is reported as held rather than printed,
/// at whatever depth it sits: masking only the top level would have shown
/// the remote's key in full the moment it moved inside the pairing record.
fn collect_rows(parent_key: &str, val: serde_json::Value, rows: &mut Vec<(String, String)>) {
    match val {
        serde_json::Value::Object(map) => {
            for (child_key, child_val) in map {
                let kebab_child = camel_to_kebab(&child_key);
                let full_key = format!("{parent_key}.{kebab_child}");
                collect_rows(&full_key, child_val, rows);
            }
        }
        leaf => rows.push((parent_key.to_owned(), mask_or_show(parent_key, leaf))),
    }
}

/// A leaf as it should be shown: masked when [`MASKED_KEYS`] names it.
///
/// Null still renders as "None": whether a key is held at all is the thing a
/// person runs this to find out, and it is not the secret.
fn mask_or_show(key: &str, leaf: serde_json::Value) -> String {
    if MASKED_KEYS.contains(&key) && !leaf.is_null() {
        return MASKED_VALUE.to_owned();
    }
    format_leaf(leaf)
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
        } else if kebab_key == "default-context-size" && val.is_null() {
            // Unset is the ordinary — and preferred — state for this one
            // field: it is what lets the daemon size each launch. A bare
            // "None" reads like something is missing, and this is the surface
            // a person runs to find out what is configured. Every write
            // surface says what empty means; the read surface has to as well.
            //
            // "sized per launch" rather than "fitted to this machine",
            // because the fit is not always reachable: `fit_context` needs
            // the device budget, the weight size and the KV geometry, and
            // ADR 0009 records that a host whose device memory cannot be
            // read gets no fit at all and lands on the floor. This row cannot
            // know which side of that a launch will hit, and a read surface
            // that names a rung the reader will never reach is worse than one
            // that names none.
            rows.push((kebab_key, "None (sized per launch)".to_owned()));
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
#[path = "settings_display_tests.rs"]
mod settings_display_tests;

#[cfg(test)]
#[path = "settings_sections_tests.rs"]
mod settings_sections_tests;
