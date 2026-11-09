//! The limits a turn runs with at the daemon's two doors, against what its
//! chat saved and what this machine stores: a paired device's turn and this
//! machine's own, neither of which names a limit of its own.

use gglib_core::SettingsUpdate;
use gglib_core::domain::agent::{
    AgentConfig, DEFAULT_MAX_ITERATIONS, DEFAULT_MAX_STAGNATION_STEPS,
};
use gglib_core::domain::chat::ConversationSettings;

use super::hub_turn_tests::{chat, device, turn};
use super::plan;
use crate::handlers::agent::compose::config_for;
use crate::handlers::agent::run_fixture::{LOCAL, state};
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
    /// A paired device's turn.
    Device { saved: Option<usize> },
    /// This machine's own turn, which the page sends.
    Page { saved: Option<usize> },
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

/// The config `turn` runs with: read as its door reads it, then given its
/// limits as `prepare_over` gives them.
async fn config_of(state: &AppState, turn_at: Turn) -> AgentConfig {
    let (scope, saved) = match turn_at {
        Turn::Device { saved } => (device("phone"), saved),
        Turn::Page { saved } => (LOCAL, saved),
    };
    let id = chat_saving(state, saved).await;
    let plan = plan(state, &scope, turn(id, "second"), None).await.unwrap();
    config_for(state, plan.chat.limits).await
}

/// Each row is the iteration limit this machine stores, the turn, and the
/// limit it runs with: the one its chat saved, then the stored one, then
/// the default. The same order at both doors.
#[tokio::test]
async fn a_turn_runs_with_its_chats_iteration_limit_then_the_stored_one() {
    let (_dir, state) = state().await;
    let device = |saved| Turn::Device { saved };
    let page = |saved| Turn::Page { saved };
    let table = [
        // The limit its chat saved.
        (Some(7), page(Some(4)), 4),
        (Some(7), device(Some(4)), 4),
        (None, page(Some(4)), 4),
        (None, device(Some(4)), 4),
        // The chat saved none: the stored one.
        (Some(7), page(None), 7),
        (Some(7), device(None), 7),
        // Nothing is stored: the default.
        (None, page(None), DEFAULT_MAX_ITERATIONS),
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

/// The two tool limits a chat saved (`PUT /api/conversations/{id}`) are the
/// ones this machine's turn runs with, as a device's does.
#[tokio::test]
async fn this_machines_turn_runs_with_the_tool_limits_its_chat_saved() {
    let (_dir, state) = state().await;
    let settings = ConversationSettings {
        max_parallel: Some(3),
        tool_timeout_ms: Some(45_000),
        ..ConversationSettings::default()
    };
    let id = chat(&state, Some(settings)).await;
    for scope in [LOCAL, device("phone")] {
        let plan = plan(&state, &scope, turn(id, "second"), None)
            .await
            .unwrap();
        let config = config_for(&state, plan.chat.limits).await;
        assert_eq!(
            (config.max_parallel_tools, config.tool_timeout_ms),
            (3, 45_000),
            "{scope:?}"
        );
    }
}

/// No turn names a stagnation limit: it is the stored one at both doors,
/// and the default when none is stored.
#[tokio::test]
async fn the_stagnation_limit_is_the_stored_one_at_both_doors() {
    let (_dir, state) = state().await;
    let turns = [
        Turn::Device { saved: Some(4) },
        Turn::Page { saved: Some(4) },
        Turn::Page { saved: None },
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
