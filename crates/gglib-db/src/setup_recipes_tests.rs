//! The pass that clears stored copies of the `reasoning` recipe, on a
//! database file of the test's own: which rows it clears, which it keeps, and
//! that it runs once.

use gglib_core::domain::{DefaultsOrigin, InferenceConfig};

use super::*;
use crate::setup_database;

/// A database as the build before this pass left it: this build's schema,
/// stamped at the loop guard's version.
async fn left_by_an_older_build(path: &std::path::Path) -> SqlitePool {
    let pool = setup_database(path).await.expect("a fresh database");
    sqlx::query("PRAGMA user_version = 2")
        .execute(&pool)
        .await
        .expect("a stamp");
    pool
}

/// Plant one model row holding `defaults` labelled `origin`.
async fn plant(pool: &SqlitePool, name: &str, defaults: Option<&str>, origin: Option<&str>) {
    sqlx::query(
        "INSERT INTO models (name, file_path, param_count_b, added_at, model_key, \
         inference_defaults, defaults_origin) VALUES (?, ?, 1.0, 'then', ?, ?, ?)",
    )
    .bind(name)
    .bind(format!("/models/{name}.gguf"))
    .bind(format!("local:{name}"))
    .bind(defaults)
    .bind(origin)
    .execute(pool)
    .await
    .expect("a planted row");
}

async fn stored(pool: &SqlitePool, name: &str) -> (Option<String>, Option<String>) {
    sqlx::query_as("SELECT inference_defaults, defaults_origin FROM models WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("the row")
}

fn json(config: &InferenceConfig) -> String {
    serde_json::to_string(config).expect("serialise")
}

#[tokio::test]
async fn stored_copies_are_cleared_and_everything_else_is_kept() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let pool = left_by_an_older_build(&dir.path().join("gglib.db")).await;

    let recipe = json(&STORED_RECIPE);
    let tuned = json(&InferenceConfig {
        temperature: Some(0.3),
        ..Default::default()
    });
    plant(&pool, "labelled", Some(&recipe), Some("auto_detected")).await;
    plant(&pool, "unlabelled", Some(&recipe), None).await;
    plant(&pool, "by-hand", Some(&recipe), Some("user")).await;
    plant(&pool, "legacy-tuned", Some(&tuned), None).await;
    plant(&pool, "published", Some(&tuned), Some("published")).await;
    plant(&pool, "unreadable", Some("{not json"), None).await;

    drop_stored_reasoning_recipes(&pool)
        .await
        .expect("the pass");

    assert_eq!(stored(&pool, "labelled").await, (None, None));
    assert_eq!(stored(&pool, "unlabelled").await, (None, None));
    assert_eq!(
        stored(&pool, "by-hand").await,
        (Some(recipe.clone()), Some("user".to_owned())),
        "a person may set the recipe's values by hand"
    );
    assert_eq!(
        stored(&pool, "legacy-tuned").await,
        (Some(tuned.clone()), None)
    );
    assert_eq!(
        stored(&pool, "published").await,
        (Some(tuned), Some("published".to_owned()))
    );
    assert_eq!(
        stored(&pool, "unreadable").await,
        (Some("{not json".to_owned()), None)
    );

    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(version, UNSTORED_RECIPE_SCHEMA_VERSION);
}

/// Once stamped, a copy an older build writes is left alone: the pass is a
/// migration, not a rule every open enforces.
#[tokio::test]
async fn the_pass_runs_once() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let pool = left_by_an_older_build(&dir.path().join("gglib.db")).await;
    drop_stored_reasoning_recipes(&pool)
        .await
        .expect("the pass");

    plant(
        &pool,
        "late",
        Some(&json(&STORED_RECIPE)),
        Some("auto_detected"),
    )
    .await;
    drop_stored_reasoning_recipes(&pool)
        .await
        .expect("a second open");

    assert_eq!(
        stored(&pool, "late").await.1.as_deref(),
        Some("auto_detected")
    );
}

/// A stored copy is the recipe as it was, not as it is now: the constant
/// differs from today's recipe in `max_tokens` alone.
#[test]
fn the_stored_recipe_is_the_old_one() {
    let now = InferenceConfig::reasoning_profile();
    assert_eq!(STORED_RECIPE.max_tokens, Some(8192));
    assert_eq!(
        InferenceConfig {
            max_tokens: now.max_tokens,
            ..STORED_RECIPE
        },
        now
    );
    // And no origin label names it any more.
    assert!("auto_detected".parse::<DefaultsOrigin>().is_err());
}
