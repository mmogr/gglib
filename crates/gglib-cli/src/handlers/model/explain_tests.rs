//! `gglib model explain` as a person reads it: the launch line and the
//! context section from the runtime's plan, the sampling table for a model
//! with a profile and without one, and the answer to a profile nobody
//! configured.
//!
//! The tables are the text the command printed before it read its
//! explanation from `ModelOps::explain`, character for character. What a
//! running daemon says is in `explain_daemon_tests.rs`.

use std::path::Path;

use gglib_core::domain::Model;
use gglib_core::services::ImportMode;

use super::*;
use crate::handlers::model::one_shot_model_ops;
use crate::handlers::model::test_library::{library, run, stored, write_gguf};
use crate::presentation::launch_display;

/// The two lines under every table, with client sampling left untrusted.
const CAVEATS: &str = "  Operator flags (gglib proxy --temperature, ...) outrank every layer above.
  Client sampling is ignored, except max_tokens, reasoning_budget_tokens.
";

/// The table for `model`: `heading` after its name and id, and `rows`.
fn table(model: &Model, heading: &str, rows: &str) -> String {
    let rule = "-".repeat(85);
    format!(
        "\n  Sampling for {} (id {}){heading}\n{rule}\n{rows}{rule}\n{CAVEATS}\n",
        model.name, model.id
    )
}

/// The sampling table `gglib model explain <model> [--profile <profile>]`
/// prints after the launch.
async fn printed(ctx: &CliContext, model: &Model, profile: Option<&str>) -> String {
    let explanation = explain(&one_shot_model_ops(ctx), model.id, profile).await;
    explain_display::explanation_text(&model.name, model.id, &explanation.unwrap().sampling)
}

/// A library of two models: the first stores nothing, and the second stores
/// sampling defaults of its own over a GGUF that publishes three.
async fn two_models(dir: &Path) -> (CliContext, Model, Model) {
    let (ctx, plain) = library(dir).await;
    let published = [
        ("general.architecture", "llama"),
        ("general.sampling.temp", "0.33"),
        ("general.sampling.top_k", "17"),
        ("general.sampling.top_p", "warm"),
    ];
    let weights = write_gguf(dir, "bravo.Q8_0.gguf", &published);
    let tuned = (ctx.app.models())
        .import_from_file(&weights, ctx.gguf_parser.as_ref(), None, ImportMode::Fresh)
        .await
        .expect("the model imports");
    let id = tuned.id.to_string();
    let edit = [
        "--temperature",
        "0.6",
        "--top-k",
        "30",
        "--frequency-penalty",
        "0.25",
    ];
    let argv = [&["gglib", "model", "update", &id, "--yes"], &edit[..]].concat();
    run(&ctx, &argv).await.expect("the defaults are stored");
    let tuned = stored(&ctx, tuned.id).await;
    (ctx, plain, tuned)
}

#[tokio::test]
async fn a_model_that_stores_nothing_is_explained_as_unset() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, plain, _) = two_models(dir.path()).await;

    let rows = "  temperature             —       ← unset by design
  top_p                   —       ← unset by design
  top_k                   —       ← unset by design
  presence_penalty        —       ← unset by design
  repeat_penalty          —       ← unset by design
  min_p                   —       ← unset by design
  frequency_penalty       —       ← unset by design
  dynatemp_range          —       ← unset by design
  dynatemp_exponent       —       ← unset by design
  top_n_sigma             —       ← unset by design
  dry_multiplier          —       ← unset by design
  dry_base                —       ← unset by design
  dry_allowed_length      —       ← unset by design
  dry_penalty_last_n      —       ← unset by design
  max_tokens              —       ← unset by design
  reasoning_effort        —       ← unset by design
  reasoning_budget_tokens —       ← unset by design
";
    assert_eq!(printed(&ctx, &plain, None).await, table(&plain, "", rows));
}

/// A model's own defaults, each beside what its GGUF publishes for the field.
#[tokio::test]
async fn a_models_own_defaults_are_set_against_what_its_gguf_publishes() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, _, tuned) = two_models(dir.path()).await;

    let rows = "  temperature             0.6     ← per-model defaults (user-set)
        ! general.sampling.temp = 0.33; gglib is sending 0.6
  top_p                   —       ← unset by design
        ? general.sampling.top_p is set to a value gglib cannot read
  top_k                   30      ← per-model defaults (user-set)
        ! general.sampling.top_k = 17; gglib is sending 30
  presence_penalty        —       ← unset (coupled to temperature layer)
  repeat_penalty          —       ← unset (coupled to temperature layer)
  min_p                   —       ← unset (coupled to temperature layer)
  frequency_penalty       0.25    ← per-model defaults (user-set)
  dynatemp_range          —       ← unset by design
  dynatemp_exponent       —       ← unset by design
  top_n_sigma             —       ← unset by design
  dry_multiplier          —       ← unset by design
  dry_base                —       ← unset by design
  dry_allowed_length      —       ← unset by design
  dry_penalty_last_n      —       ← unset by design
  max_tokens              —       ← unset by design
  reasoning_effort        —       ← unset by design
  reasoning_budget_tokens —       ← unset by design
";
    assert_eq!(printed(&ctx, &tuned, None).await, table(&tuned, "", rows));
}

/// A profile is the rung above the model's own defaults, and is named in the
/// heading and on each row it supplied.
#[tokio::test]
async fn a_profile_is_applied_over_the_models_own_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, plain, tuned) = two_models(dir.path()).await;
    let settings = ctx.app.settings();
    settings.install_profile_templates(false).await.unwrap();

    let rows = "  temperature             1.1     ← profile 'creative'
        ! general.sampling.temp = 0.33; gglib is sending 1.1
  top_p                   0.98    ← profile 'creative'
        ? general.sampling.top_p is set to a value gglib cannot read
  top_k                   30      ← per-model defaults (user-set)
        ! general.sampling.top_k = 17; gglib is sending 30
  presence_penalty        —       ← unset (coupled to temperature layer)
  repeat_penalty          —       ← unset (coupled to temperature layer)
  min_p                   —       ← unset (coupled to temperature layer)
  frequency_penalty       0.25    ← per-model defaults (user-set)
  dynatemp_range          —       ← unset by design
  dynatemp_exponent       —       ← unset by design
  top_n_sigma             —       ← unset by design
  dry_multiplier          —       ← unset by design
  dry_base                —       ← unset by design
  dry_allowed_length      —       ← unset by design
  dry_penalty_last_n      —       ← unset by design
  max_tokens              —       ← unset by design
  reasoning_effort        —       ← unset by design
  reasoning_budget_tokens —       ← unset by design
";
    assert_eq!(
        printed(&ctx, &tuned, Some("creative")).await,
        table(&tuned, ", profile 'creative'", rows)
    );

    let rows = "  temperature             —       ← unset by design
  top_p                   —       ← unset by design
  top_k                   —       ← unset by design
  presence_penalty        —       ← unset by design
  repeat_penalty          —       ← unset by design
  min_p                   —       ← unset by design
  frequency_penalty       —       ← unset by design
  dynatemp_range          —       ← unset by design
  dynatemp_exponent       —       ← unset by design
  top_n_sigma             —       ← unset by design
  dry_multiplier          —       ← unset by design
  dry_base                —       ← unset by design
  dry_allowed_length      —       ← unset by design
  dry_penalty_last_n      —       ← unset by design
  max_tokens              —       ← unset by design
  reasoning_effort        high    ← profile 'high'
  reasoning_budget_tokens 16384   ← profile 'high'
";
    assert_eq!(
        printed(&ctx, &plain, Some("high")).await,
        table(&plain, ", profile 'high'", rows)
    );
}

/// A name that is no configured profile is answered with the ones there are,
/// in the words `ModelOps::explain` refuses it with, so the inspector is
/// told what the terminal is.
#[tokio::test]
async fn an_unknown_profile_is_answered_with_the_configured_ones_on_both_surfaces() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let id = model.id.to_string();
    let argv = ["gglib", "model", "explain", &id, "--profile", "codign"];

    let none = run(&ctx, &argv)
        .await
        .expect_err("no profile is configured");
    assert_eq!(
        none.to_string(),
        "no profile named 'codign'; none are configured \
         (run `gglib config profile install-templates`)"
    );

    let settings = ctx.app.settings();
    settings.install_profile_templates(false).await.unwrap();
    let said = "no profile named 'codign'; configured profiles are: \
                coding, chat, creative, minimal, low, medium, high, xhigh, max";

    let typo = run(&ctx, &argv).await.expect_err("the name is a typo");
    assert_eq!(typo.to_string(), said);
    let ops = one_shot_model_ops(&ctx);
    let refused = ops.explain(model.id, Some("codign")).await;
    assert!(
        matches!(&refused, Err(GuiError::ValidationFailed(message)) if message == said),
        "{refused:?}"
    );
}

/// The launch half comes from the runtime's plan of a bare start, through
/// the one-shot runtime: the command names the model's own file and takes
/// its port when it loads, every argument has a reason, and the context
/// section follows.
#[tokio::test]
async fn the_launch_is_the_runtimes_plan_through_the_one_shot_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;

    let explanation = explain(&one_shot_model_ops(&ctx), model.id, None)
        .await
        .expect("explained");
    let text = launch_display::launch_text(&model.name, model.id, &explanation.plan);

    let line = format!(
        "llama-server -m {} --host 127.0.0.1 --port auto --metrics --parallel 1",
        model.file_path.display()
    );
    assert!(text.contains(&line), "{text}");
    assert!(
        explanation
            .plan
            .args
            .iter()
            .all(|a| a.decision != "unexplained"),
        "{:?}",
        explanation.plan.args
    );
    // The fit sentence the runtime served, over the figures: this fixture's
    // header says neither what its context costs nor its trained window.
    // Whether this machine's memory is read depends on where the test runs,
    // so that reason may come first.
    let sentence = text
        .split("\nContext\n  ")
        .nth(1)
        .and_then(|rest| rest.split_once("\n  serves at"))
        .map(|(sentence, _)| sentence)
        .unwrap_or_else(|| panic!("{text}"));
    assert!(
        sentence.starts_with("Serves at 4K context; gglib could not fit one to this machine: "),
        "{text}"
    );
    assert!(
        sentence
            .ends_with("the model does not say what its context costs or what it was trained for."),
        "{text}"
    );
}
