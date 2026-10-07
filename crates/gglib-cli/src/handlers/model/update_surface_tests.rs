//! `gglib model update` beside the inspector: the same edit, made from the
//! command line and made with the body the inspector sends, leaves the same
//! row, because both are `ModelOps::update`.

use std::sync::Arc;

use gglib_app_services::ModelOps;
use gglib_app_services::types::UpdateModelRequest;
use gglib_core::domain::DefaultsOrigin;
use gglib_core::events::AppEvent;
use serde_json::{Value, json};

use super::super::test_library::{Heard, Runtime, library, ops, run_with, stored};
use super::*;
use crate::bootstrap::CliContext;

/// The defaults the model holds before each edit: two parameters, written at
/// import and never reviewed by a person.
fn seeded() -> InferenceConfig {
    InferenceConfig {
        temperature: Some(0.2),
        top_k: Some(40),
        ..InferenceConfig::default()
    }
}

/// A library whose one model holds [`seeded`], the row as stored, and the
/// ops its edits are made through.
async fn seeded_library(dir: &tempfile::TempDir) -> (CliContext, Model, Arc<Heard>, ModelOps) {
    let (ctx, mut model) = library(dir.path()).await;
    model.inference_defaults = Some(seeded());
    model.defaults_origin = Some(DefaultsOrigin::AutoDetected);
    ctx.app.models().update(&model).await.expect("seeded");
    let model = stored(&ctx, model.id).await;
    let heard = Arc::new(Heard::default());
    let ops = ops(&ctx, &heard, &Arc::new(Runtime::default()));
    (ctx, model, heard, ops)
}

/// The row whole, so two of them compare field for field.
fn whole(model: &Model) -> Value {
    serde_json::to_value(model).expect("a model serializes")
}

/// The row `flags` leave from the command line, and the row `body` leaves
/// sent as the inspector sends it, each made from the same seeded row.
async fn from_both_surfaces(flags: &[&str], body: Value) -> (Model, Model) {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, seed, _heard, ops) = seeded_library(&dir).await;
    let id = seed.id.to_string();
    let argv = [&["gglib", "model", "update", &id, "--force"], flags].concat();

    run_with(&ctx, &ops, &argv).await.expect("the command");
    let from_cli = stored(&ctx, seed.id).await;

    ctx.app
        .models()
        .update(&seed)
        .await
        .expect("the row is reset");
    let request: UpdateModelRequest = serde_json::from_value(body).expect("the inspector's body");
    ops.update(seed.id, request).await.expect("the request");
    let from_gui = stored(&ctx, seed.id).await;

    (from_cli, from_gui)
}

/// One parameter set: the inspector sends the form whole with that field
/// changed, and the flag changes that field of what the model holds.
#[tokio::test]
async fn setting_one_parameter_stores_the_row_the_inspector_stores() {
    let (cli, gui) = from_both_surfaces(
        &["--temperature", "0.7"],
        json!({"inferenceDefaults": {"temperature": 0.7, "topK": 40}}),
    )
    .await;

    assert_eq!(whole(&cli), whole(&gui));
    assert_eq!(
        cli.inference_defaults,
        Some(InferenceConfig {
            temperature: Some(0.7),
            ..seeded()
        })
    );
    assert_eq!(cli.defaults_origin, Some(DefaultsOrigin::User));
}

#[tokio::test]
async fn clearing_one_parameter_stores_the_row_the_inspector_stores() {
    let (cli, gui) = from_both_surfaces(
        &["--unset", "top-k"],
        json!({"inferenceDefaults": {"temperature": 0.2}}),
    )
    .await;

    assert_eq!(whole(&cli), whole(&gui));
    assert_eq!(
        cli.inference_defaults,
        Some(InferenceConfig {
            top_k: None,
            ..seeded()
        })
    );
    assert_eq!(cli.defaults_origin, Some(DefaultsOrigin::User));
}

/// Cleared from either surface, in one step or a parameter at a time, the
/// model inherits: no config, and so no origin. The inspector's `{}` used to
/// leave an empty user-set row here.
#[tokio::test]
async fn clearing_every_default_stores_the_row_the_inspector_stores() {
    let empty = json!({"inferenceDefaults": {}});
    let (cli, gui) = from_both_surfaces(&["--clear-inference-defaults"], empty.clone()).await;
    let (one_at_a_time, _) =
        from_both_surfaces(&["--unset", "temperature", "--unset", "top-k"], empty).await;

    assert_eq!(whole(&cli), whole(&gui));
    assert_eq!(gui.inference_defaults, None);
    assert_eq!(gui.defaults_origin, None, "no value left to have an origin");
    assert_eq!(one_at_a_time.inference_defaults, None);
    assert_eq!(one_at_a_time.defaults_origin, None);
}

/// A field that is not a sampling default, and one only the command line
/// could set before: the row is the same, and the defaults are left alone.
#[tokio::test]
async fn setting_a_plain_field_stores_the_row_the_inspector_stores() {
    let (cli, gui) = from_both_surfaces(
        &["--name", "Renamed", "--context-length", "8192"],
        json!({"name": "Renamed", "contextLength": 8192}),
    )
    .await;

    assert_eq!(whole(&cli), whole(&gui));
    assert_eq!(cli.name, "Renamed");
    assert_eq!(cli.context_length, Some(8192));
    assert_eq!(cli.inference_defaults, Some(seeded()));
    assert_eq!(cli.defaults_origin, Some(DefaultsOrigin::AutoDetected));
}

/// The command tells whoever is listening what it stored, as the inspector's
/// update does; a dry run stores nothing and says nothing.
#[tokio::test]
async fn the_command_announces_the_row_it_stored() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, seed, heard, ops) = seeded_library(&dir).await;
    let id = seed.id.to_string();
    let rename = ["gglib", "model", "update", &id, "--name", "Renamed"];

    run_with(&ctx, &ops, &[&rename[..], &["--dry-run"]].concat())
        .await
        .expect("the dry run");
    assert!(heard.events().is_empty(), "{:?}", heard.events());
    assert_eq!(stored(&ctx, seed.id).await.name, seed.name);

    run_with(&ctx, &ops, &[&rename[..], &["--force"]].concat())
        .await
        .expect("the command");

    match heard.events().as_slice() {
        [AppEvent::ModelUpdated { model }] => {
            assert_eq!(model.id, seed.id);
            assert_eq!(model.name, "Renamed");
        }
        other => panic!("expected one ModelUpdated, got {other:?}"),
    }
}

/// `--replace-metadata` starts from nothing, and a removal is applied after
/// the sets whichever way the map was started.
#[tokio::test]
async fn metadata_flags_store_the_map_the_model_holds_afterwards() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, seed, _heard, ops) = seeded_library(&dir).await;
    let id = seed.id.to_string();
    let update = ["gglib", "model", "update", &id, "--force"];
    let had = seed.metadata.len();

    run_with(&ctx, &ops, &[&update[..], &["--metadata", "a=1"]].concat())
        .await
        .expect("a key is added");
    let merged = stored(&ctx, seed.id).await.metadata;
    assert_eq!(merged.len(), had + 1, "{merged:?}");
    assert_eq!(merged.get("a").map(String::as_str), Some("1"));

    let replace = [
        "--metadata",
        "b=2",
        "--metadata",
        "c=3",
        "--replace-metadata",
    ];
    let argv = [&update[..], &replace[..], &["--remove-metadata", "c"]].concat();
    run_with(&ctx, &ops, &argv)
        .await
        .expect("the map is replaced");
    let replaced = stored(&ctx, seed.id).await.metadata;
    assert_eq!(
        replaced,
        HashMap::from([("b".to_owned(), "2".to_owned())]),
        "only what this command set, less what it removed"
    );
}

/// A flag that was not passed is not in the request, so `ModelOps::update`
/// leaves that part of the row as it finds it.
#[tokio::test]
async fn flags_that_were_not_passed_ask_for_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (_ctx, seed, _heard, _ops) = seeded_library(&dir).await;
    let args = UpdateArgs {
        name: Some("Renamed".to_owned()),
        ..update_tests::bare_args()
    };

    let request = build_request(&seed, &args).expect("a request");

    assert_eq!(request.name.as_deref(), Some("Renamed"));
    assert!(request.quantization.is_none() && request.file_path.is_none());
    assert!(request.param_count_b.is_none() && request.architecture.is_none());
    assert!(request.context_length.is_none());
    assert!(request.metadata.is_none(), "no metadata flag was passed");
    assert!(request.inference_defaults.is_none(), "nor a sampling one");
    assert!(request.server_defaults.is_none() && request.projector_path.is_none());
}
