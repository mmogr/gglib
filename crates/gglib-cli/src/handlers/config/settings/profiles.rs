//! `gglib config profile` — manage named sampling profiles.
//!
//! Profiles are global rather than per-model: one `coding` profile applies to
//! every model, and a client selects it per request by asking for
//! `<model>:<profile>`. See `gglib_core::domain::inference_profile`.
//!
//! Every mutation reads the current list, edits it, and writes the whole list
//! back through `SettingsUpdate`. That keeps validation in one place — the
//! settings service validates the merged result before saving, so an invalid
//! name or an out-of-range parameter is rejected here by exactly the same
//! rules that reject it over HTTP.

use anyhow::{Result, bail};

use gglib_core::SettingsUpdate;
use gglib_core::domain::{InferenceConfig, InferenceProfile, builtin_templates};

use crate::bootstrap::CliContext;
use crate::config_commands::ProfileCommand;
use crate::sampling_params::clear_param;

/// Dispatch a `config profile` subcommand.
pub(crate) async fn handle_profile(ctx: &CliContext, command: ProfileCommand) -> Result<()> {
    match command {
        ProfileCommand::List => list(ctx).await,
        ProfileCommand::Show { name } => show(ctx, &name).await,
        ProfileCommand::Set {
            name,
            description,
            temperature,
            top_p,
            top_k,
            max_tokens,
            repeat_penalty,
            presence_penalty,
            min_p,
            dry_multiplier,
            dry_base,
            dry_allowed_length,
            dry_penalty_last_n,
            dynatemp_range,
            dynatemp_exponent,
            top_n_sigma,
            frequency_penalty,
            reasoning_effort,
            reasoning_budget_tokens,
            unset,
            list_in_models,
            no_list_in_models,
        } => {
            let edits = Edits {
                description,
                config: InferenceConfig {
                    temperature,
                    top_p,
                    top_k,
                    max_tokens,
                    repeat_penalty,
                    presence_penalty,
                    min_p,
                    dry_multiplier,
                    dry_base,
                    dry_allowed_length,
                    dry_penalty_last_n,
                    dynatemp_range,
                    dynatemp_exponent,
                    top_n_sigma,
                    frequency_penalty,
                    // Profiles are stored and reused across every request that
                    // selects them, so a seed here would pin them all to one
                    // output. There is deliberately no --seed profile flag.
                    seed: None,
                    reasoning_effort,
                    reasoning_budget_tokens,
                },
                unset,
                list_in_models: match (list_in_models, no_list_in_models) {
                    (true, _) => Some(true),
                    (_, true) => Some(false),
                    _ => None,
                },
            };
            set(ctx, &name, edits).await
        }
        ProfileCommand::Rm { name } => remove(ctx, &name).await,
        ProfileCommand::InstallTemplates { force } => install_templates(ctx, force).await,
    }
}

/// The parts of a profile a single `set` invocation may change.
struct Edits {
    description: Option<String>,
    /// Parameters to set. `None` fields mean "not mentioned", not "clear".
    config: InferenceConfig,
    /// Parameter names to clear back to falling through.
    unset: Vec<String>,
    /// `None` when neither listing flag was passed.
    list_in_models: Option<bool>,
}

/// Load the configured profiles, or an empty list when none are set.
async fn load(ctx: &CliContext) -> Result<Vec<InferenceProfile>> {
    Ok(ctx
        .app
        .settings()
        .get()
        .await?
        .inference_profiles
        .unwrap_or_default())
}

/// Persist the whole profile list.
///
/// Validation happens inside the settings service, against the merged result.
async fn save(ctx: &CliContext, profiles: Vec<InferenceProfile>) -> Result<()> {
    ctx.app
        .settings()
        .update(SettingsUpdate {
            inference_profiles: Some(Some(profiles)),
            ..Default::default()
        })
        .await?;
    Ok(())
}

async fn list(ctx: &CliContext) -> Result<()> {
    let profiles = load(ctx).await?;
    if profiles.is_empty() {
        println!("No inference profiles configured.");
        println!("Run `gglib config profile install-templates` to add starter profiles.");
        return Ok(());
    }

    println!("Inference profiles:");
    for profile in &profiles {
        let listed = if profile.list_in_models {
            " [listed]"
        } else {
            ""
        };
        println!("  {}{}", profile.name, listed);
        if let Some(ref description) = profile.description {
            println!("      {description}");
        }
        let params = summarize(&profile.config);
        println!(
            "      {}",
            if params.is_empty() {
                "no parameters set".to_owned()
            } else {
                params
            }
        );
    }
    Ok(())
}

async fn show(ctx: &CliContext, name: &str) -> Result<()> {
    let profiles = load(ctx).await?;
    let Some(profile) = profiles.iter().find(|p| p.name == name) else {
        bail!("{}", not_found_message(name, &profiles));
    };

    println!("Profile: {}", profile.name);
    if let Some(ref description) = profile.description {
        println!("  description      {description}");
    }
    println!("  list-in-models   {}", profile.list_in_models);
    print_opt("  temperature      ", profile.config.temperature);
    print_opt("  top-p            ", profile.config.top_p);
    print_opt("  top-k            ", profile.config.top_k);
    print_opt("  max-tokens       ", profile.config.max_tokens);
    print_opt("  repeat-penalty   ", profile.config.repeat_penalty);
    print_opt("  presence-penalty ", profile.config.presence_penalty);
    print_opt("  min-p            ", profile.config.min_p);
    print_opt("  frequency-penalty", profile.config.frequency_penalty);
    print_opt("  dynatemp-range   ", profile.config.dynatemp_range);
    print_opt("  dynatemp-exponent", profile.config.dynatemp_exponent);
    print_opt("  top-n-sigma      ", profile.config.top_n_sigma);
    print_opt("  dry-multiplier   ", profile.config.dry_multiplier);
    print_opt("  dry-base         ", profile.config.dry_base);
    print_opt("  dry-allowed-len  ", profile.config.dry_allowed_length);
    print_opt("  dry-penalty-last ", profile.config.dry_penalty_last_n);
    print_opt(
        "  reasoning-effort ",
        profile
            .config
            .reasoning_effort
            .map(|level| format!("{level} (applies only where the model's template reads it)")),
    );
    print_opt(
        "  reasoning-budget ",
        profile.config.reasoning_budget_tokens.map(|n| match n {
            -1 => "-1 (defers to the launch default)".to_owned(),
            0 => "0 (thinking off)".to_owned(),
            n => format!("{n} tokens"),
        }),
    );
    println!();
    println!("Select it per request as `<model>:{}`.", profile.name);
    Ok(())
}

async fn set(ctx: &CliContext, name: &str, edits: Edits) -> Result<()> {
    let mut profiles = load(ctx).await?;

    let existing = profiles.iter().position(|p| p.name == name);
    let mut profile = match existing {
        Some(index) => profiles[index].clone(),
        None => InferenceProfile {
            name: name.to_owned(),
            description: None,
            config: InferenceConfig::default(),
            list_in_models: false,
        },
    };

    // Merge: only parameters actually passed are touched.
    merge_set(&mut profile.config, &edits.config);
    for param in &edits.unset {
        clear_param(&mut profile.config, param)?;
    }
    if let Some(description) = edits.description {
        profile.description = Some(description);
    }
    if let Some(listed) = edits.list_in_models {
        profile.list_in_models = listed;
    }

    let verb = if existing.is_some() {
        "Updated"
    } else {
        "Created"
    };
    match existing {
        Some(index) => profiles[index] = profile,
        None => profiles.push(profile),
    }

    save(ctx, profiles).await?;
    println!("✓ {verb} profile '{name}'.");
    if edits.list_in_models == Some(true) {
        println!("  Clients will see `<model>:{name}` in their model list.");
    }
    Ok(())
}

async fn remove(ctx: &CliContext, name: &str) -> Result<()> {
    let mut profiles = load(ctx).await?;
    let Some(index) = profiles.iter().position(|p| p.name == name) else {
        bail!("{}", not_found_message(name, &profiles));
    };

    profiles.remove(index);
    save(ctx, profiles).await?;
    println!("✓ Deleted profile '{name}'.");
    println!("  Requests naming `<model>:{name}` will now fail with 404.");
    Ok(())
}

async fn install_templates(ctx: &CliContext, force: bool) -> Result<()> {
    let mut profiles = load(ctx).await?;
    let mut added = Vec::new();
    let mut skipped = Vec::new();

    for template in builtin_templates() {
        match profiles.iter().position(|p| p.name == template.name) {
            Some(index) if force => {
                added.push(template.name.clone());
                profiles[index] = template;
            }
            Some(_) => skipped.push(template.name),
            None => {
                added.push(template.name.clone());
                profiles.push(template);
            }
        }
    }

    if added.is_empty() {
        println!("All starter profiles are already installed.");
        println!("Pass --force to overwrite them with the defaults.");
        return Ok(());
    }

    save(ctx, profiles).await?;
    println!("✓ Installed: {}", added.join(", "));
    if !skipped.is_empty() {
        println!("  Skipped (already present): {}", skipped.join(", "));
        println!("  Pass --force to overwrite.");
    }
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────────

/// Apply the parameters an invocation actually named, leaving the rest alone.
///
/// The inverse of `InferenceConfig::merge_with`, which fills gaps in `self`
/// from `other`; here `other` wins wherever it has an opinion.
fn merge_set(target: &mut InferenceConfig, edits: &InferenceConfig) {
    if edits.temperature.is_some() {
        target.temperature = edits.temperature;
    }
    if edits.top_p.is_some() {
        target.top_p = edits.top_p;
    }
    if edits.top_k.is_some() {
        target.top_k = edits.top_k;
    }
    if edits.max_tokens.is_some() {
        target.max_tokens = edits.max_tokens;
    }
    if edits.repeat_penalty.is_some() {
        target.repeat_penalty = edits.repeat_penalty;
    }
    if edits.presence_penalty.is_some() {
        target.presence_penalty = edits.presence_penalty;
    }
    if edits.min_p.is_some() {
        target.min_p = edits.min_p;
    }
    if edits.dry_multiplier.is_some() {
        target.dry_multiplier = edits.dry_multiplier;
    }
    if edits.dry_base.is_some() {
        target.dry_base = edits.dry_base;
    }
    if edits.dry_allowed_length.is_some() {
        target.dry_allowed_length = edits.dry_allowed_length;
    }
    if edits.dry_penalty_last_n.is_some() {
        target.dry_penalty_last_n = edits.dry_penalty_last_n;
    }
    if edits.dynatemp_range.is_some() {
        target.dynatemp_range = edits.dynatemp_range;
    }
    if edits.dynatemp_exponent.is_some() {
        target.dynatemp_exponent = edits.dynatemp_exponent;
    }
    if edits.top_n_sigma.is_some() {
        target.top_n_sigma = edits.top_n_sigma;
    }
    if edits.frequency_penalty.is_some() {
        target.frequency_penalty = edits.frequency_penalty;
    }
    if edits.reasoning_effort.is_some() {
        target.reasoning_effort = edits.reasoning_effort;
    }
    if edits.reasoning_budget_tokens.is_some() {
        target.reasoning_budget_tokens = edits.reasoning_budget_tokens;
    }
}

/// One-line summary of the parameters a profile sets.
fn summarize(config: &InferenceConfig) -> String {
    let mut parts = Vec::new();
    if let Some(v) = config.temperature {
        parts.push(format!("temperature={v}"));
    }
    if let Some(v) = config.top_p {
        parts.push(format!("top-p={v}"));
    }
    if let Some(v) = config.top_k {
        parts.push(format!("top-k={v}"));
    }
    if let Some(v) = config.max_tokens {
        parts.push(format!("max-tokens={v}"));
    }
    if let Some(v) = config.repeat_penalty {
        parts.push(format!("repeat-penalty={v}"));
    }
    if let Some(v) = config.presence_penalty {
        parts.push(format!("presence-penalty={v}"));
    }
    if let Some(v) = config.min_p {
        parts.push(format!("min-p={v}"));
    }
    if let Some(v) = config.frequency_penalty {
        parts.push(format!("frequency-penalty={v}"));
    }
    if let Some(v) = config.dynatemp_range {
        parts.push(format!("dynatemp-range={v}"));
    }
    if let Some(v) = config.dynatemp_exponent {
        parts.push(format!("dynatemp-exponent={v}"));
    }
    if let Some(v) = config.top_n_sigma {
        parts.push(format!("top-n-sigma={v}"));
    }
    if let Some(v) = config.dry_multiplier {
        parts.push(format!("dry-multiplier={v}"));
    }
    if let Some(v) = config.dry_base {
        parts.push(format!("dry-base={v}"));
    }
    if let Some(v) = config.dry_allowed_length {
        parts.push(format!("dry-allowed-length={v}"));
    }
    if let Some(v) = config.dry_penalty_last_n {
        parts.push(format!("dry-penalty-last-n={v}"));
    }
    if let Some(v) = config.reasoning_effort {
        parts.push(format!("reasoning-effort={v}"));
    }
    if let Some(v) = config.reasoning_budget_tokens {
        parts.push(format!("reasoning-budget-tokens={v}"));
    }
    parts.join("  ")
}

/// Error text for a name that does not match a configured profile.
pub(crate) fn not_found_message(name: &str, profiles: &[InferenceProfile]) -> String {
    if profiles.is_empty() {
        return format!(
            "no profile named '{name}'; none are configured \
             (run `gglib config profile install-templates`)"
        );
    }
    let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
    format!(
        "no profile named '{name}'; configured profiles are: {}",
        names.join(", ")
    )
}

fn print_opt<T: std::fmt::Display>(label: &str, value: Option<T>) {
    match value {
        Some(v) => println!("{label} {v}"),
        None => println!("{label} (falls through to model default)"),
    }
}

#[cfg(test)]
#[path = "profiles_tests.rs"]
mod profiles_tests;
