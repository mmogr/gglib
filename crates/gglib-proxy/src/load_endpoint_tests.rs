//! What a load records and answers, by runtime: a llama-server it started is
//! a restart to the slot cache and answers its context; an image model on
//! `sd-server` is neither, and answers context 0. Both narrate.

use std::time::{Duration, UNIX_EPOCH};

use gglib_core::domain::{LaunchDecision, LaunchNarration};

use super::*;

/// The proxy started at second 100; the load lands at second 200.
fn started_at_100() -> SlotCacheState {
    SlotCacheState::new(UNIX_EPOCH + Duration::from_secs(100))
}

fn at_200() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(200)
}

/// `name` on port 9001, just started, launched at a 32k context.
fn started(name: &str, runtime: RuntimeKind) -> RunningTarget {
    let mut narration = LaunchNarration::new(name, None, 0);
    narration.push(LaunchDecision::new("runtime", runtime.label(), "installed"));
    RunningTarget::local(9001, 7, name.to_owned(), 32_768, true)
        .with_runtime(runtime)
        .with_narration(narration)
}

#[test]
fn an_image_model_answers_no_context_and_leaves_the_slot_cache_alone() {
    let slot_cache = started_at_100();
    let launch = LaunchNarrationCache::new();
    let target = started("flux", RuntimeKind::StableDiffusion);
    let narration = target.narration.clone();

    let answer = loaded(target, &slot_cache, &launch, at_200());

    assert_eq!(
        (answer.model.as_str(), answer.started, answer.context),
        ("flux", true, 0)
    );
    assert_eq!(
        slot_cache.server_start_secs(),
        100,
        "no llama-server restarted"
    );
    assert_eq!(launch.get(), narration, "the load is narrated");
}

#[test]
fn a_chat_model_it_started_answers_its_context_and_restarts_the_slot_cache() {
    let slot_cache = started_at_100();
    let launch = LaunchNarrationCache::new();
    let target = started("qwen", RuntimeKind::Llama);
    let narration = target.narration.clone();

    let answer = loaded(target, &slot_cache, &launch, at_200());

    assert_eq!(
        (answer.model.as_str(), answer.started, answer.context),
        ("qwen", true, 32_768)
    );
    assert_eq!(slot_cache.server_start_secs(), 200, "a restart is recorded");
    assert_eq!(launch.get(), narration);
}

#[test]
fn a_chat_model_found_running_is_not_a_restart() {
    let slot_cache = started_at_100();
    let mut target = started("qwen", RuntimeKind::Llama);
    target.just_started = false;

    let answer = loaded(target, &slot_cache, &LaunchNarrationCache::new(), at_200());

    assert!(!answer.started);
    assert_eq!(slot_cache.server_start_secs(), 100);
}
