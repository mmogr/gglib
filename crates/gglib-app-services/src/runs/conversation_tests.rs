//! One live reply per conversation: a second run for a conversation is
//! refused until the first has ended and its reply is saved.

use gglib_core::domain::runs::{RunError, RunKind};
use gglib_core::ports::{RunScope, RunsError, RunsPort};
use tokio::sync::oneshot;

use super::RunRegistry;
use super::cell::RunSpec;
use super::local::{Reservation, RunEnded};
use super::test_executor::{registry, until};

fn agent(conversation_id: Option<i64>) -> RunSpec {
    RunSpec {
        kind: RunKind::Agent,
        model: None,
        conversation_id,
    }
}

/// Start `id` on `conversation`: its work ends when `finish` is sent, and
/// its end handler (the reply's save) returns when `save` is sent.
fn start(
    runs: &RunRegistry,
    id: &str,
    conversation: Option<i64>,
) -> (oneshot::Sender<()>, oneshot::Sender<()>) {
    let (finish, finished) = oneshot::channel::<()>();
    let (save, saved) = oneshot::channel::<()>();
    let ended: RunEnded = Box::new(move |_, _| {
        Box::pin(async move {
            let _ = saved.await;
            Ok(())
        })
    });
    let Ok(Reservation::New(reserved)) = runs.reserve(id, agent(conversation)) else {
        panic!("a new reservation");
    };
    reserved.start(
        |_| {
            Box::pin(async move {
                let _ = finished.await;
                Ok::<(), RunError>(())
            })
        },
        ended,
    );
    (finish, save)
}

fn busy(run: &str) -> RunsError {
    RunsError::ConversationBusy {
        conversation_id: 7,
        run: run.to_owned(),
    }
}

/// Whether `id` on `conversation` would be admitted (the reservation is
/// dropped again).
fn admits(runs: &RunRegistry, id: &str, conversation: Option<i64>) -> bool {
    matches!(
        runs.reserve(id, agent(conversation)),
        Ok(Reservation::New(_))
    )
}

#[tokio::test]
async fn a_second_run_is_refused_until_the_first_reply_is_saved() {
    let (runs, _, _) = registry();
    let (finish, save) = start(&runs, "a1", Some(7));

    let refused = runs.reserve("a2", agent(Some(7))).err();
    assert_eq!(refused, Some(busy("a1")));
    assert_eq!(
        refused.unwrap().to_string(),
        "conversation 7 already has a live reply, run a1; stop it or wait"
    );

    // Ended, but its reply not yet saved: still live.
    finish.send(()).unwrap();
    until(|| runs.lock().runs["a1"].ending().status.is_terminal()).await;
    assert_eq!(runs.reserve("a2", agent(Some(7))).err(), Some(busy("a1")));
    assert!(runs.existing("a2").unwrap().is_none(), "nothing reserved");

    save.send(()).unwrap();
    until(|| runs.lock().runs["a1"].is_ended()).await;
    assert!(admits(&runs, "a2", Some(7)));
}

#[tokio::test]
async fn a_cancelled_run_frees_its_conversation_once_settled() {
    let (runs, _, _) = registry();
    let (_finish, save) = start(&runs, "a1", Some(7));

    runs.cancel(&RunScope::Local, "a1").unwrap();
    assert_eq!(runs.reserve("a2", agent(Some(7))).err(), Some(busy("a1")));

    save.send(()).unwrap();
    until(|| runs.lock().runs["a1"].is_ended()).await;
    assert!(admits(&runs, "a2", Some(7)));
}

#[tokio::test]
async fn runs_without_a_conversation_or_on_another_are_not_refused() {
    let (runs, _, _) = registry();
    let (_f1, _s1) = start(&runs, "a1", None);
    let (_f2, _s2) = start(&runs, "a2", Some(7));

    assert!(admits(&runs, "a3", None));
    assert!(admits(&runs, "a4", Some(8)));
}

#[tokio::test]
async fn a_retry_of_the_live_runs_id_answers_with_that_run() {
    let (runs, _, _) = registry();
    let (_finish, _save) = start(&runs, "a1", Some(7));

    let Ok(Reservation::Existing(info)) = runs.reserve("a1", agent(Some(7))) else {
        panic!("the existing run");
    };
    assert_eq!(info.id, "a1");
}
