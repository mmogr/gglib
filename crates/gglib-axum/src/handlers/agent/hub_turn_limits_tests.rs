//! The limits a turn runs with at the daemon's two doors, against what its
//! chat saved and what this machine stores: a paired device's turn, which
//! carries only its message, and the page's run, whose body may name limits
//! of its own.

use gglib_core::SettingsUpdate;
use gglib_core::domain::agent::{
    AgentConfig, DEFAULT_MAX_ITERATIONS, DEFAULT_MAX_STAGNATION_STEPS,
};
use gglib_core::domain::chat::ConversationSettings;
use serde_json::{Value, json};

use super::hub_turn_tests::{chat, turn};
use super::plan;
use crate::handlers::agent::compose::config_for;
use crate::handlers::agent::dto::{AgentRequestConfig, AgentRunRequest};
use crate::handlers::agent::run;
use crate::handlers::agent::run_fixture::state;
use crate::state::AppState;

/// Store this machine's two limits; `None` stores none.
async fn store(state: &AppState, iterations: Option<u32>, stagnation: Option<u32>) {
    state
        .core
        .settings()
        .update(SettingsUpdate {
            max_tool_iterations: Some(iterations),
            max_stagnation_steps: Some(stagnation),
            ..SettingsUpdate::default()
        })
        .await
        .unwrap();
}

/// A turn as it reaches one of the two doors, on a chat that saved the
/// iteration limit `saved`, if any.
#[derive(Debug, Clone, Copy)]
enum Turn {
    /// A paired device's turn, which names no limit of its own.
    Device { saved: Option<usize> },
    /// The page's run, whose body names the limit `names`, if any.
    Page {
        names: Option<usize>,
        saved: Option<usize>,
    },
}

/// A chat whose settings save the iteration limit `saved`, or one with no
/// settings at all.
async fn chat_saving(state: &AppState, saved: Option<usize>) -> i64 {
    let settings = saved.map(|_| ConversationSettings {
        max_iterations: saved,
        ..ConversationSettings::default()
    });
    chat(state, settings).await
}

/// The page's run on chat `id` with `config` as its body's, as its door
/// reads it: the config it goes on to `prepare` with.
async fn page_run(state: &AppState, id: Option<i64>, config: Value) -> Option<AgentRequestConfig> {
    let request: AgentRunRequest = serde_json::from_value(json!({
        "port": 9000,
        "messages": [{ "role": "user", "content": "hi" }],
        "config": config,
        "conversation_id": id,
    }))
    .unwrap();
    run::plan(state, request).await.unwrap().0.config
}

/// The config `turn` runs with: read as its door reads it, then given its
/// limits as `prepare` gives them.
async fn config_of(state: &AppState, turn_at: Turn) -> AgentConfig {
    let named = match turn_at {
        Turn::Device { saved } => {
            let id = chat_saving(state, saved).await;
            plan(state, turn(id, "second")).await.unwrap().chat.config
        }
        Turn::Page { names, saved } => {
            let id = chat_saving(state, saved).await;
            let config = names.map_or(Value::Null, |n| json!({ "max_iterations": n }));
            page_run(state, Some(id), config).await
        }
    };
    config_for(state, named).await
}

/// Each row is the iteration limit this machine stores, the turn, and the
/// limit it runs with: the one the turn names, then the one its chat saved,
/// then the stored one, then the default. The same order at both doors.
#[tokio::test]
async fn a_turn_runs_with_its_own_iteration_limit_then_its_chats_then_the_stored_one() {
    let (_dir, state) = state().await;
    let device = |saved| Turn::Device { saved };
    let page = |names, saved| Turn::Page { names, saved };
    let table = [
        // The turn's own limit, whatever its chat saved.
        (Some(7), page(Some(3), Some(4)), 3),
        (Some(7), page(Some(3), None), 3),
        (None, page(Some(3), Some(4)), 3),
        (None, page(Some(3), None), 3),
        // It names none: the limit its chat saved.
        (Some(7), page(None, Some(4)), 4),
        (Some(7), device(Some(4)), 4),
        (None, page(None, Some(4)), 4),
        (None, device(Some(4)), 4),
        // The chat saved none either: the stored one.
        (Some(7), page(None, None), 7),
        (Some(7), device(None), 7),
        // Nothing is stored: the default.
        (None, page(None, None), DEFAULT_MAX_ITERATIONS),
        (None, device(None), DEFAULT_MAX_ITERATIONS),
    ];
    for (stored, turn_at, want) in table {
        store(&state, stored, None).await;
        assert_eq!(
            config_of(&state, turn_at).await.max_iterations,
            want,
            "stored {stored:?}, {turn_at:?}"
        );
    }
}

/// The page sends a config that names its other limits and a `null`
/// iteration limit. On a chat that saved one it takes the chat's and keeps
/// what it named; on a chat that saved none, the stored one.
#[tokio::test]
async fn a_config_that_names_other_limits_takes_its_chats_iteration_limit_then_the_stored_one() {
    let (_dir, state) = state().await;
    store(&state, Some(7), None).await;
    let sent = json!({ "max_iterations": null, "max_parallel_tools": 4 });
    for (saved, want) in [(Some(5), 5), (None, 7)] {
        let id = chat_saving(&state, saved).await;
        let named = page_run(&state, Some(id), sent.clone()).await;
        let config = config_for(&state, named).await;
        assert_eq!(
            (config.max_iterations, config.max_parallel_tools),
            (want, 4),
            "saved {saved:?}"
        );
    }
}

/// A run saved to no conversation has no chat to take a limit from: its own,
/// or the stored one.
#[tokio::test]
async fn a_run_with_no_conversation_takes_its_own_limit_or_the_stored_one() {
    let (_dir, state) = state().await;
    store(&state, Some(7), None).await;
    for (config, want) in [(Value::Null, 7), (json!({ "max_iterations": 3 }), 3)] {
        let named = page_run(&state, None, config).await;
        assert_eq!(config_for(&state, named).await.max_iterations, want);
    }
}

/// No turn names a stagnation limit: it is the stored one at both doors,
/// and the default when none is stored.
#[tokio::test]
async fn the_stagnation_limit_is_the_stored_one_at_both_doors() {
    let (_dir, state) = state().await;
    let turns = [
        Turn::Device { saved: Some(4) },
        Turn::Page {
            names: Some(3),
            saved: None,
        },
        Turn::Page {
            names: None,
            saved: Some(4),
        },
        Turn::Page {
            names: None,
            saved: None,
        },
    ];
    for turn_at in turns {
        store(&state, Some(7), Some(9)).await;
        let stored = config_of(&state, turn_at).await;
        assert_eq!(stored.max_stagnation_steps, Some(9), "{turn_at:?}");
        store(&state, Some(7), None).await;
        let unset = config_of(&state, turn_at).await;
        assert_eq!(
            unset.max_stagnation_steps,
            Some(DEFAULT_MAX_STAGNATION_STEPS),
            "{turn_at:?}"
        );
    }
}
