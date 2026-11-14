//! `record_template_caps` and the stored tool flag: llama-server's
//! `supports_tool_calls` is written into it when first read and when it
//! changes, and an unchanged answer leaves the owner's own setting alone.

use std::sync::Arc;

use super::port_tests::RecordingRepo;
use super::*;
use crate::domain::{ModelCapabilities, TemplateCaps};

fn calls(answer: Option<bool>) -> TemplateCaps {
    TemplateCaps {
        supports_tool_calls: answer,
        ..TemplateCaps::default()
    }
}

/// A repo whose one row holds `stored` caps and `flag` as its tool bit.
fn repo(stored: Option<TemplateCaps>, flag: bool) -> Arc<RecordingRepo> {
    let repo = RecordingRepo::with_caps(stored);
    repo.stored
        .lock()
        .unwrap()
        .capabilities
        .set(ModelCapabilities::SUPPORTS_TOOL_CALLS, flag);
    Arc::new(repo)
}

/// The row's tool bit after `caps` is recorded onto it.
async fn flag_after(repo: &Arc<RecordingRepo>, caps: TemplateCaps) -> bool {
    let port = CatalogPortImpl::new(Arc::clone(repo) as Arc<dyn ModelRepository>);
    port.record_template_caps(7, caps).await.unwrap();
    repo.stored
        .lock()
        .unwrap()
        .capabilities
        .supports_tool_calls()
}

#[tokio::test]
async fn a_first_answer_sets_the_flag_either_way() {
    assert!(flag_after(&repo(None, false), calls(Some(true))).await);
    assert!(!flag_after(&repo(None, true), calls(Some(false))).await);
}

/// A stored observation that did not say counts as no answer yet.
#[tokio::test]
async fn an_answer_after_an_unknown_one_sets_the_flag() {
    let repo = repo(Some(calls(None)), true);
    assert!(!flag_after(&repo, calls(Some(false))).await);
}

/// The owner flipped the flag after the engine's answer; the same answer at
/// the next launch leaves the owner's setting.
#[tokio::test]
async fn the_same_answer_again_leaves_the_owners_setting() {
    let repo = repo(None, false);
    assert!(flag_after(&repo, calls(Some(true))).await);
    repo.stored
        .lock()
        .unwrap()
        .capabilities
        .remove(ModelCapabilities::SUPPORTS_TOOL_CALLS);
    assert!(!flag_after(&repo, calls(Some(true))).await);
}

#[tokio::test]
async fn a_changed_answer_applies() {
    let repo = repo(Some(calls(Some(true))), true);
    assert!(!flag_after(&repo, calls(Some(false))).await);
}

/// Unknown decides nothing, whatever was stored before.
#[tokio::test]
async fn no_answer_leaves_the_flag() {
    assert!(flag_after(&repo(None, true), calls(None)).await);
    assert!(!flag_after(&repo(Some(calls(Some(true))), false), calls(None)).await);
}

/// Other caps moved and the tool answer did not: the caps are stored, the
/// flag is left.
#[tokio::test]
async fn a_caps_change_with_the_same_tool_answer_leaves_the_flag() {
    let repo = repo(Some(calls(Some(true))), false);
    let mut caps = calls(Some(true));
    caps.supports_reasoning_effort = Some(true);
    assert!(!flag_after(&repo, caps.clone()).await);
    assert_eq!(repo.update_count(), 1);
    assert_eq!(repo.stored.lock().unwrap().template_caps, Some(caps));
}
