//! The `reasoning` recipe leaves the `models` rows it was written to.
//!
//! A `#[path]` child of `setup.rs`. Until ADR 0022 the import wrote
//! `InferenceConfig::reasoning_profile()` into a `reasoning`-tagged model's
//! `inference_defaults`, and any edit of that model's defaults replaced it, so
//! clearing them left the model with no recipe at all (#1428). Resolution now
//! supplies the recipe from the tag and the import stores none, so a stored
//! copy is a setting left behind: it would hold the old `max_tokens: 8192`
//! and shadow the recipe as it is now.

use anyhow::Result;
use gglib_core::domain::InferenceConfig;
use sqlx::{SqliteConnection, SqliteExecutor, SqlitePool};

/// `PRAGMA user_version` once stored copies of the `reasoning` recipe are
/// gone.
///
/// The third version this project has assigned, above
/// [`LOOP_GUARD_MODE_SCHEMA_VERSION`](super::settings::LOOP_GUARD_MODE_SCHEMA_VERSION),
/// which goes on meaning what it meant; anything later takes a higher number
/// still.
pub(super) const UNSTORED_RECIPE_SCHEMA_VERSION: i64 = 3;

/// The recipe the import wrote before ADR 0022, value for value.
///
/// Written out rather than read from `reasoning_profile()`, which has since
/// moved (`max_tokens` 8192 to 32768): a row is a stored copy only if it
/// equals what was stored.
const STORED_RECIPE: InferenceConfig = InferenceConfig {
    temperature: Some(1.0),
    top_p: Some(0.95),
    top_k: Some(20),
    max_tokens: Some(8192),
    repeat_penalty: Some(1.0),
    presence_penalty: Some(1.5),
    min_p: Some(0.0),
    seed: None,
    frequency_penalty: None,
    dynatemp_range: None,
    dynatemp_exponent: None,
    top_n_sigma: None,
    dry_multiplier: None,
    dry_base: None,
    dry_allowed_length: None,
    dry_penalty_last_n: None,
    reasoning_effort: None,
    reasoning_budget_tokens: None,
};

/// Migration: clear every stored copy of the `reasoning` recipe, once per
/// database.
///
/// Two kinds of row hold one: a row labelled `auto_detected`, and a row from
/// before the label existed whose defaults equal the recipe exactly, which
/// the read path used to label `auto_detected` by that comparison. Both lose
/// their `inference_defaults` and `defaults_origin`. A row a person or a tune
/// sweep wrote keeps its values whatever they are.
///
/// Gated on `user_version` and run in one `BEGIN IMMEDIATE` transaction, as
/// the loop guard's fold is; run after that fold, so a library stamped below
/// it gets both.
pub(super) async fn drop_stored_reasoning_recipes(pool: &SqlitePool) -> Result<()> {
    if user_version(pool).await? >= UNSTORED_RECIPE_SCHEMA_VERSION {
        return Ok(());
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if user_version(&mut *tx).await? >= UNSTORED_RECIPE_SCHEMA_VERSION {
        return Ok(());
    }
    clear(&mut tx).await?;
    // Not bindable as a parameter: SQLite requires a literal here.
    sqlx::query(&format!(
        "PRAGMA user_version = {UNSTORED_RECIPE_SCHEMA_VERSION}"
    ))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Clear both kinds of stored copy. A row whose defaults cannot be read is
/// left as found: it is not a copy anybody can recognise.
async fn clear(conn: &mut SqliteConnection) -> Result<()> {
    let labelled = sqlx::query(
        "UPDATE models SET inference_defaults = NULL, defaults_origin = NULL \
         WHERE defaults_origin = 'auto_detected'",
    )
    .execute(&mut *conn)
    .await?
    .rows_affected();

    let unlabelled: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, inference_defaults FROM models \
         WHERE defaults_origin IS NULL AND inference_defaults IS NOT NULL",
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut matched = 0_u64;
    for (id, json) in unlabelled {
        let stored: Option<InferenceConfig> = serde_json::from_str(&json).ok();
        if stored.as_ref() == Some(&STORED_RECIPE) {
            sqlx::query("UPDATE models SET inference_defaults = NULL WHERE id = ?")
                .bind(id)
                .execute(&mut *conn)
                .await?;
            matched += 1;
        }
    }

    if labelled + matched > 0 {
        tracing::info!(
            cleared = labelled + matched,
            "cleared stored copies of the reasoning recipe"
        );
    }
    Ok(())
}

async fn user_version(conn: impl SqliteExecutor<'_>) -> sqlx::Result<i64> {
    sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(conn)
        .await
}

#[cfg(test)]
#[path = "setup_recipes_tests.rs"]
mod setup_recipes_tests;
