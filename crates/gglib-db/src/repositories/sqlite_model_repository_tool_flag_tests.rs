//! A re-registration of one file and the stored tool flag: kept when the row
//! holds llama-server's answer, re-guessed with the other bits when it does
//! not.

use std::path::PathBuf;

use chrono::Utc;
use gglib_core::NewModel;
use gglib_core::domain::{ModelCapabilities, TemplateCaps};

use super::*;
use crate::setup::setup_test_database;

const TOOLS: ModelCapabilities = ModelCapabilities::SUPPORTS_TOOL_CALLS;
const SYSTEM: ModelCapabilities = ModelCapabilities::SUPPORTS_SYSTEM_ROLE;
const REASONING: ModelCapabilities = ModelCapabilities::SUPPORTS_REASONING;

/// The file `/models/a.gguf`, guessed as `capabilities`.
fn guessed(capabilities: ModelCapabilities) -> NewModel {
    let mut new = NewModel::new(
        "a".to_owned(),
        PathBuf::from("/models/a.gguf"),
        Some(7.0),
        Utc::now(),
    );
    new.capabilities = capabilities;
    new
}

/// A repo whose one row was registered as `first`, then given `caps` from a
/// launch and `flag` as its tool bit.
async fn launched(
    first: ModelCapabilities,
    caps: Option<TemplateCaps>,
    flag: bool,
) -> SqliteModelRepository {
    let repo = SqliteModelRepository::new(setup_test_database().await.unwrap());
    let mut model = repo.insert(&guessed(first)).await.unwrap();
    model.template_caps = caps;
    model.capabilities.set(TOOLS, flag);
    repo.update(&model).await.unwrap();
    repo
}

fn answered(calls: Option<bool>) -> TemplateCaps {
    TemplateCaps {
        supports_tool_calls: calls,
        ..TemplateCaps::default()
    }
}

/// The bits the one stored row holds after a re-registration guessed as
/// `again`.
async fn after(repo: &SqliteModelRepository, again: ModelCapabilities) -> ModelCapabilities {
    let reread = repo.insert(&guessed(again)).await.unwrap();
    let [stored] = &repo.list().await.unwrap()[..] else {
        panic!("one row");
    };
    assert_eq!(reread.capabilities, stored.capabilities);
    stored.capabilities
}

#[tokio::test]
async fn a_reimport_keeps_the_engines_tool_answer_and_re_guesses_the_rest() {
    let repo = launched(SYSTEM, Some(answered(Some(true))), true).await;
    assert_eq!(after(&repo, REASONING).await, REASONING | TOOLS);
}

/// The engine said yes and the owner turned it off since: a guess of yes
/// does not turn it back on.
#[tokio::test]
async fn a_reimport_keeps_the_owners_setting_over_an_engine_answer() {
    let repo = launched(SYSTEM, Some(answered(Some(true))), false).await;
    assert_eq!(after(&repo, REASONING | TOOLS).await, REASONING);
}

#[tokio::test]
async fn a_reimport_keeps_an_engine_no() {
    let repo = launched(TOOLS, Some(answered(Some(false))), false).await;
    assert_eq!(after(&repo, TOOLS).await, ModelCapabilities::empty());
}

/// No launch, or one whose caps did not say: the guess is taken whole, as
/// before.
#[tokio::test]
async fn a_reimport_without_an_engine_answer_takes_the_guess() {
    for caps in [None, Some(answered(None))] {
        let repo = launched(SYSTEM, caps.clone(), true).await;
        assert_eq!(after(&repo, REASONING).await, REASONING, "{caps:?}");
        assert_eq!(after(&repo, TOOLS).await, TOOLS, "{caps:?}");
    }
}

/// A stored value that is not a boolean is no answer: the guess is taken.
#[tokio::test]
async fn a_reimport_over_a_non_boolean_answer_takes_the_guess() {
    let repo = launched(SYSTEM, None, true).await;
    sqlx::query("UPDATE models SET template_caps = '{\"supports_tool_calls\":\"yes\"}'")
        .execute(repo.pool())
        .await
        .unwrap();
    assert_eq!(after(&repo, REASONING).await, REASONING);
}
