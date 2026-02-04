//! Update command handler.
//!
//! Handles updating model metadata in the database.

use std::collections::HashMap;
use std::io::{self, Write};

use anyhow::{Result, anyhow};
use gglib_core::{Model, domain::InferenceConfig};

use crate::bootstrap::CliContext;

/// Arguments for the update command.
#[derive(Debug, Clone)]
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
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub max_tokens: Option<u32>,
    pub repeat_penalty: Option<f32>,
    pub clear_inference_defaults: bool,
    pub dry_run: bool,
    pub force: bool,
}

/// Execute the update command.
///
/// Updates model metadata including name, parameters, architecture,
/// quantization, context length, and custom metadata.
///
/// # Arguments
///
/// * `ctx` - The CLI context providing access to AppCore
/// * `args` - The update command arguments
///
/// # Returns
///
/// Returns `Result<()>` indicating the success or failure of the operation.
pub async fn execute(ctx: &CliContext, args: UpdateArgs) -> Result<()> {
    // Get the existing model by ID
    let existing_model = ctx
        .app()
        .models()
        .get_by_id(args.id as i64)
        .await?
        .ok_or_else(|| anyhow!("Model with ID {} not found", args.id))?;

    // Verify the file still exists
    if !existing_model.file_path.exists() && !args.force {
        tracing::warn!(
            file_path = %existing_model.file_path.display(),
            "Model file no longer exists"
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
    ctx.app().models().update(&updated_model).await?;

    println!("✅ Model updated successfully!");
    Ok(())
}

/// Parse metadata updates from command line arguments.
pub fn parse_metadata_updates(metadata_args: &[String]) -> Result<HashMap<String, String>> {
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

/// Parse metadata keys to remove.
pub fn parse_metadata_removals(remove_arg: &Option<String>) -> Result<Vec<String>> {
    match remove_arg {
        Some(keys_str) => Ok(keys_str.split(',').map(|s| s.trim().to_string()).collect()),
        None => Ok(Vec::new()),
    }
}

/// Create updated model with new values.
pub fn create_updated_model(
    existing: &Model,
    args: &UpdateArgs,
    metadata_updates: &HashMap<String, String>,
    metadata_removals: &[String],
) -> Result<Model> {
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

    // Handle inference parameter defaults
    if args.clear_inference_defaults {
        // Clear all inference defaults (revert to inherit mode)
        updated.inference_defaults = None;
    } else {
        // Check if any inference parameters were provided
        let has_inference_updates = args.temperature.is_some()
            || args.top_p.is_some()
            || args.top_k.is_some()
            || args.max_tokens.is_some()
            || args.repeat_penalty.is_some();

        if has_inference_updates {
            // Start with existing inference defaults or create new
            let mut inference_config = updated.inference_defaults.clone().unwrap_or_default();

            // Update only the fields that were provided
            if let Some(temp) = args.temperature {
                inference_config.temperature = Some(temp);
            }
            if let Some(top_p) = args.top_p {
                inference_config.top_p = Some(top_p);
            }
            if let Some(top_k) = args.top_k {
                inference_config.top_k = Some(top_k);
            }
            if let Some(max_tokens) = args.max_tokens {
                inference_config.max_tokens = Some(max_tokens);
            }
            if let Some(repeat_penalty) = args.repeat_penalty {
                inference_config.repeat_penalty = Some(repeat_penalty);
            }

            updated.inference_defaults = Some(inference_config);
        }
    }

    Ok(updated)
}

/// Show a preview of the changes that will be applied.
fn show_changes_preview(existing: &Model, updated: &Model) {
    println!("\n📋 Preview of changes for model ID {}:", existing.id);
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

    // Show inference defaults changes
    show_inference_defaults_changes(&existing.inference_defaults, &updated.inference_defaults);
}

/// Show inference defaults changes.
fn show_inference_defaults_changes(
    old_config: &Option<InferenceConfig>,
    new_config: &Option<InferenceConfig>,
) {
    // Check if there are any changes
    let has_changes = match (old_config, new_config) {
        (None, None) => false,
        (Some(_), None) => true, // Cleared
        (None, Some(_)) => true, // Added
        (Some(old), Some(new)) => {
            old.temperature != new.temperature
                || old.top_p != new.top_p
                || old.top_k != new.top_k
                || old.max_tokens != new.max_tokens
                || old.repeat_penalty != new.repeat_penalty
        }
    };

    if !has_changes {
        return;
    }

    println!("  Inference Defaults:");

    match (old_config, new_config) {
        (Some(_), None) => {
            println!("    ✗ Cleared (will inherit from global/hardcoded)");
        }
        (None, Some(new)) => {
            println!("    + Set model-specific defaults:");
            if let Some(temp) = new.temperature {
                println!("      Temperature: {}", temp);
            }
            if let Some(top_p) = new.top_p {
                println!("      Top-p: {}", top_p);
            }
            if let Some(top_k) = new.top_k {
                println!("      Top-k: {}", top_k);
            }
            if let Some(max_tokens) = new.max_tokens {
                println!("      Max tokens: {}", max_tokens);
            }
            if let Some(repeat_penalty) = new.repeat_penalty {
                println!("      Repeat penalty: {}", repeat_penalty);
            }
        }
        (Some(old), Some(new)) => {
            if old.temperature != new.temperature {
                println!(
                    "    Temperature: {} → {}",
                    format_option_f32(&old.temperature),
                    format_option_f32(&new.temperature)
                );
            }
            if old.top_p != new.top_p {
                println!(
                    "    Top-p: {} → {}",
                    format_option_f32(&old.top_p),
                    format_option_f32(&new.top_p)
                );
            }
            if old.top_k != new.top_k {
                println!(
                    "    Top-k: {} → {}",
                    format_option_i32(&old.top_k),
                    format_option_i32(&new.top_k)
                );
            }
            if old.max_tokens != new.max_tokens {
                println!(
                    "    Max tokens: {} → {}",
                    format_option_u32(&old.max_tokens),
                    format_option_u32(&new.max_tokens)
                );
            }
            if old.repeat_penalty != new.repeat_penalty {
                println!(
                    "    Repeat penalty: {} → {}",
                    format_option_f32(&old.repeat_penalty),
                    format_option_f32(&new.repeat_penalty)
                );
            }
        }
        (None, None) => {}
    }
}

/// Show metadata changes.
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
}

fn format_option(opt: &Option<String>) -> String {
    opt.as_deref().unwrap_or("--").to_string()
}

fn format_option_u64(opt: &Option<u64>) -> String {
    opt.map(|v| v.to_string())
        .unwrap_or_else(|| "--".to_string())
}

fn format_option_f32(opt: &Option<f32>) -> String {
    opt.map(|v| v.to_string())
        .unwrap_or_else(|| "unset".to_string())
}

fn format_option_i32(opt: &Option<i32>) -> String {
    opt.map(|v| v.to_string())
        .unwrap_or_else(|| "unset".to_string())
}

fn format_option_u32(opt: &Option<u32>) -> String {
    opt.map(|v| v.to_string())
        .unwrap_or_else(|| "unset".to_string())
}

/// Show a single field change.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_model() -> Model {
        let mut metadata = HashMap::new();
        metadata.insert("general.name".to_string(), "Test Model".to_string());
        metadata.insert("test.key".to_string(), "test.value".to_string());

        Model {
            id: 1,
            name: "Original Name".to_string(),
            file_path: PathBuf::from("/test/model.gguf"),
            param_count_b: 7.0,
            architecture: Some("llama".to_string()),
            quantization: Some("Q4_0".to_string()),
            context_length: Some(4096),
            metadata,
            added_at: chrono::Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            capabilities: gglib_core::ModelCapabilities::default(),
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
        assert!(updated.metadata.contains_key("new.key"));
        assert!(!updated.metadata.contains_key("test.key")); // Removed
    }
}
