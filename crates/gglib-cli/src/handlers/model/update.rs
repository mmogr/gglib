//! Update command handler.
//!
//! Handles updating model metadata in the database.

use std::collections::HashMap;
use std::io::{self, Write};

use anyhow::{Result, anyhow};
use gglib_core::{
    Model,
    domain::{DefaultsOrigin, InferenceConfig},
};

use crate::bootstrap::CliContext;

/// Arguments for the update command.
#[derive(Debug, Clone)]
pub(crate) struct UpdateArgs {
    pub identifier: String,
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
    pub presence_penalty: Option<f32>,
    pub min_p: Option<f32>,
    pub dry_multiplier: Option<f32>,
    pub dry_base: Option<f32>,
    pub dry_allowed_length: Option<i32>,
    pub dry_penalty_last_n: Option<i32>,
    pub dynatemp_range: Option<f32>,
    pub dynatemp_exponent: Option<f32>,
    pub top_n_sigma: Option<f32>,
    pub frequency_penalty: Option<f32>,
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
pub(crate) async fn execute(ctx: &CliContext, args: UpdateArgs) -> Result<()> {
    // Get the existing model by name or ID
    let existing_model = ctx
        .app
        .models()
        .get(&args.identifier)
        .await?
        .ok_or_else(|| anyhow!("No model found matching: '{}'", args.identifier))?;

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
    ctx.app.models().update(&updated_model).await?;

    println!("✓ Model updated successfully!");
    Ok(())
}

/// Parse metadata updates from command line arguments.
pub(crate) fn parse_metadata_updates(metadata_args: &[String]) -> Result<HashMap<String, String>> {
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
pub(crate) fn parse_metadata_removals(remove_arg: &Option<String>) -> Result<Vec<String>> {
    match remove_arg {
        Some(keys_str) => Ok(keys_str.split(',').map(|s| s.trim().to_string()).collect()),
        None => Ok(Vec::new()),
    }
}

/// Create updated model with new values.
pub(crate) fn create_updated_model(
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
        // Clear all inference defaults (revert to inherit mode). No value
        // left to have an origin either.
        updated.inference_defaults = None;
        updated.defaults_origin = None;
    } else {
        // Check if any inference parameters were provided
        let has_inference_updates = args.temperature.is_some()
            || args.top_p.is_some()
            || args.top_k.is_some()
            || args.max_tokens.is_some()
            || args.repeat_penalty.is_some()
            || args.presence_penalty.is_some()
            || args.min_p.is_some()
            || args.dry_multiplier.is_some()
            || args.dry_base.is_some()
            || args.dry_allowed_length.is_some()
            || args.dry_penalty_last_n.is_some()
            || args.dynatemp_range.is_some()
            || args.dynatemp_exponent.is_some()
            || args.top_n_sigma.is_some()
            || args.frequency_penalty.is_some();

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
            if let Some(presence_penalty) = args.presence_penalty {
                inference_config.presence_penalty = Some(presence_penalty);
            }
            if let Some(min_p) = args.min_p {
                inference_config.min_p = Some(min_p);
            }
            if let Some(dry_multiplier) = args.dry_multiplier {
                inference_config.dry_multiplier = Some(dry_multiplier);
            }
            if let Some(dry_base) = args.dry_base {
                inference_config.dry_base = Some(dry_base);
            }
            if let Some(dry_allowed_length) = args.dry_allowed_length {
                inference_config.dry_allowed_length = Some(dry_allowed_length);
            }
            if let Some(dry_penalty_last_n) = args.dry_penalty_last_n {
                inference_config.dry_penalty_last_n = Some(dry_penalty_last_n);
            }
            if let Some(dynatemp_range) = args.dynatemp_range {
                inference_config.dynatemp_range = Some(dynatemp_range);
            }
            if let Some(dynatemp_exponent) = args.dynatemp_exponent {
                inference_config.dynatemp_exponent = Some(dynatemp_exponent);
            }
            if let Some(top_n_sigma) = args.top_n_sigma {
                inference_config.top_n_sigma = Some(top_n_sigma);
            }
            if let Some(frequency_penalty) = args.frequency_penalty {
                inference_config.frequency_penalty = Some(frequency_penalty);
            }

            // A deliberate flag from the user, so this is a user-set value
            // from here on — even if it happens to land on the same
            // numbers gglib would have guessed. See `DefaultsOrigin`.
            updated.defaults_origin = Some(DefaultsOrigin::User);

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
                || old.presence_penalty != new.presence_penalty
                || old.min_p != new.min_p
                || old.dry_multiplier != new.dry_multiplier
                || old.dry_base != new.dry_base
                || old.dry_allowed_length != new.dry_allowed_length
                || old.dry_penalty_last_n != new.dry_penalty_last_n
                || old.dynatemp_range != new.dynatemp_range
                || old.dynatemp_exponent != new.dynatemp_exponent
                || old.top_n_sigma != new.top_n_sigma
                || old.frequency_penalty != new.frequency_penalty
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
            if let Some(pp) = new.presence_penalty {
                println!("      Presence penalty: {}", pp);
            }
            if let Some(mp) = new.min_p {
                println!("      Min-P: {}", mp);
            }
            if let Some(dm) = new.dry_multiplier {
                println!("      DRY multiplier: {}", dm);
            }
            if let Some(db) = new.dry_base {
                println!("      DRY base: {}", db);
            }
            if let Some(dal) = new.dry_allowed_length {
                println!("      DRY allowed length: {}", dal);
            }
            if let Some(dpn) = new.dry_penalty_last_n {
                println!("      DRY penalty last N: {}", dpn);
            }
            if let Some(dr) = new.dynatemp_range {
                println!("      Dynatemp range: {}", dr);
            }
            if let Some(de) = new.dynatemp_exponent {
                println!("      Dynatemp exponent: {}", de);
            }
            if let Some(ts) = new.top_n_sigma {
                println!("      Top-n-sigma: {}", ts);
            }
            if let Some(fp) = new.frequency_penalty {
                println!("      Frequency penalty: {}", fp);
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
            if old.presence_penalty != new.presence_penalty {
                println!(
                    "    Presence penalty: {} → {}",
                    format_option_f32(&old.presence_penalty),
                    format_option_f32(&new.presence_penalty)
                );
            }
            if old.min_p != new.min_p {
                println!(
                    "    Min-P: {} → {}",
                    format_option_f32(&old.min_p),
                    format_option_f32(&new.min_p)
                );
            }
            if old.dry_multiplier != new.dry_multiplier {
                println!(
                    "    DRY multiplier: {} → {}",
                    format_option_f32(&old.dry_multiplier),
                    format_option_f32(&new.dry_multiplier)
                );
            }
            if old.dry_base != new.dry_base {
                println!(
                    "    DRY base: {} → {}",
                    format_option_f32(&old.dry_base),
                    format_option_f32(&new.dry_base)
                );
            }
            if old.dry_allowed_length != new.dry_allowed_length {
                println!(
                    "    DRY allowed length: {} → {}",
                    format_option_i32(&old.dry_allowed_length),
                    format_option_i32(&new.dry_allowed_length)
                );
            }
            if old.dry_penalty_last_n != new.dry_penalty_last_n {
                println!(
                    "    DRY penalty last N: {} → {}",
                    format_option_i32(&old.dry_penalty_last_n),
                    format_option_i32(&new.dry_penalty_last_n)
                );
            }
            if old.dynatemp_range != new.dynatemp_range {
                println!(
                    "    Dynatemp range: {} → {}",
                    format_option_f32(&old.dynatemp_range),
                    format_option_f32(&new.dynatemp_range)
                );
            }
            if old.dynatemp_exponent != new.dynatemp_exponent {
                println!(
                    "    Dynatemp exponent: {} → {}",
                    format_option_f32(&old.dynatemp_exponent),
                    format_option_f32(&new.dynatemp_exponent)
                );
            }
            if old.top_n_sigma != new.top_n_sigma {
                println!(
                    "    Top-n-sigma: {} → {}",
                    format_option_f32(&old.top_n_sigma),
                    format_option_f32(&new.top_n_sigma)
                );
            }
            if old.frequency_penalty != new.frequency_penalty {
                println!(
                    "    Frequency penalty: {} → {}",
                    format_option_f32(&old.frequency_penalty),
                    format_option_f32(&new.frequency_penalty)
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
            dialect_spec: None,
            id: 1,
            name: "Original Name".to_string(),
            model_key: String::new(),
            file_path: PathBuf::from("/test/model.gguf"),
            param_count_b: 7.0,
            inference_defaults: None,
            defaults_origin: None,
            architecture: Some("llama".to_string()),
            quantization: Some("Q4_0".to_string()),
            context_length: Some(4096),
            expert_count: None,
            expert_used_count: None,
            expert_shared_count: None,
            metadata,
            added_at: chrono::Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            capabilities: gglib_core::ModelCapabilities::default(),
            last_update_check: None,
            tags: Vec::new(),
            server_defaults: None,
            template_caps: None,
            benchmark_summary: None,
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
            identifier: "1".to_string(),
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
            temperature: None,
            top_p: None,
            top_k: None,
            max_tokens: None,
            repeat_penalty: None,
            presence_penalty: None,
            min_p: None,
            dry_multiplier: None,
            dry_base: None,
            dry_allowed_length: None,
            dry_penalty_last_n: None,
            dynatemp_range: None,
            dynatemp_exponent: None,
            top_n_sigma: None,
            frequency_penalty: None,
            clear_inference_defaults: false,
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
