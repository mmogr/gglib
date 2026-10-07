//! Tests for [`super`] — the profile merge, the summary line, the install
//! report, and `install-templates` run over a database of the test's own.

use gglib_core::domain::builtin_templates;

use super::*;
use crate::bootstrap::test_context;

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

/// `gglib config profile list` is the only place a stored reasoning
/// control is visible at all — neither field is echoed by llama-server, so
/// a profile omitted from this line is a setting with no surface.
#[test]
fn summarize_names_both_reasoning_controls() {
    let summary = summarize(&InferenceConfig {
        reasoning_effort: Some(gglib_core::domain::ReasoningEffort::XHigh),
        reasoning_budget_tokens: Some(-1),
        ..Default::default()
    });

    assert!(summary.contains("reasoning-effort=xhigh"), "got: {summary}");
    assert!(
        summary.contains("reasoning-budget-tokens=-1"),
        "got: {summary}"
    );
}

/// A profile that set only an effort would merge into an all-`None`
/// config on any surface that forgot the field, which reads identically
/// to "no parameters set". This pins that `set` carries both.
#[test]
fn merge_set_carries_both_reasoning_controls() {
    let mut target = InferenceConfig::default();
    merge_set(
        &mut target,
        &InferenceConfig {
            reasoning_effort: Some(gglib_core::domain::ReasoningEffort::Low),
            reasoning_budget_tokens: Some(0),
            ..Default::default()
        },
    );

    assert_eq!(
        target.reasoning_effort,
        Some(gglib_core::domain::ReasoningEffort::Low)
    );
    assert_eq!(target.reasoning_budget_tokens, Some(0));
}

/// What `install-templates` prints for each thing the install can do: all
/// added, some kept, and nothing left to add.
#[test]
fn the_install_report_names_what_was_stored_and_what_was_kept() {
    let names = |list: &[&str]| list.iter().map(|&name| name.to_owned()).collect();
    let done = |installed: &[&str], kept: &[&str]| TemplateInstall {
        installed: names(installed),
        kept: names(kept),
    };

    assert_eq!(
        installed_report(&done(&["coding", "chat"], &[])),
        "✓ Installed: coding, chat\n"
    );
    assert_eq!(
        installed_report(&done(&["coding"], &["chat", "max"])),
        "✓ Installed: coding\n  Skipped (already present): chat, max\n  \
         Pass --force to overwrite.\n"
    );
    assert_eq!(
        installed_report(&done(&[], &["coding", "chat"])),
        "All starter profiles are already installed.\n\
         Pass --force to overwrite them with the defaults.\n"
    );
}

/// The command is the settings service's install and hands it `--force` as
/// given: every starter profile is stored, one already stored under a
/// template's name is kept, and only `--force` puts the template in its place.
/// The settings page's button is that install without force.
#[tokio::test]
async fn the_command_stores_every_starter_profile_and_keeps_a_stored_one_unless_forced() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let templates = builtin_templates();
    let chat = |list: &[InferenceProfile]| list.iter().find(|p| p.name == "chat").cloned();
    let rest = |list: &[InferenceProfile]| -> Vec<InferenceProfile> {
        list.iter().filter(|p| p.name != "chat").cloned().collect()
    };
    let mut mine = chat(&templates).expect("chat is a starter profile");
    mine.config.temperature = Some(0.123);
    save(&ctx, vec![mine.clone()]).await.expect("seeded");
    let install = |force| handle_profile(&ctx, ProfileCommand::InstallTemplates { force });

    install(false).await.expect("installs");

    let stored = load(&ctx).await.expect("profiles");
    assert_eq!(stored.len(), 9);
    assert_eq!(rest(&stored), rest(&templates));
    assert_eq!(chat(&stored), Some(mine), "replaced without --force");

    install(true).await.expect("installs");

    let stored = load(&ctx).await.expect("profiles");
    assert_eq!(stored.len(), 9);
    assert_eq!(chat(&stored), chat(&templates), "kept under --force");
}
