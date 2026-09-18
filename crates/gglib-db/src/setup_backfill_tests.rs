//! Tests for the model-key backfill: a row stored under a spelling of its
//! path that is not the canonical one is re-keyed and repaired at boot.
//!
//! Split from `setup_tests.rs` by subject — the two files together are 399
//! lines, over the 300 a new file may be. Declared as `mod backfill_tests`,
//! so these two tests moved from `setup::tests::` to
//! `setup::backfill_tests::`.

use super::*;

/// A row written by a build that hashed the path as the user spelled it
/// must be re-keyed onto the canonical rule, or it stays unreachable by
/// `ON CONFLICT(model_key)` and the next registration of that file
/// silently appends a second row.
///
/// The non-canonical spelling is built with `..` rather than borrowed from
/// the platform. A `tempfile` directory is non-canonical on macOS, where
/// `/var` resolves through `/private/var`, and already canonical on Linux
/// — so leaning on it wrote a test that passed on the machine it was
/// written on and failed in CI. `..` survives as a real path component
/// everywhere, which is what makes the two spellings differ on every
/// platform.
#[tokio::test]
async fn backfill_rekeys_a_row_stored_under_a_non_canonical_spelling() {
    use crate::repositories::sqlite_model_repository::local_model_key_for;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let pool = setup_test_database().await.unwrap();

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("Legacy.gguf");
    std::fs::File::create(&file).unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();

    let spelled = dir.path().join("sub").join("..").join("Legacy.gguf");
    let canonical = std::fs::canonicalize(&file).unwrap();
    assert_ne!(
        spelled, canonical,
        "this test needs the two spellings to differ"
    );

    // Exactly what a previous build wrote: canonical column, key hashed
    // from the path it was handed.
    let legacy_key = {
        let mut hasher = DefaultHasher::new();
        spelled.hash(&mut hasher);
        format!("local:{:x}", hasher.finish())
    };
    sqlx::query(
        "INSERT INTO models (name, file_path, param_count_b, added_at, model_key) \
             VALUES (?, ?, ?, ?, ?)",
    )
    .bind("Legacy")
    .bind(canonical.to_string_lossy().as_ref())
    .bind(7.0_f64)
    .bind(chrono::Utc::now().to_string())
    .bind(&legacy_key)
    .execute(&pool)
    .await
    .unwrap();

    backfill_local_model_keys(&pool).await.unwrap();

    let key: String = sqlx::query_scalar("SELECT model_key FROM models WHERE name = 'Legacy'")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(
        key,
        local_model_key_for(canonical.to_string_lossy().as_ref()),
        "the stranded row must be re-keyed onto the rule this build computes"
    );
    assert_ne!(key, legacy_key, "the key must actually have moved");

    // Idempotent: a second pass is a no-op.
    backfill_local_model_keys(&pool).await.unwrap();
    let again: String = sqlx::query_scalar("SELECT model_key FROM models WHERE name = 'Legacy'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(again, key);
}

/// A row whose stored `file_path` is itself non-canonical must be repaired,
/// not merely re-keyed from the bad value.
///
/// `insert` normalised the column but `update` did not, so any row that
/// went through `PATCH /api/models/{id}` holds whatever spelling the caller
/// sent — and `insert`'s own normalisation falls back to the literal path
/// when the file is missing. Hashing the column verbatim would compute a
/// key from that non-canonical string and leave the row exactly as
/// unreachable as before, while reporting success.
#[tokio::test]
async fn backfill_repairs_a_row_whose_stored_path_is_not_canonical() {
    use crate::repositories::sqlite_model_repository::local_model_key_for;

    let pool = setup_test_database().await.unwrap();

    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("Patched.gguf");
    std::fs::File::create(&file).unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();

    // What pre-fix `update` left behind: a spelling only the filesystem
    // can equate with the real path.
    let non_canonical = dir.path().join("sub").join("..").join("Patched.gguf");
    let canonical = std::fs::canonicalize(&file).unwrap();
    assert_ne!(
        non_canonical.to_string_lossy(),
        canonical.to_string_lossy(),
        "the fixture must actually be non-canonical"
    );

    sqlx::query(
        "INSERT INTO models (name, file_path, param_count_b, added_at, model_key) \
             VALUES (?, ?, ?, ?, ?)",
    )
    .bind("Patched")
    .bind(non_canonical.to_string_lossy().as_ref())
    .bind(7.0_f64)
    .bind(chrono::Utc::now().to_string())
    .bind(local_model_key_for(
        non_canonical.to_string_lossy().as_ref(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    backfill_local_model_keys(&pool).await.unwrap();

    let (stored_path, stored_key): (String, String) =
        sqlx::query_as("SELECT file_path, model_key FROM models WHERE name = 'Patched'")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(
        stored_path,
        canonical.to_string_lossy(),
        "the column itself must be repaired, or find_by_path still misses it"
    );
    assert_eq!(
        stored_key,
        local_model_key_for(canonical.to_string_lossy().as_ref()),
        "the key must be the one a fresh `model add` of this file computes"
    );
}
