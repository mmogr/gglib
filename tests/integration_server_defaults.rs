//! Integration tests for per-model server defaults (server_defaults).
//!
//! These tests verify that `server_defaults` on [`Model`] round-trips correctly
//! through the SQLite repository: persistence, null-clearing, partial updates,
//! no-op omission, sequential overwrites, and boundary validation.

mod common;

use chrono::Utc;
use common::database::setup_test_pool;
use gglib_core::domain::ServerConfig;
use gglib_core::{ModelRepository, NewModel, services::ModelService};
use gglib_db::SqliteModelRepository;
use std::path::PathBuf;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a minimal [`NewModel`] with a unique path so each test gets an
/// independent `model_key` (avoids UPSERT collisions).
fn make_new_model(name: &str) -> NewModel {
    NewModel::new(
        name.to_string(),
        PathBuf::from(format!("/test/models/{name}.gguf")),
        7.0,
        Utc::now(),
    )
}

/// Convenience: create a `ServerConfig` with only `context_length` set.
fn cfg_ctx(len: usize) -> ServerConfig {
    ServerConfig {
        context_length: Some(len),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// (a) PATCH model with server_defaults containing context_length, verify it
/// persists and round-trips correctly through the DB.
#[tokio::test]
async fn test_server_defaults_patch_and_retrieve() {
    let pool = setup_test_pool().await.unwrap();
    let repo = Arc::new(SqliteModelRepository::new(pool));
    let service = ModelService::new(repo.clone());

    // Insert a model
    let new_model = make_new_model("patch-retrieve");
    let model = service.add(new_model).await.unwrap();
    assert!(
        model.server_defaults.is_none(),
        "fresh model has no server_defaults"
    );

    // Update: set server_defaults with context_length = 32768
    let mut updated = model.clone();
    updated.server_defaults = Some(cfg_ctx(32_768));
    service.update(&updated).await.unwrap();

    // Retrieve and verify
    let fetched = repo.get_by_id(model.id).await.unwrap();
    assert!(
        fetched.server_defaults.is_some(),
        "server_defaults should be set after update"
    );
    assert_eq!(
        fetched.server_defaults.as_ref().unwrap().context_length,
        Some(32_768),
        "context_length should round-trip as 32768"
    );
}

/// (b) Set server_defaults on a model, then clear it (set to None), verify DB
/// stores NULL.
#[tokio::test]
async fn test_server_defaults_null_clearing() {
    let pool = setup_test_pool().await.unwrap();
    let repo = Arc::new(SqliteModelRepository::new(pool));
    let service = ModelService::new(repo.clone());

    // Insert + set server_defaults
    let model = service.add(make_new_model("null-clear")).await.unwrap();
    let mut with_defaults = model.clone();
    with_defaults.server_defaults = Some(cfg_ctx(16_384));
    service.update(&with_defaults).await.unwrap();

    // Verify it's set
    let check = repo.get_by_id(model.id).await.unwrap();
    assert_eq!(
        check.server_defaults.as_ref().unwrap().context_length,
        Some(16_384)
    );

    // Clear: set server_defaults to None
    let mut cleared = model.clone();
    cleared.id = check.id; // keep the ID from DB
    cleared.server_defaults = None;
    service.update(&cleared).await.unwrap();

    // Verify it's NULL in DB
    let fetched = repo.get_by_id(model.id).await.unwrap();
    assert!(
        fetched.server_defaults.is_none(),
        "server_defaults should be None (NULL) after clearing"
    );
}

/// (c) Set multiple server_defaults fields, update only one, verify the other
/// survives.  Currently ServerConfig has only `context_length`, so this test
/// verifies that updating the model's *other* fields (e.g., name) doesn't
/// clobber server_defaults — i.e., the full-model UPDATE preserves the JSON
/// blob when we explicitly re-assign it.
#[tokio::test]
async fn test_server_defaults_partial_update_preserves_other_fields() {
    let pool = setup_test_pool().await.unwrap();
    let repo = Arc::new(SqliteModelRepository::new(pool));
    let service = ModelService::new(repo.clone());

    // Insert model with server_defaults
    let model = service.add(make_new_model("partial-update")).await.unwrap();
    let mut with_defaults = model.clone();
    with_defaults.server_defaults = Some(cfg_ctx(8192));
    service.update(&with_defaults).await.unwrap();

    // Now update a different field (name) while preserving server_defaults
    let mut renamed = repo.get_by_id(model.id).await.unwrap();
    renamed.name = "Renamed Model".to_string();
    // server_defaults is already Some from the fetch — just don't touch it
    service.update(&renamed).await.unwrap();

    // Verify both name change AND server_defaults survived
    let fetched = repo.get_by_id(model.id).await.unwrap();
    assert_eq!(fetched.name, "Renamed Model", "name should be updated");
    assert_eq!(
        fetched.server_defaults.as_ref().unwrap().context_length,
        Some(8192),
        "server_defaults.context_length should survive a name-only update"
    );
}

/// (d) Fetch a model that has server_defaults, modify an unrelated field, and
/// save — the existing server_defaults value should be untouched (no-op for
/// that key).  This is the "omitted key is no-op" behaviour: when you read
/// the full model from DB and write it back without touching server_defaults,
/// the value persists.
#[tokio::test]
async fn test_server_defaults_omitted_key_is_noop() {
    let pool = setup_test_pool().await.unwrap();
    let repo = Arc::new(SqliteModelRepository::new(pool));
    let service = ModelService::new(repo.clone());

    // Insert + set server_defaults
    let model = service.add(make_new_model("noop-omit")).await.unwrap();
    let mut with_defaults = model.clone();
    with_defaults.server_defaults = Some(cfg_ctx(4096));
    service.update(&with_defaults).await.unwrap();

    // Fetch the model (which includes server_defaults), change quantization,
    // save back — server_defaults should be preserved because it was read
    // from DB and written back.
    let mut modified = repo.get_by_id(model.id).await.unwrap();
    modified.quantization = Some("Q8_0".to_string());
    service.update(&modified).await.unwrap();

    // Verify server_defaults is untouched
    let fetched = repo.get_by_id(model.id).await.unwrap();
    assert_eq!(
        fetched.server_defaults.as_ref().unwrap().context_length,
        Some(4096),
        "server_defaults should be preserved when only quantization changes"
    );
    assert_eq!(fetched.quantization, Some("Q8_0".to_string()));
}

/// (e) Two sequential PATCH requests to the same model; verify last-writer-wins.
#[tokio::test]
async fn test_server_defaults_sequential_overwrite() {
    let pool = setup_test_pool().await.unwrap();
    let repo = Arc::new(SqliteModelRepository::new(pool));
    let service = ModelService::new(repo.clone());

    let model = service.add(make_new_model("seq-overwrite")).await.unwrap();

    // First write: context_length = 2048
    let mut first = model.clone();
    first.server_defaults = Some(cfg_ctx(2048));
    service.update(&first).await.unwrap();

    // Second write: context_length = 65536
    let mut second = repo.get_by_id(model.id).await.unwrap();
    second.server_defaults = Some(cfg_ctx(65_536));
    service.update(&second).await.unwrap();

    // Verify last-writer-wins
    let fetched = repo.get_by_id(model.id).await.unwrap();
    assert_eq!(
        fetched.server_defaults.as_ref().unwrap().context_length,
        Some(65_536),
        "last writer (65536) should win over first write (2048)"
    );

    // Third write: clear it again
    let mut third = repo.get_by_id(model.id).await.unwrap();
    third.server_defaults = None;
    service.update(&third).await.unwrap();

    let final_model = repo.get_by_id(model.id).await.unwrap();
    assert!(
        final_model.server_defaults.is_none(),
        "final clear should set server_defaults to None"
    );
}

/// (f) Attempt to set context_length to 0.  Document observed behaviour:
/// the current ServerConfig type stores `Option<usize>`, so 0 is a valid
/// usize value and will be accepted by the DB layer.  This test documents
/// that behaviour (accepts 0) rather than asserting rejection, because no
/// validation layer currently rejects zero.
#[tokio::test]
async fn test_server_defaults_context_length_zero_accepted() {
    let pool = setup_test_pool().await.unwrap();
    let repo = Arc::new(SqliteModelRepository::new(pool));
    let service = ModelService::new(repo.clone());

    let model = service.add(make_new_model("zero-ctx")).await.unwrap();

    // Set context_length to 0 — this is a valid usize, so the DB accepts it.
    // NOTE: If/when validation is added to reject or clamp zero, update this
    // test to assert the new behaviour (e.g., expect an error).
    let mut with_zero = model.clone();
    with_zero.server_defaults = Some(ServerConfig {
        context_length: Some(0),
    });

    // Currently accepted — no validation layer rejects zero.
    service.update(&with_zero).await.unwrap();

    let fetched = repo.get_by_id(model.id).await.unwrap();
    assert_eq!(
        fetched.server_defaults.as_ref().unwrap().context_length,
        Some(0),
        "context_length=0 is currently accepted (no validation rejects it)"
    );
}
