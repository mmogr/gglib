//! Update-specific diff and preview rendering.

use crate::models::Gguf;
use std::collections::HashMap;

/// Show a preview of the changes that will be applied
pub fn show_changes_preview(existing: &Gguf, updated: &Gguf) {
    println!(
        "\n📋 Preview of changes for model ID {}:",
        existing.id.unwrap_or(0)
    );
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Show field changes
    show_field_change("Name", &existing.name, &updated.name);
    show_field_change(
        "Parameters",
        &format!("{:.1}B", existing.param_count_b),
        &format!("{:.1}B", updated.param_count_b),
    );
    show_field_change(
        "Architecture",
        &format_option(&existing.architecture),
        &format_option(&updated.architecture),
    );
    show_field_change(
        "Quantization",
        &format_option(&existing.quantization),
        &format_option(&updated.quantization),
    );
    show_field_change(
        "Context Length",
        &format_option_u64(&existing.context_length),
        &format_option_u64(&updated.context_length),
    );

    // Show metadata changes
    show_metadata_changes(&existing.metadata, &updated.metadata);
}

/// Show a single field change
fn show_field_change(field_name: &str, old_value: &str, new_value: &str) {
    if old_value != new_value {
        println!(
            "  {:<15} {} → {}",
            format!("{}:", field_name),
            old_value,
            new_value
        );
    }
}

/// Show metadata changes
fn show_metadata_changes(
    old_metadata: &HashMap<String, String>,
    new_metadata: &HashMap<String, String>,
) {
    let mut has_metadata_changes = false;

    // Check for additions and modifications
    for (key, new_value) in new_metadata {
        match old_metadata.get(key) {
            Some(old_value) if old_value != new_value => {
                if !has_metadata_changes {
                    println!("  Metadata changes:");
                    has_metadata_changes = true;
                }
                println!("    {key}: {old_value} → {new_value}");
            }
            None => {
                if !has_metadata_changes {
                    println!("  Metadata changes:");
                    has_metadata_changes = true;
                }
                println!("    {key} → {new_value} (new)");
            }
            _ => {} // No change
        }
    }

    // Check for removals
    for key in old_metadata.keys() {
        if !new_metadata.contains_key(key) {
            if !has_metadata_changes {
                println!("  Metadata changes:");
                has_metadata_changes = true;
            }
            println!("    {key} (removed)");
        }
    }

    if !has_metadata_changes {
        println!("  No metadata changes");
    }
}

/// Format optional string for display
pub fn format_option(value: &Option<String>) -> String {
    match value {
        Some(v) => v.clone(),
        None => "(none)".to_string(),
    }
}

/// Format optional u64 for display
pub fn format_option_u64(value: &Option<u64>) -> String {
    match value {
        Some(v) => v.to_string(),
        None => "(none)".to_string(),
    }
}
