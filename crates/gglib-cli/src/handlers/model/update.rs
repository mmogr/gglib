//! Update command handler.
//!
//! Turns the flags into the request the inspector sends too, shows what it
//! would change, and hands it to `ModelOps::update`, which writes the row.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use gglib_app_services::ModelOps;
use gglib_app_services::types::UpdateModelRequest;
use gglib_core::{
    Model,
    domain::{InferenceConfig, ReasoningEffort},
};

use super::{resolver, update_components, update_projector};
use crate::bootstrap::CliContext;
use crate::component_args::request_components;
use crate::sampling_params::clear_param;
use crate::utils::input;

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
    pub projector: crate::projector_args::ProjectorArgs,
    pub components: crate::component_args::ComponentArgs,
}

/// Execute the update command.
///
/// Updates model metadata including name, parameters, architecture,
/// quantization, context length, and custom metadata.
///
/// # Arguments
///
/// * `ctx` - The CLI context providing access to `AppCore`
/// * `ops` - The model operations the update is written through
/// * `args` - The update command arguments
///
/// # Returns
///
/// Returns `Result<()>` indicating the success or failure of the operation.
pub(crate) async fn execute(ctx: &CliContext, ops: &ModelOps, args: UpdateArgs) -> Result<()> {
    // Get the existing model by name or ID
    let existing_model = resolver::resolve_model_identifier(ctx, &args.identifier).await?;

    // Verify the file still exists
    if !existing_model.file_path.exists() && !args.force {
        tracing::warn!(
            file_path = %existing_model.file_path.display(),
            "Model file no longer exists"
        );
        if !args.dry_run && !input::prompt_confirmation("Continue with metadata update anyway?")? {
            println!("Update cancelled.");
            return Ok(());
        }
    }

    // The preview is the row the request leaves, made by the merge
    // `ModelOps::update` writes with.
    let request = build_request(&existing_model, &args)?;
    let mut updated_model = existing_model.clone();
    request.apply_to(&mut updated_model);

    // Show preview of changes
    show_changes_preview(&existing_model, &updated_model);
    if let Some(line) = update_projector::preview(&existing_model, args.projector.change()) {
        println!("{line}");
    }
    for line in update_components::preview(&existing_model, &args.components.changes()?) {
        println!("{line}");
    }

    if args.dry_run {
        println!("\n🔍 Dry run mode - no changes applied");
        return Ok(());
    }

    // Confirm changes unless force flag is used
    if !args.force {
        println!();
        if !input::prompt_confirmation("Apply these changes?")? {
            println!("Update cancelled.");
            return Ok(());
        }
    }

    // Apply the updates
    ops.update(existing_model.id, request).await?;

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
            return Err(anyhow!("Invalid metadata format '{arg}'. Use 'key=value'"));
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

/// The request `args` make of `existing`: each flag as the field it sets.
///
/// A request carries a model's metadata and its sampling defaults whole, so
/// the flags that change part of either are applied here to what the model
/// holds now.
pub(crate) fn build_request(existing: &Model, args: &UpdateArgs) -> Result<UpdateModelRequest> {
    Ok(UpdateModelRequest {
        name: args.name.clone(),
        quantization: args.quantization.clone(),
        param_count_b: args.param_count,
        architecture: args.architecture.clone(),
        context_length: args.context_length,
        metadata: metadata_after(existing, args)?,
        inference_defaults: inference_defaults_after(existing, args)?,
        // The path to link, an explicit nothing to unlink, and no field at
        // all when neither flag was passed.
        projector_path: args.projector.change().map(|change| {
            change
                .path()
                .map(|path| path.to_string_lossy().into_owned())
        }),
        components: request_components(&args.components.changes()?),
        ..UpdateModelRequest::default()
    })
}

/// The metadata the model holds once the flags are applied, or `None` when no
/// flag touches it.
fn metadata_after(existing: &Model, args: &UpdateArgs) -> Result<Option<HashMap<String, String>>> {
    let updates = parse_metadata_updates(&args.metadata)?;
    let removals = parse_metadata_removals(&args.remove_metadata)?;
    if updates.is_empty() && removals.is_empty() && !args.replace_metadata {
        return Ok(None);
    }

    let mut metadata = if args.replace_metadata {
        HashMap::new()
    } else {
        existing.metadata.clone()
    };
    metadata.extend(updates);
    for key in &removals {
        metadata.remove(key);
    }
    Ok(Some(metadata))
}

/// The sampling defaults the model holds once the flags are applied, or
/// `None` when no flag touches them.
///
/// Cleared defaults are the empty config, here and when `--unset` takes the
/// last parameter: `ModelOps::update` stores that as a model that inherits,
/// so one `--unset` at a time reaches the state `--clear-inference-defaults`
/// reaches in one step.
fn inference_defaults_after(
    existing: &Model,
    args: &UpdateArgs,
) -> Result<Option<InferenceConfig>> {
    if args.clear_inference_defaults {
        return Ok(Some(InferenceConfig::default()));
    }

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

    if !has_inference_updates {
        return Ok(None);
    }

    // Start with existing inference defaults or create new
    let mut inference_config = existing.inference_defaults.clone().unwrap_or_default();

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

    // Clears run after sets, so `--top-k 40 --unset top-k` ends cleared. The
    // order is the one the flags read in: the last thing said about a
    // parameter is what holds.
    for param in &args.unset {
        clear_param(&mut inference_config, param)?;
    }

    Ok(Some(inference_config))
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
                println!("      Temperature: {temp}");
            }
            if let Some(top_p) = new.top_p {
                println!("      Top-p: {top_p}");
            }
            if let Some(top_k) = new.top_k {
                println!("      Top-k: {top_k}");
            }
            if let Some(max_tokens) = new.max_tokens {
                println!("      Max tokens: {max_tokens}");
            }
            if let Some(repeat_penalty) = new.repeat_penalty {
                println!("      Repeat penalty: {repeat_penalty}");
            }
            if let Some(pp) = new.presence_penalty {
                println!("      Presence penalty: {pp}");
            }
            if let Some(mp) = new.min_p {
                println!("      Min-P: {mp}");
            }
            if let Some(dm) = new.dry_multiplier {
                println!("      DRY multiplier: {dm}");
            }
            if let Some(db) = new.dry_base {
                println!("      DRY base: {db}");
            }
            if let Some(dal) = new.dry_allowed_length {
                println!("      DRY allowed length: {dal}");
            }
            if let Some(dpn) = new.dry_penalty_last_n {
                println!("      DRY penalty last N: {dpn}");
            }
            if let Some(dr) = new.dynatemp_range {
                println!("      Dynatemp range: {dr}");
            }
            if let Some(de) = new.dynatemp_exponent {
                println!("      Dynatemp exponent: {de}");
            }
            if let Some(ts) = new.top_n_sigma {
                println!("      Top-n-sigma: {ts}");
            }
            if let Some(fp) = new.frequency_penalty {
                println!("      Frequency penalty: {fp}");
            }
            if let Some(re) = new.reasoning_effort {
                println!("      Reasoning effort: {re}");
            }
            if let Some(rb) = new.reasoning_budget_tokens {
                println!("      Reasoning budget tokens: {rb}");
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
    opt.map_or_else(|| "--".to_string(), |v| v.to_string())
}

/// A `Copy` option rendered for the preview, `None` as `unset`.
///
/// The `format_option_*` family below takes references and predates this;
/// `ReasoningEffort` is `Copy` and this reads the same on either side.
fn format_unset<T: std::fmt::Display>(opt: Option<T>) -> String {
    opt.map_or_else(|| "unset".to_owned(), |v| v.to_string())
}

fn format_option_f32(opt: &Option<f32>) -> String {
    opt.map_or_else(|| "unset".to_string(), |v| v.to_string())
}

fn format_option_i32(opt: &Option<i32>) -> String {
    opt.map_or_else(|| "unset".to_string(), |v| v.to_string())
}

fn format_option_u32(opt: &Option<u32>) -> String {
    opt.map_or_else(|| "unset".to_string(), |v| v.to_string())
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

#[cfg(test)]
#[path = "update_surface_tests.rs"]
mod update_surface_tests;
