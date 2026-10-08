use std::time::Duration;

use super::*;

/// A time `secs` after the epoch, so a test says which second it means.
fn at(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn started_at(secs: u64) -> SlotCacheState {
    SlotCacheState::new(at(secs))
}

#[test]
fn a_saved_session_is_hot_under_its_own_model_only() {
    let state = started_at(100);
    assert!(!state.is_hot(7, "planner"));

    state.mark_hot(7, "planner");

    assert!(state.is_hot(7, "planner"));
    assert!(!state.is_hot(8, "planner"), "another model's session");
    assert!(!state.is_hot(7, "coder"), "another session");
}

/// The clear route knows a session and no model, so the hot session is
/// forgotten by its id alone.
#[test]
fn clearing_a_session_forgets_it_under_whichever_model_it_was_hot() {
    let state = started_at(100);
    state.mark_hot(7, "planner");

    state.clear_session("planner");

    assert!(!state.is_hot(7, "planner"));
}

#[test]
fn clearing_one_session_leaves_another_hot() {
    let state = started_at(100);
    state.mark_hot(7, "planner");

    state.clear_session("coder");

    assert!(state.is_hot(7, "planner"));
    assert!(state.may_save("planner"));
}

#[test]
fn a_cleared_session_may_not_save_until_it_is_restored() {
    let state = started_at(100);
    assert!(state.may_save("planner"));

    state.clear_session("planner");
    assert!(!state.may_save("planner"));
    assert!(state.may_save("coder"), "the clear named one session");

    state.restore_attempted("planner");
    assert!(state.may_save("planner"));
}

#[test]
fn clearing_everything_blocks_every_save_and_forgets_the_hot_session() {
    let state = started_at(100);
    state.mark_hot(7, "planner");

    state.clear_all();

    assert!(!state.is_hot(7, "planner"));
    assert!(!state.may_save("planner"));
    assert!(!state.may_save("coder"));

    // Any session's restore lifts it: the flag is not per session.
    state.restore_attempted("coder");
    assert!(state.may_save("planner"));
    assert!(state.may_save("coder"));
}

#[test]
fn a_restart_forgets_the_hot_session_blocks_saves_and_moves_the_cutoff() {
    let state = started_at(100);
    state.mark_hot(7, "planner");

    assert!(state.on_restart(at(160)));

    assert!(!state.is_hot(7, "planner"));
    assert!(!state.may_save("planner"));
    assert_eq!(state.server_start_secs(), 160);
}

/// Every request a fresh spawn satisfies reports it. The second report must
/// not block saving again once the first request's restore has lifted it.
#[test]
fn a_restart_reported_twice_is_recorded_once() {
    let state = started_at(100);
    assert!(state.on_restart(at(160)));
    state.restore_attempted("planner");
    state.mark_hot(7, "planner");

    assert!(!state.on_restart(at(160)));

    assert!(state.is_hot(7, "planner"));
    assert!(state.may_save("planner"));
    assert_eq!(state.server_start_secs(), 160);
}

#[test]
fn a_restart_no_later_than_the_recorded_start_is_not_recorded() {
    let state = started_at(100);
    state.mark_hot(7, "planner");

    assert!(!state.on_restart(at(99)));

    assert!(state.is_hot(7, "planner"));
    assert_eq!(state.server_start_secs(), 100);
}

/// A clear that lands while its session generates skips that cycle's save.
/// Were the session then marked hot, its next request would skip the
/// restore, and the restore is what lets it save again.
#[test]
fn a_session_cleared_while_it_generates_is_not_marked_hot() {
    let state = started_at(100);
    state.restore_attempted("planner");

    state.clear_session("planner");
    state.mark_hot(7, "planner");
    assert!(!state.is_hot(7, "planner"));

    state.restore_attempted("planner");
    state.mark_hot(7, "planner");
    assert!(state.is_hot(7, "planner"));
}

/// The session that generated is what the server holds, saved or not. One
/// left hot from before it would skip its restore on a server that no longer
/// holds it, and prefill its whole prompt again.
#[test]
fn a_session_cleared_while_it_generates_leaves_no_earlier_session_hot() {
    let state = started_at(100);
    state.mark_hot(7, "planner");
    state.restore_attempted("coder");

    state.clear_session("coder");
    state.mark_hot(7, "coder");

    assert!(!state.is_hot(7, "coder"));
    assert!(!state.is_hot(7, "planner"), "the server's RAM is coder's");
}

#[test]
fn nothing_is_marked_hot_while_everything_is_cleared() {
    let state = started_at(100);

    state.clear_all();
    state.mark_hot(7, "planner");

    assert!(!state.is_hot(7, "planner"));
}
