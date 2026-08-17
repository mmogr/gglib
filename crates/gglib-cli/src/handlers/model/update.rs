//! Update command handler.
//!
//! Handles updating model metadata in the database.

use std::collections::HashMap;
use std::io::{self, Write};

use anyhow::{Result, anyhow};
use gglib_core::{
    Model,
    domain::{DefaultsOrigin, InferenceConfig, ReasoningEffort},
};

use crate::bootstrap::CliContext;
use crate::sampling_params::clear_param;

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
    pub reasoning_effort: Option<ReasoningEffort>,
    pub reasoning_budget_tokens: Option<i32>,
    /// Parameters to clear back to falling through, by flag name.
    pub unset: Vec<String>,
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
            || args.frequency_penalty.is_some()
            || args.reasoning_effort.is_some()
            || args.reasoning_budget_tokens.is_some()
            || !args.unset.is_empty();

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
            if let Some(reasoning_effort) = args.reasoning_effort {
                inference_config.reasoning_effort = Some(reasoning_effort);
            }
            if let Some(reasoning_budget_tokens) = args.reasoning_budget_tokens {
                inference_config.reasoning_budget_tokens = Some(reasoning_budget_tokens);
            }

            // Clears run after sets, so `--top-k 40 --unset top-k` ends
            // cleared. The order is the one the flags read in: the last thing
            // said about a parameter is what holds.
            for param in &args.unset {
                clear_param(&mut inference_config, param)?;
            }

            // A deliberate flag from the user, so this is a user-set value
            // from here on — even if it happens to land on the same
            // numbers gglib would have guessed. See `DefaultsOrigin`.
            updated.defaults_origin = Some(DefaultsOrigin::User);

            updated.inference_defaults = Some(inference_config);

            // Unsetting the last parameter must land back at *inherit*, not at
            // an empty row that outranks global settings while saying nothing.
            // `--unset` one at a time therefore reaches the same state
            // `--clear-inference-defaults` reaches in one step.
            if updated.inference_defaults.as_ref() == Some(&InferenceConfig::default()) {
                updated.inference_defaults = None;
                updated.defaults_origin = None;
            }
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
    // Field-by-field equality, not a hand-written disjunction: the previous
    // one listed fifteen fields by name, so a newly modelled parameter changed
    // silently until someone remembered to add a sixteenth line.
    if old_config == new_config {
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
            if let Some(re) = new.reasoning_effort {
                println!("      Reasoning effort: {}", re);
            }
            if let Some(rb) = new.reasoning_budget_tokens {
                println!("      Reasoning budget tokens: {}", rb);
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
            if old.reasoning_effort != new.reasoning_effort {
                println!(
                    "    Reasoning effort: {} → {}",
                    format_unset(old.reasoning_effort),
                    format_unset(new.reasoning_effort)
                );
            }
            if old.reasoning_budget_tokens != new.reasoning_budget_tokens {
                println!(
                    "    Reasoning budget tokens: {} → {}",
                    format_unset(old.reasoning_budget_tokens),
                    format_unset(new.reasoning_budget_tokens)
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

/// A `Copy` option rendered for the preview, `None` as `unset`.
///
/// The `format_option_*` family below takes references and predates this;
/// `ReasoningEffort` is `Copy` and this reads the same on either side.
fn format_unset<T: std::fmt::Display>(opt: Option<T>) -> String {
    opt.map_or_else(|| "unset".to_owned(), |v| v.to_string())
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
#[path = "update_tests.rs"]
mod update_tests;
