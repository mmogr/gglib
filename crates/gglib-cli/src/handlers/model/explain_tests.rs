//! `gglib model explain` as a person reads it: the sampling table for a
//! model with a profile and without one, the answer to a profile nobody
//! configured, and what the context fit is sized by.
//!
//! The tables are the text the command printed before it read its
//! explanation from `ModelOps::explain_sampling`, character for character.

use std::path::Path;

use gglib_core::services::ImportMode;

use super::*;
use crate::handlers::model::one_shot_model_ops;
use crate::handlers::model::test_library::{library, run, stored, write_gguf};

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

/// What `gglib model explain <model> [--profile <profile>]` prints above the
/// context chain.
async fn printed(ctx: &CliContext, model: &Model, profile: Option<&str>) -> String {
    let explanation = explain(&one_shot_model_ops(ctx), model.id, profile).await;
    explain_display::explanation_text(&model.name, model.id, &explanation.unwrap())
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
    let argv = [&["gglib", "model", "update", &id, "--force"], &edit[..]].concat();
    run(&ctx, &argv).await.expect("the defaults are stored");
    let tuned = stored(&ctx, tuned.id).await;
    (ctx, plain, tuned)
}

#[tokio::test]
async fn a_model_that_stores_nothing_is_explained_from_the_floor() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, plain, _) = two_models(dir.path()).await;

    let rows = "  temperature             0.7     ← default floor
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
  presence_penalty        —       ← default floor (coupled to temperature layer)
  repeat_penalty          —       ← default floor (coupled to temperature layer)
  min_p                   —       ← default floor (coupled to temperature layer)
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
  presence_penalty        —       ← default floor (coupled to temperature layer)
  repeat_penalty          —       ← default floor (coupled to temperature layer)
  min_p                   —       ← default floor (coupled to temperature layer)
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

    let rows = "  temperature             0.7     ← default floor
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
/// in the words `ModelOps::explain_sampling` refuses it with, so the inspector
/// is told what the terminal is.
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
    let refused = ops.explain_sampling(model.id, Some("codign")).await;
    assert!(
        matches!(&refused, Err(GuiError::ValidationFailed(message)) if message == said),
        "{refused:?}"
    );
}

fn model(weights: &std::path::Path, projector: Option<&std::path::Path>) -> Model {
    let mut new = gglib_core::NewModel::new(
        "qwen".to_owned(),
        weights.to_path_buf(),
        7.0,
        chrono::Utc::now(),
    );
    new.projector_path = projector.map(std::path::Path::to_path_buf);
    Model::stored(1, &new)
}

/// The resident figure is labelled for what `resident_bytes` summed.
#[test]
fn the_resident_figure_names_the_projector_when_it_counts_one() {
    let weights = std::path::Path::new("/models/qwen.gguf");
    let projector = std::path::Path::new("/models/mmproj-F16.gguf");

    assert_eq!(resident_label(&model(weights, None)), "  weights");
    assert_eq!(
        resident_label(&model(weights, Some(projector))),
        "  weights + projector"
    );
}

/// The figure under that label: both files, and the same number a launch
/// of the model is sized by.
#[test]
fn the_fit_is_sized_by_the_weights_and_the_projector_as_a_launch_is() {
    let dir = tempfile::tempdir().unwrap();
    let weights = dir.path().join("qwen.Q8_0.gguf");
    let projector = dir.path().join("mmproj-F16.gguf");
    std::fs::write(&weights, vec![0u8; 4096]).unwrap();
    std::fs::write(&projector, vec![0u8; 512]).unwrap();
    let linked = model(&weights, Some(&projector));

    let (_, inputs) = context_fit(&linked);

    assert_eq!(inputs.weights_bytes, Some(4608));
    let launch = gglib_runtime::ports_impl::model_catalog::model_to_launch_spec(linked);
    assert_eq!(inputs.weights_bytes, Some(launch.file_size_bytes));
}

#[test]
fn the_fit_of_an_unlinked_model_is_sized_by_its_weights() {
    let dir = tempfile::tempdir().unwrap();
    let weights = dir.path().join("qwen.Q8_0.gguf");
    std::fs::write(&weights, vec![0u8; 4096]).unwrap();

    let (_, inputs) = context_fit(&model(&weights, None));

    assert_eq!(inputs.weights_bytes, Some(4096));
}
