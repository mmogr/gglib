//! `SQLite` implementation of [`BenchmarkRepositoryPort`].

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Row, SqlitePool};

use gglib_core::domain::{
    BenchmarkRun, BenchmarkRunStatus, BenchmarkRunType, ModelBenchmarkSummary, ModelCompareResult,
    ModelPerfResult,
};
use gglib_core::ports::{BenchmarkRepositoryPort, RepositoryError};

use super::row_mappers::parse_datetime;

/// `SQLite` implementation of [`BenchmarkRepositoryPort`].
pub struct SqliteBenchmarkRepository {
    pool: SqlitePool,
}

impl SqliteBenchmarkRepository {
    /// Create a new benchmark repository from a shared connection pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create a new in-memory repository (blocking, for tests and stubs).
    ///
    /// # Panics
    ///
    /// Panics if the in-memory SQLite connection cannot be established.
    #[must_use]
    pub fn new_in_memory_blocking() -> Self {
        let pool = tokio::runtime::Handle::try_current()
            .map(|h| {
                h.block_on(SqlitePool::connect("sqlite::memory:"))
                    .expect("in-memory SQLite pool")
            })
            .unwrap_or_else(|_| {
                tokio::runtime::Runtime::new()
                    .expect("tokio runtime")
                    .block_on(SqlitePool::connect("sqlite::memory:"))
                    .expect("in-memory SQLite pool")
            });
        Self { pool }
    }

    /// Reset any benchmark runs that are still in `Running` status to
    /// `Failed` (zombie cleanup on process restart).
    ///
    /// Convenience wrapper around [`crate::setup::cleanup_zombie_benchmark_runs`]
    /// so callers do not need a direct `sqlx` dependency.
    pub async fn cleanup_zombie_runs(&self) -> Result<(), RepositoryError> {
        crate::setup::cleanup_zombie_benchmark_runs(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))
    }
}

// ── Helper: enum ↔ string ────────────────────────────────────────────────────

fn run_type_to_str(t: &BenchmarkRunType) -> &'static str {
    match t {
        BenchmarkRunType::Compare => "compare",
        BenchmarkRunType::Perf => "perf",
        BenchmarkRunType::Tune => "tune",
    }
}

fn str_to_run_type(s: &str) -> BenchmarkRunType {
    match s {
        "perf" => BenchmarkRunType::Perf,
        "tune" => BenchmarkRunType::Tune,
        _ => BenchmarkRunType::Compare,
    }
}

fn str_to_run_status(s: &str) -> BenchmarkRunStatus {
    match s {
        "complete" => BenchmarkRunStatus::Complete,
        "failed" => BenchmarkRunStatus::Failed,
        _ => BenchmarkRunStatus::Running,
    }
}

// ── Row mapping helpers ──────────────────────────────────────────────────────

fn row_to_benchmark_run(row: &sqlx::sqlite::SqliteRow) -> Result<BenchmarkRun, RepositoryError> {
    let model_ids_json: String = row
        .try_get("model_ids")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
    let model_ids: Vec<i64> = serde_json::from_str(&model_ids_json).unwrap_or_default();

    let run_type_str: String = row
        .try_get("run_type")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
    let status_str: String = row
        .try_get("status")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
    let created_at_str: Option<String> = row
        .try_get("created_at")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
    let completed_at_str: Option<String> = row
        .try_get("completed_at")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    Ok(BenchmarkRun {
        id: row
            .try_get("id")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        run_type: str_to_run_type(&run_type_str),
        status: str_to_run_status(&status_str),
        model_ids,
        prompt_text: row.try_get("prompt_text").ok().flatten(),
        system_prompt: row.try_get("system_prompt").ok().flatten(),
        config_json: row.try_get("config_json").ok().flatten(),
        error: row.try_get("error").ok().flatten(),
        created_at: parse_datetime(created_at_str).unwrap_or_else(Utc::now),
        completed_at: parse_datetime(completed_at_str),
    })
}

fn row_to_compare_result(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<ModelCompareResult, RepositoryError> {
    let was_truncated: i64 = row
        .try_get("was_truncated")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
    let created_at_str: Option<String> = row
        .try_get("created_at")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    Ok(ModelCompareResult {
        id: row.try_get::<Option<i64>, _>("id").ok().flatten(),
        model_id: row
            .try_get("model_id")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        run_id: row.try_get("run_id").ok().flatten(),
        prompt_text: row
            .try_get("prompt_text")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        system_prompt: row.try_get("system_prompt").ok().flatten(),
        response_text: row
            .try_get("response_text")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        was_truncated: was_truncated != 0,
        prompt_tokens: row.try_get("prompt_tokens").ok().flatten(),
        completion_tokens: row.try_get("completion_tokens").ok().flatten(),
        prompt_ms: row.try_get("prompt_ms").ok().flatten(),
        generation_ms: row.try_get("generation_ms").ok().flatten(),
        prompt_tps: row.try_get("prompt_tps").ok().flatten(),
        generation_tps: row.try_get("generation_tps").ok().flatten(),
        created_at: parse_datetime(created_at_str).unwrap_or_else(Utc::now),
    })
}

fn row_to_perf_result(row: &sqlx::sqlite::SqliteRow) -> Result<ModelPerfResult, RepositoryError> {
    let created_at_str: Option<String> = row
        .try_get("created_at")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    Ok(ModelPerfResult {
        id: row.try_get::<Option<i64>, _>("id").ok().flatten(),
        model_id: row
            .try_get("model_id")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        run_id: row.try_get("run_id").ok().flatten(),
        pp_tps: row
            .try_get("pp_tps")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        tg_tps: row
            .try_get("tg_tps")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        pp_tokens: row
            .try_get("pp_tokens")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        tg_tokens: row
            .try_get("tg_tokens")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        backend: row.try_get("backend").ok().flatten(),
        ngl: row.try_get("ngl").ok().flatten(),
        context_size: row.try_get("context_size").ok().flatten(),
        repetitions: row
            .try_get("repetitions")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        created_at: parse_datetime(created_at_str).unwrap_or_else(Utc::now),
    })
}

pub(crate) fn row_to_summary(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<ModelBenchmarkSummary, RepositoryError> {
    let last_benchmarked_at_str: Option<String> = row
        .try_get("last_benchmarked_at")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
    let updated_at_str: Option<String> = row
        .try_get("updated_at")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    Ok(ModelBenchmarkSummary {
        model_id: row
            .try_get("model_id")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        best_tg_tps: row.try_get("best_tg_tps").ok().flatten(),
        best_pp_tps: row.try_get("best_pp_tps").ok().flatten(),
        latest_tg_tps: row.try_get("latest_tg_tps").ok().flatten(),
        latest_pp_tps: row.try_get("latest_pp_tps").ok().flatten(),
        latest_backend: row.try_get("latest_backend").ok().flatten(),
        perf_run_count: row.try_get("perf_run_count").unwrap_or(0),
        compare_run_count: row.try_get("compare_run_count").unwrap_or(0),
        last_benchmarked_at: parse_datetime(last_benchmarked_at_str).unwrap_or_else(Utc::now),
        updated_at: parse_datetime(updated_at_str).unwrap_or_else(Utc::now),
    })
}

// ── Trait implementation ─────────────────────────────────────────────────────

#[async_trait]
impl BenchmarkRepositoryPort for SqliteBenchmarkRepository {
    async fn create_run(
        &self,
        run_type: BenchmarkRunType,
        model_ids: &[i64],
        prompt_text: Option<&str>,
        system_prompt: Option<&str>,
        config_json: Option<&str>,
    ) -> Result<i64, RepositoryError> {
        let model_ids_json = serde_json::to_string(model_ids)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;
        let run_type_str = run_type_to_str(&run_type);
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let rec = sqlx::query(
            "INSERT INTO benchmark_runs (run_type, status, model_ids, prompt_text, system_prompt, config_json, created_at)
             VALUES (?, 'running', ?, ?, ?, ?, ?)
             RETURNING id",
        )
        .bind(run_type_str)
        .bind(model_ids_json)
        .bind(prompt_text)
        .bind(system_prompt)
        .bind(config_json)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(rec
            .try_get(0)
            .map_err(|e| RepositoryError::Storage(e.to_string()))?)
    }

    async fn complete_run(&self, run_id: i64) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE benchmark_runs SET status = 'complete', completed_at = datetime('now') WHERE id = ?",
        )
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn fail_run(&self, run_id: i64, error: &str) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE benchmark_runs SET status = 'failed', error = ?, completed_at = datetime('now') WHERE id = ?",
        )
        .bind(error)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn save_compare_result(
        &self,
        result: &ModelCompareResult,
        run_id: i64,
    ) -> Result<i64, RepositoryError> {
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let was_truncated: i64 = if result.was_truncated { 1 } else { 0 };

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        let rec = sqlx::query(
            "INSERT INTO model_compare_results
             (model_id, run_id, prompt_text, system_prompt, response_text, was_truncated,
              prompt_tokens, completion_tokens, prompt_ms, generation_ms, prompt_tps,
              generation_tps, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             RETURNING id",
        )
        .bind(result.model_id)
        .bind(run_id)
        .bind(&result.prompt_text)
        .bind(&result.system_prompt)
        .bind(&result.response_text)
        .bind(was_truncated)
        .bind(result.prompt_tokens)
        .bind(result.completion_tokens)
        .bind(result.prompt_ms)
        .bind(result.generation_ms)
        .bind(result.prompt_tps)
        .bind(result.generation_tps)
        .bind(&now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        let result_id: i64 = rec
            .try_get(0)
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        // Upsert the denormalised summary — increment compare_run_count and
        // refresh last_benchmarked_at.  Perf fields are left untouched.
        sqlx::query(
            "INSERT INTO model_benchmark_summaries
             (model_id, best_tg_tps, best_pp_tps, latest_tg_tps, latest_pp_tps,
              latest_backend, perf_run_count, compare_run_count,
              last_benchmarked_at, updated_at)
             VALUES (?, NULL, NULL, NULL, NULL, NULL, 0, 1, datetime('now'), datetime('now'))
             ON CONFLICT(model_id) DO UPDATE SET
               compare_run_count    = model_benchmark_summaries.compare_run_count + 1,
               last_benchmarked_at  = datetime('now'),
               updated_at           = datetime('now')",
        )
        .bind(result.model_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(result_id)
    }

    async fn save_perf_result(
        &self,
        result: &ModelPerfResult,
        run_id: i64,
    ) -> Result<i64, RepositoryError> {
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        let rec = sqlx::query(
            "INSERT INTO model_perf_results
             (model_id, run_id, pp_tps, tg_tps, pp_tokens, tg_tokens,
              backend, ngl, context_size, repetitions, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             RETURNING id",
        )
        .bind(result.model_id)
        .bind(run_id)
        .bind(result.pp_tps)
        .bind(result.tg_tps)
        .bind(result.pp_tokens)
        .bind(result.tg_tokens)
        .bind(&result.backend)
        .bind(result.ngl)
        .bind(result.context_size)
        .bind(result.repetitions)
        .bind(&now)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        let result_id: i64 = rec
            .try_get(0)
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        // Upsert summary: track best (all-time max) and latest tg/pp throughput.
        // MAX(COALESCE(existing, new), new) handles the NULL-on-first-insert case.
        sqlx::query(
            "INSERT INTO model_benchmark_summaries
             (model_id, best_tg_tps, best_pp_tps, latest_tg_tps, latest_pp_tps,
              latest_backend, perf_run_count, compare_run_count,
              last_benchmarked_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 1, 0, datetime('now'), datetime('now'))
             ON CONFLICT(model_id) DO UPDATE SET
               best_tg_tps         = MAX(COALESCE(model_benchmark_summaries.best_tg_tps, excluded.best_tg_tps), excluded.best_tg_tps),
               best_pp_tps         = MAX(COALESCE(model_benchmark_summaries.best_pp_tps, excluded.best_pp_tps), excluded.best_pp_tps),
               latest_tg_tps       = excluded.latest_tg_tps,
               latest_pp_tps       = excluded.latest_pp_tps,
               latest_backend      = excluded.latest_backend,
               perf_run_count      = model_benchmark_summaries.perf_run_count + 1,
               last_benchmarked_at = datetime('now'),
               updated_at          = datetime('now')",
        )
        .bind(result.model_id)
        .bind(result.tg_tps)
        .bind(result.pp_tps)
        .bind(result.tg_tps)
        .bind(result.pp_tps)
        .bind(&result.backend)
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(result_id)
    }

    async fn list_runs(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<BenchmarkRun>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, run_type, status, model_ids, prompt_text, system_prompt,
                    config_json, error, created_at, completed_at
             FROM benchmark_runs
             ORDER BY created_at DESC
             LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        rows.iter().map(row_to_benchmark_run).collect()
    }

    async fn get_run(&self, run_id: i64) -> Result<Option<BenchmarkRun>, RepositoryError> {
        let row = sqlx::query(
            "SELECT id, run_type, status, model_ids, prompt_text, system_prompt,
                    config_json, error, created_at, completed_at
             FROM benchmark_runs WHERE id = ?",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        row.as_ref().map(row_to_benchmark_run).transpose()
    }

    async fn get_model_compare_history(
        &self,
        model_id: i64,
        limit: i64,
    ) -> Result<Vec<ModelCompareResult>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, model_id, run_id, prompt_text, system_prompt, response_text,
                    was_truncated, prompt_tokens, completion_tokens, prompt_ms,
                    generation_ms, prompt_tps, generation_tps, created_at
             FROM model_compare_results
             WHERE model_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(model_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        rows.iter().map(row_to_compare_result).collect()
    }

    async fn get_model_perf_history(
        &self,
        model_id: i64,
        limit: i64,
    ) -> Result<Vec<ModelPerfResult>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT id, model_id, run_id, pp_tps, tg_tps, pp_tokens, tg_tokens,
                    backend, ngl, context_size, repetitions, created_at
             FROM model_perf_results
             WHERE model_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(model_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        rows.iter().map(row_to_perf_result).collect()
    }

    async fn get_model_summary(
        &self,
        model_id: i64,
    ) -> Result<Option<ModelBenchmarkSummary>, RepositoryError> {
        let row = sqlx::query(
            "SELECT model_id, best_tg_tps, best_pp_tps, latest_tg_tps, latest_pp_tps,
                    latest_backend, perf_run_count, compare_run_count,
                    last_benchmarked_at, updated_at
             FROM model_benchmark_summaries WHERE model_id = ?",
        )
        .bind(model_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        row.as_ref().map(row_to_summary).transpose()
    }
}
