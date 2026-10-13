//! Tests for the `agentic-prompt-note` setting on `gglib config settings`:
//! set, shown and unset over a database of the test's own.

use clap::{Args, Command, FromArgMatches};

use super::super::settings_display::settings_display_rows;
use super::super::unset::handle_unset;
use super::*;
use crate::bootstrap::test_context;

fn args(flags: &[&str]) -> SettingsSetArgs {
    let matches = SettingsSetArgs::augment_args(Command::new("set"))
        .try_get_matches_from(std::iter::once("set").chain(flags.iter().copied()))
        .expect("the flags parse");
    SettingsSetArgs::from_arg_matches(&matches).expect("args")
}

/// The row `settings show` prints for the note.
async fn shown(ctx: &CliContext) -> String {
    let settings = ctx.app.settings().get().await.expect("settings");
    let rows = settings_display_rows(&settings, None, None);
    let row = rows.into_iter().find(|(k, _)| k == "agentic-prompt-note");
    row.expect("the row is listed").1
}

/// The flag reaches the update as what was typed, and is left out of it
/// when not typed.
#[test]
fn the_flag_maps_to_the_update_field() {
    assert_eq!(args(&[]).agentic_prompt_note, None);
    assert_eq!(
        args(&["--agentic-prompt-note", "true"]).agentic_prompt_note,
        Some(true)
    );
    assert_eq!(
        args(&["--agentic-prompt-note", "false"]).agentic_prompt_note,
        Some(false)
    );
    assert_eq!(
        args(&["--agentic-sampling", "false"]).agentic_prompt_note,
        None
    );
}

/// The note is off when absent and `show` says so; `true` turns it on, only
/// `true` and `false` parse, and `unset` turns it back off.
#[tokio::test]
async fn the_prompt_note_is_off_when_absent_and_set_by_true_or_false() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    assert_eq!(shown(&ctx).await, "None (off)");

    handle_set(&ctx, args(&["--agentic-prompt-note", "true"]))
        .await
        .expect("on");
    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored.agentic_prompt_note, Some(true));
    assert!(stored.effective_agentic_prompt_note());
    assert_eq!(shown(&ctx).await, "true");

    handle_set(&ctx, args(&["--agentic-prompt-note", "false"]))
        .await
        .expect("off");
    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored.agentic_prompt_note, Some(false));
    assert!(!stored.effective_agentic_prompt_note());
    assert_eq!(shown(&ctx).await, "false");

    assert!(
        SettingsSetArgs::augment_args(Command::new("set"))
            .try_get_matches_from(["set", "--agentic-prompt-note", "maybe"])
            .is_err(),
        "only true or false is a switch position"
    );

    handle_unset(&ctx, "agentic-prompt-note")
        .await
        .expect("unset");
    let stored = ctx.app.settings().get().await.expect("settings");
    assert_eq!(stored.agentic_prompt_note, None);
    assert_eq!(shown(&ctx).await, "None (off)");
}
