//! Update command implementation for modifying GGUF model metadata.
//!
//! This module provides functionality to update existing GGUF models in the database,
//! including model metadata, architecture information, and custom key-value pairs.

use crate::models::Gguf;
use crate::services::database;
use anyhow::{Result, anyhow};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::io::{self, Write};

/// Arguments for the update command
pub struct UpdateArgs {
    pub id: u32,
    pub name: Option<String>,
    pub param_count: Option<f64>,
    pub architecture: Option<String>,
    pub quantization: Option<String>,
    pub context_length: Option<u64>,
    pub metadata: Vec<String>,
    pub remove_metadata: Option<String>,
    pub replace_metadata: bool,
    pub dry_run: bool,
    pub force: bool,
}

/// Handle the update command from the CLI
pub async fn handle_update(args: UpdateArgs) -> Result<()> {
    let pool = database::setup_database().await?;
    execute(&pool, args).await
}

/// Execute the update command
///
/// This function performs the core update logic, including model validation,
/// file checking, metadata processing, and database updates.
///
/// # Arguments
///
/// * `pool` - A reference to the SQLite connection pool
/// * `args` - The update command arguments
///
/// # Returns
///
/// Returns `Ok(())` if the update succeeds, or an error if the operation fails.
///
/// # Examples
///
/// ```rust,no_run
/// use gglib::commands::update::{execute, UpdateArgs};
/// use gglib::services::database;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let pool = database::setup_database().await?;
///     
///     let update_args = UpdateArgs {
///         id: 1,
///         name: Some("Updated Model Name".to_string()),
///         param_count: Some(13.0),
///         architecture: Some("mistral".to_string()),
///         quantization: Some("Q8_0".to_string()),
///         context_length: Some(8192),
///         metadata: vec!["version=2.0".to_string(), "tag=production".to_string()],
///         remove_metadata: None,
///         replace_metadata: false,
///         dry_run: false,
///         force: true, // Skip confirmation prompts
///     };
///     
///     execute(&pool, update_args).await?;
///     println!("Model updated successfully!");
///     
///     Ok(())
/// }
/// ```
pub async fn execute(pool: &SqlitePool, args: UpdateArgs) -> Result<()> {
    // First, get the existing model
    let existing_model = database::get_model_by_id(pool, args.id)
        .await?
        .ok_or_else(|| anyhow!("Model with ID {} not found", args.id))?;

    // Verify the file still exists
    if !existing_model.file_path.exists() && !args.force {
        eprintln!(
            "Warning: Model file '{}' no longer exists",
            existing_model.file_path.display()
        );
        if !args.dry_run {
            print!("Continue with metadata update anyway? [y/N]: ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if !input.trim().to_lowercase().starts_with('y') {
                println!("Update cancelled.");
                return Ok(());
            }
        }
    }

    // Parse metadata changes
    let metadata_updates = parse_metadata_updates(&args.metadata)?;
    let metadata_removals = parse_metadata_removals(&args.remove_metadata)?;

    // Create the updated model
    let updated_model = create_updated_model(
        &existing_model,
        &args,
        &metadata_updates,
        &metadata_removals,
    )?;

    // Show preview of changes
    show_changes_preview(&existing_model, &updated_model);

    if args.dry_run {
        println!("\n🔍 Dry run mode - no changes applied");
        return Ok(());
    }

    // Confirm changes unless force flag is used
    if !args.force {
        print!("\nApply these changes? [y/N]: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().to_lowercase().starts_with('y') {
            println!("Update cancelled.");
            return Ok(());
        }
    }

    // Apply the updates
    database::update_model(pool, args.id, &updated_model).await?;

    println!("✅ Model updated successfully!");
    Ok(())
}

/// Parse metadata updates from command line arguments
fn parse_metadata_updates(metadata_args: &[String]) -> Result<HashMap<String, String>> {
    let mut metadata = HashMap::new();

    for arg in metadata_args {
        if let Some((key, value)) = arg.split_once('=') {
            metadata.insert(key.to_string(), value.to_string());
        } else {
            return Err(anyhow!(
                "Invalid metadata format '{}'. Use 'key=value'",
                arg
            ));
        }
    }

    Ok(metadata)
}

/// Parse metadata keys to remove
fn parse_metadata_removals(remove_arg: &Option<String>) -> Result<Vec<String>> {
    match remove_arg {
        Some(keys_str) => Ok(keys_str.split(',').map(|s| s.trim().to_string()).collect()),
        None => Ok(Vec::new()),
    }
}

/// Create updated model with new values
fn create_updated_model(
    existing: &Gguf,
    args: &UpdateArgs,
    metadata_updates: &HashMap<String, String>,
    metadata_removals: &[String],
) -> Result<Gguf> {
    let mut updated = existing.clone();

    // Update basic fields
    if let Some(name) = &args.name {
        updated.name = name.clone();
    }
    if let Some(param_count) = args.param_count {
        updated.param_count_b = param_count;
    }
    if let Some(architecture) = &args.architecture {
        updated.architecture = Some(architecture.clone());
    }
    if let Some(quantization) = &args.quantization {
        updated.quantization = Some(quantization.clone());
    }
    if let Some(context_length) = args.context_length {
        updated.context_length = Some(context_length);
    }

    // Handle metadata updates
    if args.replace_metadata {
        // Replace entire metadata with new values
        updated.metadata = metadata_updates.clone();
    } else {
        // Merge metadata updates
        for (key, value) in metadata_updates {
            updated.metadata.insert(key.clone(), value.clone());
        }
    }

    // Remove specified metadata keys
    for key in metadata_removals {
        updated.metadata.remove(key);
    }

    Ok(updated)
}

/// Show a preview of the changes that will be applied
fn show_changes_preview(existing: &Gguf, updated: &Gguf) {
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
fn format_option(value: &Option<String>) -> String {
    match value {
        Some(v) => v.clone(),
        None => "(none)".to_string(),
    }
}

/// Format optional u64 for display
fn format_option_u64(value: &Option<u64>) -> String {
    match value {
        Some(v) => v.to_string(),
        None => "(none)".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::path::PathBuf;

    fn create_test_model() -> Gguf {
        let mut metadata = HashMap::new();
        metadata.insert("general.name".to_string(), "Test Model".to_string());
        metadata.insert("test.key".to_string(), "test.value".to_string());

        Gguf {
            id: Some(1),
            name: "Original Name".to_string(),
            file_path: PathBuf::from("/test/model.gguf"),
            param_count_b: 7.0,
            architecture: Some("llama".to_string()),
            quantization: Some("Q4_0".to_string()),
            context_length: Some(4096),
            metadata,
            added_at: Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: Vec::new(),
        }
    }

    #[test]
    fn test_parse_metadata_updates() {
        let metadata_args = vec![
            "key1=value1".to_string(),
            "key2=value2".to_string(),
            "complex.key=complex value with spaces".to_string(),
        ];

        let result = parse_metadata_updates(&metadata_args).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result.get("key1"), Some(&"value1".to_string()));
        assert_eq!(result.get("key2"), Some(&"value2".to_string()));
        assert_eq!(
            result.get("complex.key"),
            Some(&"complex value with spaces".to_string())
        );
    }

    #[test]
    fn test_parse_metadata_updates_invalid_format() {
        let metadata_args = vec!["invalid_format".to_string()];
        let result = parse_metadata_updates(&metadata_args);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_metadata_removals() {
        let remove_arg = Some("key1,key2, key3 ".to_string());
        let result = parse_metadata_removals(&remove_arg).unwrap();

        assert_eq!(result.len(), 3);
        assert_eq!(result, vec!["key1", "key2", "key3"]);
    }

    #[test]
    fn test_create_updated_model() {
        let existing = create_test_model();
        let args = UpdateArgs {
            id: 1,
            name: Some("Updated Name".to_string()),
            param_count: Some(13.0),
            architecture: Some("mistral".to_string()),
            quantization: None,
            context_length: Some(8192),
            metadata: vec!["new.key=new.value".to_string()],
            remove_metadata: Some("test.key".to_string()),
            replace_metadata: false,
            dry_run: false,
            force: false,
        };

        let metadata_updates = parse_metadata_updates(&args.metadata).unwrap();
        let metadata_removals = parse_metadata_removals(&args.remove_metadata).unwrap();

        let updated =
            create_updated_model(&existing, &args, &metadata_updates, &metadata_removals).unwrap();

        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.param_count_b, 13.0);
        assert_eq!(updated.architecture, Some("mistral".to_string()));
        assert_eq!(updated.quantization, Some("Q4_0".to_string())); // Unchanged
        assert_eq!(updated.context_length, Some(8192));

        // Metadata should have the original general.name, new key added, and test.key removed
        assert_eq!(updated.metadata.len(), 2);
        assert!(updated.metadata.contains_key("general.name"));
        assert!(updated.metadata.contains_key("new.key"));
        assert!(!updated.metadata.contains_key("test.key"));
    }

    #[test]
    fn test_create_updated_model_replace_metadata() {
        let existing = create_test_model();
        let args = UpdateArgs {
            id: 1,
            name: None,
            param_count: None,
            architecture: None,
            quantization: None,
            context_length: None,
            metadata: vec!["only.key=only.value".to_string()],
            remove_metadata: None,
            replace_metadata: true,
            dry_run: false,
            force: false,
        };

        let metadata_updates = parse_metadata_updates(&args.metadata).unwrap();
        let metadata_removals = parse_metadata_removals(&args.remove_metadata).unwrap();

        let updated =
            create_updated_model(&existing, &args, &metadata_updates, &metadata_removals).unwrap();

        // With replace_metadata=true, should only have the new metadata
        assert_eq!(updated.metadata.len(), 1);
        assert_eq!(
            updated.metadata.get("only.key"),
            Some(&"only.value".to_string())
        );
        assert!(!updated.metadata.contains_key("general.name"));
        assert!(!updated.metadata.contains_key("test.key"));
    }
}
