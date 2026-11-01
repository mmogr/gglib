//! `get_by_identifier` over the real store, for the ids `/v1/models` lists
//! two models of one name under: `name@QUANT` and `name@<id>`.

use std::path::PathBuf;

use chrono::Utc;
use gglib_core::NewModel;

use super::*;
use crate::setup::setup_test_database;

/// A model named `name` with `quantization`, in a file of its own.
fn model(name: &str, file: &str, quantization: Option<&str>) -> NewModel {
    let mut new = NewModel::new(
        name.to_string(),
        PathBuf::from(format!("/models/{file}.gguf")),
        7.0,
        Utc::now(),
    );
    new.quantization = quantization.map(str::to_owned);
    new
}

/// `qwen3` twice, as `Q4_K_M` and `Q6_K`, and `mistral` once: their ids, in
/// that order.
async fn library() -> (SqliteModelRepository, [i64; 3]) {
    let repo = SqliteModelRepository::new(setup_test_database().await.unwrap());
    let q4 = repo
        .insert(&model("qwen3", "qwen3-q4", Some("Q4_K_M")))
        .await
        .unwrap();
    let q6 = repo
        .insert(&model("qwen3", "qwen3-q6", Some("Q6_K")))
        .await
        .unwrap();
    let mistral = repo
        .insert(&model("mistral", "mistral", Some("Q4_K_M")))
        .await
        .unwrap();
    (repo, [q4.id, q6.id, mistral.id])
}

async fn resolved(repo: &SqliteModelRepository, identifier: &str) -> Option<i64> {
    repo.get_by_identifier(identifier)
        .await
        .unwrap()
        .map(|m| m.id)
}

#[tokio::test]
async fn a_quantization_suffix_finds_that_quantization() {
    let (repo, [q4, q6, _]) = library().await;
    assert_eq!(resolved(&repo, "qwen3@Q6_K").await, Some(q6));
    assert_eq!(resolved(&repo, "qwen3@Q4_K_M").await, Some(q4));
}

#[tokio::test]
async fn an_id_suffix_finds_that_model() {
    let (repo, [_, q6, _]) = library().await;
    assert_eq!(resolved(&repo, &format!("qwen3@{q6}")).await, Some(q6));
}

/// The id must be a model of the name before the `@`.
#[tokio::test]
async fn an_id_suffix_naming_another_models_id_finds_nothing() {
    let (repo, [_, _, mistral]) = library().await;
    assert_eq!(resolved(&repo, &format!("qwen3@{mistral}")).await, None);
}

#[tokio::test]
async fn a_quantization_no_twin_has_finds_nothing() {
    let (repo, _) = library().await;
    assert_eq!(resolved(&repo, "qwen3@NOPE").await, None);
    assert_eq!(resolved(&repo, "mystery@Q6_K").await, None);
}

/// The same quantization twice is listed by id, so the quantization finds
/// neither.
#[tokio::test]
async fn a_quantization_two_twins_share_finds_nothing() {
    let (repo, _) = library().await;
    repo.insert(&model("qwen3", "qwen3-q6-other-repo", Some("Q6_K")))
        .await
        .unwrap();
    assert_eq!(resolved(&repo, "qwen3@Q6_K").await, None);
}

/// A model whose own name has an `@` is found by that name before the name is
/// read as a listed id.
#[tokio::test]
async fn an_exact_name_with_an_at_is_found_first() {
    let (repo, _) = library().await;
    let literal = repo
        .insert(&model("qwen3@Q6_K", "literal", None))
        .await
        .unwrap();
    assert_eq!(resolved(&repo, "qwen3@Q6_K").await, Some(literal.id));
}

/// A bare shared name is still the lowest row, as configs written before
/// the list told the twins apart expect.
#[tokio::test]
async fn a_bare_shared_name_is_still_the_lowest_row() {
    let (repo, [q4, _, _]) = library().await;
    assert_eq!(resolved(&repo, "qwen3").await, Some(q4));
}

#[tokio::test]
async fn list_by_name_is_every_model_of_that_name_lowest_id_first() {
    let (repo, [q4, q6, _]) = library().await;
    let named: Vec<i64> = repo
        .list_by_name("qwen3")
        .await
        .unwrap()
        .iter()
        .map(|m| m.id)
        .collect();
    assert_eq!(named, [q4, q6]);
    assert!(repo.list_by_name("ghost").await.unwrap().is_empty());
}
