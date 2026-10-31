//! The runtime's seven states as an app's five, and a watch answering the
//! current state first, nothing for a change that shows the same, and
//! `None` after `Stopped`.

use std::time::Duration;

use tokio::time::error::Elapsed;

use super::*;
use crate::test_library::wait;

fn ready(n_ctx: u64) -> EngineState {
    EngineState::Ready {
        id: 3,
        model: "small".to_owned(),
        n_ctx,
    }
}

#[test]
fn each_of_the_runtimes_states_is_one_an_app_shows() {
    let model = || "small".to_owned();
    let cases = [
        (engine::EngineState::Idle, EngineState::Idle),
        (
            engine::EngineState::Loading {
                id: 3,
                model: model(),
                fraction: 0.5,
            },
            EngineState::Loading {
                id: 3,
                model: model(),
                fraction: 0.5,
            },
        ),
        (
            engine::EngineState::Ready {
                id: 3,
                model: model(),
                n_ctx: 8192,
            },
            ready(8192),
        ),
        // A reply being written: the model is served at the same context.
        (
            engine::EngineState::Generating {
                id: 3,
                model: model(),
                n_ctx: 8192,
            },
            ready(8192),
        ),
        // A model being freed is no longer served.
        (
            engine::EngineState::Unloading {
                id: 3,
                model: model(),
            },
            EngineState::Idle,
        ),
        (
            engine::EngineState::Failed {
                id: 3,
                model: model(),
                reason: "no".to_owned(),
            },
            EngineState::Failed {
                id: 3,
                model: model(),
                reason: "no".to_owned(),
            },
        ),
        (engine::EngineState::Stopped, EngineState::Stopped),
    ];

    for (published, shown) in cases {
        assert_eq!(EngineState::from(&published), shown, "{published:?}");
    }
}

/// The next answer, or `Err` when there is none within a moment.
fn answer_soon(watch: &Arc<EngineWatch>) -> Result<Option<EngineState>, Elapsed> {
    let next = Arc::clone(watch).next();
    wait(async { tokio::time::timeout(Duration::from_millis(100), next).await })
}

#[test]
fn a_watch_answers_the_state_now_then_each_change_an_app_can_see() {
    let (tx, rx) = watch::channel(engine::EngineState::Ready {
        id: 3,
        model: "small".to_owned(),
        n_ctx: 4096,
    });
    let watch = Arc::new(EngineWatch::new(rx));

    assert_eq!(answer_soon(&watch), Ok(Some(ready(4096))));

    // A reply starts and ends: nothing an app shows has changed.
    tx.send_replace(engine::EngineState::Generating {
        id: 3,
        model: "small".to_owned(),
        n_ctx: 4096,
    });
    assert!(answer_soon(&watch).is_err());
    tx.send_replace(engine::EngineState::Ready {
        id: 3,
        model: "small".to_owned(),
        n_ctx: 4096,
    });
    assert!(answer_soon(&watch).is_err());

    tx.send_replace(engine::EngineState::Unloading {
        id: 3,
        model: "small".to_owned(),
    });
    assert_eq!(answer_soon(&watch), Ok(Some(EngineState::Idle)));
    tx.send_replace(engine::EngineState::Idle);
    assert!(answer_soon(&watch).is_err());

    tx.send_replace(engine::EngineState::Stopped);
    drop(tx);
    assert_eq!(answer_soon(&watch), Ok(Some(EngineState::Stopped)));
    assert_eq!(answer_soon(&watch), Ok(None));
    assert_eq!(answer_soon(&watch), Ok(None));
}

/// A runtime dropped without a shutdown never said `Stopped`; its watch
/// says it, once.
#[test]
fn a_watch_of_an_engine_that_went_without_a_word_ends_stopped() {
    let (tx, rx) = watch::channel(engine::EngineState::Idle);
    let watch = Arc::new(EngineWatch::new(rx));
    assert_eq!(answer_soon(&watch), Ok(Some(EngineState::Idle)));

    drop(tx);

    assert_eq!(answer_soon(&watch), Ok(Some(EngineState::Stopped)));
    assert_eq!(answer_soon(&watch), Ok(None));
}
