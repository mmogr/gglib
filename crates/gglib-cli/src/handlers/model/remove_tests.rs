//! `gglib model remove` is `ModelOps::remove`, asked the way the inspector
//! asks it: never to stop a server.
//!
//! What is being served is a runtime the test holds. The view the command
//! builds for itself, over the pid files under a data root, is tested
//! against the built binary in `tests/model_remove_served.rs`.

use std::sync::Arc;

use gglib_core::events::AppEvent;

use super::super::test_library::{Heard, Runtime, library, ops, row, run, run_with};

/// How to stop a server, as a refusal says it.
const HOW_TO_STOP: &str = "Stop it first, in the gglib app or with `gglib daemon stop` (which stops \
                           the daemon and every model it is serving), then remove it.";

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

/// A model that is being served is refused in a terminal's words: which
/// model, on which port, and what stops it.
///
/// `--force` skips the prompt and nothing else. It is not the request's
/// `force`, which stops the server and removes the model from under it, and
/// which the inspector's refusal offers: no flag of this command is that one.
#[tokio::test]
async fn a_model_being_served_is_refused_and_told_how_to_stop_it() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let heard = Arc::new(Heard::default());
    let runtime = Arc::new(Runtime::serving(model.id, 9001));
    let ops = ops(&ctx, &heard, &runtime);
    let id = model.id.to_string();

    let refused = run_with(&ctx, &ops, &["gglib", "model", "remove", &id, "--force"])
        .await
        .expect_err("the model is being served");

    assert_eq!(
        refused.to_string(),
        format!(
            "Model '{}' (ID {}) is being served by a llama-server on port 9001, so it was not \
             removed.\n{HOW_TO_STOP}",
            model.name, model.id
        )
    );
    assert!(
        row(&ctx, model.id).await.is_some(),
        "a refused remove removed"
    );
    assert!(model.file_path.exists(), "a refused remove took the file");
    assert!(!runtime.stopped(), "--force stopped the server");
    assert!(heard.events().is_empty(), "{:?}", heard.events());
}

/// A server that came up after the command looked, as one can while the
/// prompt waits, meets `ModelOps`' own refusal. The terminal is told the
/// same thing as before, less the port, and never the inspector's
/// `force=true`.
#[tokio::test]
async fn a_server_that_came_up_while_the_prompt_waited_is_refused_in_the_same_words() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let heard = Arc::new(Heard::default());
    let runtime = Arc::new(Runtime::serving_after(1, model.id, 9001));
    let ops = ops(&ctx, &heard, &runtime);
    let id = model.id.to_string();

    let refused = run_with(&ctx, &ops, &["gglib", "model", "remove", &id, "--force"])
        .await
        .expect_err("the model is being served by now");

    assert_eq!(
        refused.to_string(),
        format!(
            "Model '{}' (ID {}) is being served, so it was not removed.\n{HOW_TO_STOP}",
            model.name, model.id
        )
    );
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
