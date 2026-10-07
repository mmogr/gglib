//! The limits a turn runs with at the daemon's two doors, against what
//! this machine stores: a paired device's turn, which carries only its
//! message, and the page's run, whose body may name limits of its own.

use gglib_core::SettingsUpdate;
use gglib_core::domain::agent::{
    AgentConfig, DEFAULT_MAX_ITERATIONS, DEFAULT_MAX_STAGNATION_STEPS,
};
use gglib_core::domain::chat::ConversationSettings;
use serde_json::{Value, json};

use super::hub_turn_tests::{chat, turn};
use super::plan;
use crate::handlers::agent::compose::config_for;
use crate::handlers::agent::dto::AgentRunRequest;
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

/// A turn as it reaches one of the two doors, with the iteration limit it
/// names, if any.
#[derive(Debug, Clone, Copy)]
enum Turn {
    /// A paired device's turn, on a chat whose settings name the limit.
    Device(Option<usize>),
    /// The page's run, whose body names the limit.
    Page(Option<usize>),
}

/// The config `turn` runs with: read as its door reads it, then given its
/// limits as `prepare` gives them.
async fn config_of(state: &AppState, turn_at: Turn) -> AgentConfig {
    let named = match turn_at {
        Turn::Device(max_iterations) => {
            let settings = max_iterations.map(|_| ConversationSettings {
                max_iterations,
                ..ConversationSettings::default()
            });
            let id = chat(state, settings).await;
            plan(state, turn(id, "second")).await.unwrap().chat.config
        }
        Turn::Page(max_iterations) => {
            let config = max_iterations.map_or(Value::Null, |n| json!({ "max_iterations": n }));
            let request: AgentRunRequest = serde_json::from_value(json!({
                "port": 9000,
                "messages": [{ "role": "user", "content": "hi" }],
                "config": config,
            }))
            .unwrap();
            run::plan(state, request).await.unwrap().0.config
        }
    };
    config_for(state, named).await
}

/// Each row is the iteration limit this machine stores, the turn, and the
/// limit it runs with.
#[tokio::test]
async fn a_turn_runs_with_its_own_iteration_limit_then_its_chats_then_the_stored_one() {
    let (_dir, state) = state().await;
    let table = [
        // The phone's shape, and the page's now that it sends no limit.
        (Some(7), Turn::Device(None), 7),
        (Some(7), Turn::Page(None), 7),
        (Some(7), Turn::Page(Some(3)), 3),
        (Some(7), Turn::Device(Some(4)), 4),
        (None, Turn::Device(None), DEFAULT_MAX_ITERATIONS),
        (None, Turn::Page(None), DEFAULT_MAX_ITERATIONS),
        (None, Turn::Page(Some(3)), 3),
        (None, Turn::Device(Some(4)), 4),
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

/// The page's config that names other limits and no iteration limit still
/// takes the stored one, and keeps what it named.
#[tokio::test]
async fn a_config_that_names_other_limits_takes_the_stored_iteration_limit() {
    let (_dir, state) = state().await;
    store(&state, Some(7), None).await;
    let named = serde_json::from_value(json!({ "max_parallel_tools": 4 })).unwrap();
    let config = config_for(&state, Some(named)).await;
    assert_eq!((config.max_iterations, config.max_parallel_tools), (7, 4));
}

/// No turn names a stagnation limit: it is the stored one at both doors,
/// and the default when none is stored.
#[tokio::test]
async fn the_stagnation_limit_is_the_stored_one_at_both_doors() {
    let (_dir, state) = state().await;
    for turn_at in [Turn::Device(Some(4)), Turn::Page(Some(3)), Turn::Page(None)] {
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
