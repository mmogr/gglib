//! A `gglib model …` command run while a daemon serves the same library:
//! what that daemon is sent of a change the command makes through
//! `ModelOps`, and what the command answers when no daemon takes it.

use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter as _;

use super::test_library::{library, row, run, stored};
use crate::daemon_client::STAND_IN_PORT;
use crate::daemon_client::library_changes::tests::{
    TOKEN, another_program, beside, daemon, nobody, told,
};

/// An edit, a capability flag and a removal each reach the daemon as the
/// event `ModelOps` emits for it on every surface: the stored row for a
/// change, and the id for a removal. `capabilities` builds its `ModelOps`
/// for itself, as `upgrade` does, and nothing more is needed of it;
/// `upgrade` asks the Hub, and is not run here.
#[tokio::test]
async fn each_command_that_changes_the_library_posts_the_daemon_its_event() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let (port, asked) = daemon("204 No Content");
    let id = model.id.to_string();
    let mut expected = Vec::new();

    for change in [
        &["update", &id, "--name", "Renamed", "--force"][..],
        &["capabilities", &id, "--set", "supports-tool-calls"],
    ] {
        let argv = [&["gglib", "model"][..], change].concat();
        beside(port, Some(TOKEN), run(&ctx, &argv))
            .await
            .expect("the change is stored");

        let event = AppEvent::model_updated((&stored(&ctx, model.id).await).into());
        expected.extend(told(&[event]));
        assert_eq!(*asked.lock().unwrap(), expected, "{change:?}");
    }
    assert_eq!(stored(&ctx, model.id).await.name, "Renamed");

    let argv = ["gglib", "model", "remove", &id, "--force"];
    beside(port, Some(TOKEN), run(&ctx, &argv))
        .await
        .expect("the model is removed");

    expected.extend(told(&[AppEvent::model_removed(model.id)]));
    assert_eq!(*asked.lock().unwrap(), expected);
    assert!(row(&ctx, model.id).await.is_none());
}

/// A command that stores nothing has nothing to tell, and asks the daemon
/// nothing: a listing, a look at the flags, and an edit that is only
/// previewed.
#[tokio::test]
async fn a_command_that_changes_nothing_asks_the_daemon_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let (port, asked) = daemon("204 No Content");
    let id = model.id.to_string();

    for unchanged in [
        &["list"][..],
        &["capabilities", &id],
        &["update", &id, "--name", "Renamed", "--dry-run"],
    ] {
        let argv = [&["gglib", "model"][..], unchanged].concat();
        beside(port, Some(TOKEN), run(&ctx, &argv))
            .await
            .expect("the command runs");
    }

    assert_eq!(*asked.lock().unwrap(), []);
    assert_eq!(stored(&ctx, model.id).await.name, model.name);
}

/// A test's data root holds no token, as one no daemon was ever started
/// under holds none. A daemon on the port is then not this library's, and
/// is asked nothing of a change the command did store.
#[tokio::test]
async fn a_daemon_that_left_no_token_under_this_data_root_is_asked_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let (port, asked) = daemon("204 No Content");
    let id = model.id.to_string();

    let argv = ["gglib", "model", "update", &id, "--name", "Renamed", "-f"];
    STAND_IN_PORT
        .scope(port, run(&ctx, &argv))
        .await
        .expect("the rename is stored");

    assert_eq!(stored(&ctx, model.id).await.name, "Renamed");
    assert_eq!(*asked.lock().unwrap(), []);
}

/// A daemon leaves its token under the data root when it stops, so a token
/// there says only that one has run. With nothing on the port, or another
/// program on it, the change is stored and the command succeeds, as it does
/// under a root with no token.
#[tokio::test]
async fn a_token_left_by_a_daemon_that_has_stopped_does_not_fail_the_command() {
    let (held, asked) = another_program();
    for (port, on_it) in [(nobody(), "nothing"), (held, "another program")] {
        let dir = tempfile::tempdir().unwrap();
        let (ctx, model) = library(dir.path()).await;
        let id = model.id.to_string();

        let argv = ["gglib", "model", "update", &id, "--name", "Renamed", "-f"];
        let ended = beside(port, Some(TOKEN), run(&ctx, &argv)).await;

        assert!(ended.is_ok(), "{on_it} on the port: {ended:?}");
        assert_eq!(stored(&ctx, model.id).await.name, "Renamed", "{on_it}");
    }
    assert_eq!(*asked.lock().unwrap(), told(&[]));
}

/// The change is stored before anybody is told of it, so a daemon that
/// refuses the event does not fail the command.
#[tokio::test]
async fn a_daemon_that_refuses_the_event_does_not_fail_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let (port, asked) = daemon("401 Unauthorized");
    let id = model.id.to_string();

    let argv = ["gglib", "model", "update", &id, "--name", "Renamed", "-f"];
    let ended = beside(port, Some(TOKEN), run(&ctx, &argv)).await;

    assert!(ended.is_ok(), "{ended:?}");
    let renamed = stored(&ctx, model.id).await;
    assert_eq!(renamed.name, "Renamed");
    let refused = AppEvent::model_updated((&renamed).into());
    assert_eq!(*asked.lock().unwrap(), told(&[refused]));
}

/// What `ModelOps` stored before a command failed is told all the same: an
/// open app is behind on it whatever the command went on to answer. The
/// event is put where `ModelOps` puts one, ahead of a command that fails.
#[tokio::test]
async fn a_command_that_fails_still_tells_what_was_stored() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let (port, asked) = daemon("204 No Content");
    let stored_first = AppEvent::model_updated((&model).into());
    ctx.library_changes.emit(stored_first.clone());

    let argv = ["gglib", "model", "inspect", "no-such-model"];
    let ended = beside(port, Some(TOKEN), run(&ctx, &argv)).await;

    assert!(ended.is_err(), "{ended:?}");
    assert_eq!(*asked.lock().unwrap(), told(&[stored_first]));
}
