//! Row mapping helpers for `SQLite` queries.

use chrono::{DateTime, NaiveDateTime, Utc};
use gglib_core::domain::benchmark::ModelBenchmarkSummary;
use gglib_core::domain::{DefaultsOrigin, InferenceConfig};
use gglib_core::{Model, ModelCapabilities, RepositoryError};
use sqlx::Row;
use std::path::Path;

/// Shared SELECT column list for model queries (no table alias required).
pub(crate) const MODEL_SELECT_COLUMNS: &str = "id, name, file_path, param_count_b, architecture, quantization, context_length, expert_count, expert_used_count, expert_shared_count, metadata, added_at, hf_repo_id, hf_commit_sha, hf_filename, download_date, last_update_check, tags, capabilities, inference_defaults, defaults_origin, server_defaults, model_key, dialect_spec, template_caps";

/// Additional columns to SELECT when the model query includes a LEFT JOIN
/// with `model_benchmark_summaries s`. All columns are aliased with an `s_`
/// prefix to avoid conflicts and allow defensive parsing in `row_to_model`.
pub(crate) const BENCHMARK_SUMMARY_COLUMNS: &str = "s.model_id AS s_model_id, \
     s.best_tg_tps AS s_best_tg_tps, \
     s.best_pp_tps AS s_best_pp_tps, \
     s.latest_tg_tps AS s_latest_tg_tps, \
     s.latest_pp_tps AS s_latest_pp_tps, \
     s.latest_backend AS s_latest_backend, \
     s.perf_run_count AS s_perf_run_count, \
     s.compare_run_count AS s_compare_run_count, \
     s.last_benchmarked_at AS s_last_benchmarked_at, \
     s.updated_at AS s_updated_at";

/// Helper to parse datetime strings that may have "UTC" suffix.
pub(crate) fn parse_datetime(datetime_str: Option<String>) -> Option<DateTime<Utc>> {
    datetime_str.and_then(|s| {
        let trimmed = s.trim_end_matches(" UTC");
        NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S%.f")
            .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
            .ok()
    })
}

/// Parse a database row into a Model.
pub(crate) fn row_to_model(row: &sqlx::sqlite::SqliteRow) -> Result<Model, RepositoryError> {
    let context_length: Option<u64> = row
        .try_get::<Option<i64>, _>("context_length")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?
        .map(|v| v as u64);

    let metadata_json: String = row
        .try_get("metadata")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    let tags_json: String = row
        .try_get("tags")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    let added_at_str: Option<String> = row
        .try_get("added_at")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    let download_date_str: Option<String> = row
        .try_get("download_date")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    let last_update_check_str: Option<String> = row
        .try_get("last_update_check")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    let inference_defaults: Option<InferenceConfig> = row
        .try_get::<Option<String>, _>("inference_defaults")
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok());
    let defaults_origin = resolve_defaults_origin(
        row.try_get::<Option<String>, _>("defaults_origin")
            .ok()
            .flatten(),
        inference_defaults.as_ref(),
    );

    Ok(Model {
        id: row
            .try_get::<i64, _>("id")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        name: row
            .try_get("name")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        model_key: row
            .try_get("model_key")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        file_path: row
            .try_get::<String, _>("file_path")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?
            .into(),
        param_count_b: row
            .try_get("param_count_b")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        architecture: row
            .try_get("architecture")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        quantization: row
            .try_get("quantization")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        context_length,
        expert_count: row.try_get::<Option<u32>, _>("expert_count").ok().flatten(),
        expert_used_count: row
            .try_get::<Option<u32>, _>("expert_used_count")
            .ok()
            .flatten(),
        expert_shared_count: row
            .try_get::<Option<u32>, _>("expert_shared_count")
            .ok()
            .flatten(),
        metadata: serde_json::from_str(&metadata_json).unwrap_or_default(),
        added_at: parse_datetime(added_at_str).unwrap_or_else(Utc::now),
        hf_repo_id: row
            .try_get("hf_repo_id")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        hf_commit_sha: row
            .try_get("hf_commit_sha")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        hf_filename: row
            .try_get("hf_filename")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        download_date: parse_datetime(download_date_str),
        last_update_check: parse_datetime(last_update_check_str),
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        capabilities: row
            .try_get::<u32, _>("capabilities")
            .ok()
            .map(ModelCapabilities::from_bits_truncate)
            .unwrap_or_default(),
        inference_defaults,
        defaults_origin,
        server_defaults: row
            .try_get::<Option<String>, _>("server_defaults")
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok()),
        // Tolerant parse, like inference_defaults: a NULL column (legacy
        // row) or unreadable JSON reads as "no spec" and the tag fallback
        // applies downstream.
        dialect_spec: row
            .try_get::<Option<String>, _>("dialect_spec")
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok()),
        // Same tolerant parse: a NULL column is "never observed" (ADR 0007's
        // third state), and unreadable JSON degrades to the same answer
        // rather than failing the whole row.
        template_caps: row
            .try_get::<Option<String>, _>("template_caps")
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok()),
        // Defensively attempt to read benchmark summary columns (only present
        // when the query includes a LEFT JOIN with model_benchmark_summaries).
        benchmark_summary: try_read_summary(row),
    })
}

/// Resolve a model's `defaults_origin`, backfilling rows written before the
/// column existed.
///
/// `stored` is whatever is actually in the `defaults_origin` column,
/// unparsed. When it names a known origin, that's the answer — no
/// backfill needed. When it's absent (every row written before this column
/// existed; there is no batch migration for them, deliberately — see
/// `crates/gglib-db/src/setup.rs`'s `defaults_origin` `ALTER TABLE` comment)
/// this derives one from `inference_defaults` itself: gglib has only ever
/// auto-written one exact recipe
/// ([`InferenceConfig::reasoning_profile`]), so a stored value that matches
/// it precisely is that guess; anything else is something a person set.
///
/// Always `None` when there is no `inference_defaults` to have an origin at
/// all, regardless of what the column says — a stale/orphaned value there
/// would be meaningless.
fn resolve_defaults_origin(
    stored: Option<String>,
    inference_defaults: Option<&InferenceConfig>,
) -> Option<DefaultsOrigin> {
    let inference_defaults = inference_defaults?;
    if let Some(origin) = stored.and_then(|s| s.parse::<DefaultsOrigin>().ok()) {
        return Some(origin);
    }
    if *inference_defaults == InferenceConfig::reasoning_profile() {
        Some(DefaultsOrigin::AutoDetected)
    } else {
        Some(DefaultsOrigin::User)
    }
}

/// Try to read benchmark summary columns from a row.
///
/// Returns `None` if the `s_model_id` column is absent (query has no JOIN) or
/// if the joined row was NULL (model has no benchmark data yet).
fn try_read_summary(row: &sqlx::sqlite::SqliteRow) -> Option<ModelBenchmarkSummary> {
    // s_model_id is the sentinel: absent means no JOIN was present;
    // NULL means LEFT JOIN found no matching summary row.
    let model_id: i64 = row.try_get("s_model_id").ok().flatten()?;

    let last_benchmarked_at_str: Option<String> =
        row.try_get("s_last_benchmarked_at").ok().flatten();
    let updated_at_str: Option<String> = row.try_get("s_updated_at").ok().flatten();

    Some(ModelBenchmarkSummary {
        model_id,
        best_tg_tps: row.try_get("s_best_tg_tps").ok().flatten(),
        best_pp_tps: row.try_get("s_best_pp_tps").ok().flatten(),
        latest_tg_tps: row.try_get("s_latest_tg_tps").ok().flatten(),
        latest_pp_tps: row.try_get("s_latest_pp_tps").ok().flatten(),
        latest_backend: row.try_get("s_latest_backend").ok().flatten(),
        perf_run_count: row.try_get("s_perf_run_count").ok().flatten().unwrap_or(0),
        compare_run_count: row
            .try_get("s_compare_run_count")
            .ok()
            .flatten()
            .unwrap_or(0),
        last_benchmarked_at: parse_datetime(last_benchmarked_at_str).unwrap_or_else(Utc::now),
        updated_at: parse_datetime(updated_at_str).unwrap_or_else(Utc::now),
    })
}

/// Normalizes a file path to a canonical string representation.
///
/// Delegates so that the stored column, the `model_key` derived from it and
/// the duplicate lookup that reads it all share one definition of "the same
/// file"; see [`gglib_core::paths::canonical_model_path`].
pub(crate) fn normalized_file_path_string(path: &Path) -> String {
    gglib_core::paths::canonical_model_path_string(path)
}

/// Parse a database row into a ModelFile.
pub(crate) fn map_model_file_row(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<gglib_core::domain::ModelFile, sqlx::Error> {
    Ok(gglib_core::domain::ModelFile {
        id: row.try_get("id")?,
        model_id: row.try_get("model_id")?,
        file_path: row.try_get("file_path")?,
        file_index: row.try_get("file_index")?,
        expected_size: row.try_get("expected_size")?,
        hf_oid: row.try_get("hf_oid")?,
        last_verified_at: row
            .try_get::<Option<String>, _>("last_verified_at")?
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
    })
}

#[cfg(test)]
mod defaults_origin_tests {
    use super::*;

    #[test]
    fn stored_value_wins_when_present() {
        let origin = resolve_defaults_origin(
            Some("user".to_owned()),
            Some(&InferenceConfig::reasoning_profile()),
        );
        assert_eq!(
            origin,
            Some(DefaultsOrigin::User),
            "explicit column value must not be second-guessed, even though \
             this inference_defaults matches the auto-detected recipe \
             exactly — a user is free to set the same values by hand"
        );
    }

    #[test]
    fn legacy_row_matching_the_reasoning_recipe_backfills_to_auto_detected() {
        let origin = resolve_defaults_origin(None, Some(&InferenceConfig::reasoning_profile()));
        assert_eq!(origin, Some(DefaultsOrigin::AutoDetected));
    }

    /// The measured origin round-trips through the same TEXT column with no
    /// schema change — `Display` writes `"measured"`, `FromStr` reads it, and
    /// the legacy backfill never manufactures it: a `Measured` row is always
    /// explicitly written by an apply, so an unlabelled row can only be a
    /// guess or a person's work.
    #[test]
    fn a_measured_origin_round_trips_and_is_never_backfilled() {
        let origin = resolve_defaults_origin(
            Some(DefaultsOrigin::Measured.to_string()),
            Some(&InferenceConfig::reasoning_profile()),
        );
        assert_eq!(origin, Some(DefaultsOrigin::Measured));

        // A legacy NULL beside any recipe backfills to a guess or to user —
        // never to measured.
        let backfilled = resolve_defaults_origin(None, Some(&InferenceConfig::reasoning_profile()));
        assert_ne!(backfilled, Some(DefaultsOrigin::Measured));
    }

    #[test]
    fn legacy_row_not_matching_the_reasoning_recipe_backfills_to_user() {
        let custom = InferenceConfig {
            temperature: Some(0.3),
            ..Default::default()
        };
        let origin = resolve_defaults_origin(None, Some(&custom));
        assert_eq!(origin, Some(DefaultsOrigin::User));
    }

    #[test]
    fn no_inference_defaults_means_no_origin_regardless_of_the_column() {
        assert_eq!(resolve_defaults_origin(Some("user".to_owned()), None), None);
        assert_eq!(resolve_defaults_origin(None, None), None);
    }

    #[test]
    fn unparseable_stored_value_falls_back_to_the_recipe_match() {
        // A column value from some future, unrecognised variant must not
        // panic or silently become `None` — it falls through to the same
        // backfill a legacy NULL would get.
        let origin = resolve_defaults_origin(
            Some("not_a_real_variant".to_owned()),
            Some(&InferenceConfig::reasoning_profile()),
        );
        assert_eq!(origin, Some(DefaultsOrigin::AutoDetected));
    }
}
