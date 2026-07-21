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

/// Dispatch a `config profile` subcommand.
pub async fn handle_profile(ctx: &CliContext, command: ProfileCommand) -> Result<()> {
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
    print_opt("  temperature     ", profile.config.temperature);
    print_opt("  top-p           ", profile.config.top_p);
    print_opt("  top-k           ", profile.config.top_k);
    print_opt("  max-tokens      ", profile.config.max_tokens);
    print_opt("  repeat-penalty  ", profile.config.repeat_penalty);
    print_opt("  presence-penalty", profile.config.presence_penalty);
    print_opt("  min-p           ", profile.config.min_p);
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
}

/// Clear one parameter by its CLI flag name.
fn clear_param(config: &mut InferenceConfig, param: &str) -> Result<()> {
    // Accept either spelling so `--unset top-k` and `--unset top_k` both work.
    match param.replace('_', "-").as_str() {
        "temperature" => config.temperature = None,
        "top-p" => config.top_p = None,
        "top-k" => config.top_k = None,
        "max-tokens" => config.max_tokens = None,
        "repeat-penalty" => config.repeat_penalty = None,
        "presence-penalty" => config.presence_penalty = None,
        "min-p" => config.min_p = None,
        other => bail!(
            "unknown parameter '{other}'; expected one of: temperature, top-p, \
             top-k, max-tokens, repeat-penalty, presence-penalty, min-p"
        ),
    }
    Ok(())
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
    parts.join("  ")
}

/// Error text for a name that does not match a configured profile.
fn not_found_message(name: &str, profiles: &[InferenceProfile]) -> String {
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
mod tests {
    use super::*;

    fn config() -> InferenceConfig {
        InferenceConfig {
            temperature: Some(0.2),
            top_k: Some(40),
            ..Default::default()
        }
    }

    /// A `set` invocation must only touch the parameters it names — that is
    /// what makes editing one field of an existing profile safe.
    #[test]
    fn merge_set_only_touches_named_parameters() {
        let mut target = config();
        merge_set(
            &mut target,
            &InferenceConfig {
                temperature: Some(0.9),
                ..Default::default()
            },
        );

        assert_eq!(target.temperature, Some(0.9), "named parameter is updated");
        assert_eq!(target.top_k, Some(40), "unnamed parameter is preserved");
    }

    #[test]
    fn clear_param_accepts_both_spellings() {
        let mut hyphen = config();
        clear_param(&mut hyphen, "top-k").expect("hyphenated form");
        assert_eq!(hyphen.top_k, None);

        let mut underscore = config();
        clear_param(&mut underscore, "top_k").expect("underscored form");
        assert_eq!(underscore.top_k, None);
    }

    #[test]
    fn clear_param_rejects_an_unknown_name() {
        let err = clear_param(&mut config(), "nonsense").expect_err("should reject");
        assert!(err.to_string().contains("nonsense"), "got: {err}");
        assert!(err.to_string().contains("temperature"), "lists valid names");
    }

    #[test]
    fn summarize_lists_only_what_is_set() {
        let summary = summarize(&config());
        assert!(summary.contains("temperature=0.2"), "got: {summary}");
        assert!(summary.contains("top-k=40"), "got: {summary}");
        assert!(
            !summary.contains("min-p"),
            "unset params omitted: {summary}"
        );
        assert!(summarize(&InferenceConfig::default()).is_empty());
    }

    #[test]
    fn not_found_message_lists_what_exists() {
        let profiles = vec![InferenceProfile {
            name: "coding".to_owned(),
            description: None,
            config: InferenceConfig::default(),
            list_in_models: false,
        }];
        let message = not_found_message("codeing", &profiles);
        assert!(message.contains("codeing"), "names the miss: {message}");
        assert!(
            message.contains("coding"),
            "names the alternative: {message}"
        );

        let empty = not_found_message("coding", &[]);
        assert!(empty.contains("install-templates"), "got: {empty}");
    }
}
