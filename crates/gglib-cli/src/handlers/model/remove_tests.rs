//! `gglib model remove` is `ModelOps::remove`, asked the way the inspector
//! asks it: never to stop a server.

use std::sync::Arc;

use gglib_app_services::types::RemoveModelRequest;
use gglib_core::events::AppEvent;

use super::super::test_library::{Heard, Runtime, library, ops, row, run, run_with};

/// From the command line to the library: the row is gone, the file is not,
/// and whoever is listening is told which model left.
#[tokio::test]
async fn a_removed_model_leaves_the_library_and_is_announced() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let heard = Arc::new(Heard::default());
    let ops = ops(&ctx, &heard, &Arc::new(Runtime::default()));
    let id = model.id.to_string();

    run_with(&ctx, &ops, &["gglib", "model", "remove", &id, "--force"])
        .await
        .expect("the command");

    assert!(
        row(&ctx, model.id).await.is_none(),
        "the row is still there"
    );
    assert!(model.file_path.exists(), "the file went with the row");
    match heard.events().as_slice() {
        [AppEvent::ModelRemoved { model_id }] => assert_eq!(*model_id, model.id),
        other => panic!("expected one ModelRemoved, got {other:?}"),
    }
}

/// A model that is being served is refused, in the words the inspector's
/// remove is refused in, and nothing is told it left.
///
/// `--force` skips the prompt and nothing else. It is not the request's
/// `force`, which stops the server and removes the model from under it.
#[tokio::test]
async fn a_model_being_served_is_refused_as_the_inspector_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let heard = Arc::new(Heard::default());
    let runtime = Arc::new(Runtime::serving(model.id, 9001));
    let ops = ops(&ctx, &heard, &runtime);
    let id = model.id.to_string();

    let refused = run_with(&ctx, &ops, &["gglib", "model", "remove", &id, "--force"])
        .await
        .expect_err("the model is being served");
    let inspector = ops
        .remove(model.id, RemoveModelRequest { force: false })
        .await
        .expect_err("the model is being served");

    assert_eq!(refused.to_string(), inspector.to_string());
    assert!(refused.to_string().contains("port 9001"), "{refused}");
    assert!(
        row(&ctx, model.id).await.is_some(),
        "a refused remove removed"
    );
    assert!(!runtime.stopped(), "--force stopped the server");
    assert!(heard.events().is_empty(), "{:?}", heard.events());
}

/// The command as it is dispatched, with the ops it builds for itself.
#[tokio::test]
async fn the_command_removes_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;

    run(&ctx, &["gglib", "model", "remove", &model.name, "--force"])
        .await
        .expect("the command");

    assert!(row(&ctx, model.id).await.is_none());
}
