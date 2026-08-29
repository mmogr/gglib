//! `SQLite` implementation of [`BenchmarkRepositoryPort`].

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Row, SqlitePool};

use gglib_core::domain::benchmark::agentic::AgenticEvalReport;
use gglib_core::domain::{
    BenchmarkRun, BenchmarkRunStatus, BenchmarkRunType, ModelBenchmarkSummary, ModelCompareResult,
    ModelPerfResult, TuneCandidateResult,
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
}

// ── Helper: enum ↔ string ────────────────────────────────────────────────────

fn run_type_to_str(t: &BenchmarkRunType) -> &'static str {
    match t {
        BenchmarkRunType::Compare => "compare",
        BenchmarkRunType::Perf => "perf",
        BenchmarkRunType::Tune => "tune",
        BenchmarkRunType::Agentic => "agentic",
    }
}

fn str_to_run_type(s: &str) -> BenchmarkRunType {
    match s {
        "perf" => BenchmarkRunType::Perf,
        "tune" => BenchmarkRunType::Tune,
        "agentic" => BenchmarkRunType::Agentic,
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
        applied_json: row.try_get("applied_json").ok().flatten(),
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

fn row_to_tune_result(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<TuneCandidateResult, RepositoryError> {
    let config_json: String = row
        .try_get("config_json")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
    let source_json: String = row
        .try_get("source_json")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
    let task_results_json: String = row
        .try_get("task_results_json")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
    let pruned: i64 = row
        .try_get("pruned")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

    Ok(TuneCandidateResult {
        config: serde_json::from_str(&config_json)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?,
        source: serde_json::from_str(&source_json)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?,
        task_results: serde_json::from_str(&task_results_json)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?,
        composite_score: row
            .try_get("composite_score")
            .map_err(|e| RepositoryError::Storage(e.to_string()))?,
        pruned: pruned != 0,
        tg_tps: row.try_get("tg_tps").ok().flatten(),
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
                    config_json, applied_json, error, created_at, completed_at
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
                    config_json, applied_json, error, created_at, completed_at
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

    async fn save_tune_result(
        &self,
        result: &TuneCandidateResult,
        run_id: i64,
        model_id: i64,
    ) -> Result<i64, RepositoryError> {
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let config_json = serde_json::to_string(&result.config)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;
        let source_json = serde_json::to_string(&result.source)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;
        let task_results_json = serde_json::to_string(&result.task_results)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;
        let pruned: i64 = if result.pruned { 1 } else { 0 };

        let rec = sqlx::query(
            "INSERT INTO benchmark_tune_results
             (model_id, run_id, config_json, source_json, composite_score, pruned,
              tg_tps, task_results_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             RETURNING id",
        )
        .bind(model_id)
        .bind(run_id)
        .bind(config_json)
        .bind(source_json)
        .bind(result.composite_score)
        .bind(pruned)
        .bind(result.tg_tps)
        .bind(task_results_json)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(rec
            .try_get(0)
            .map_err(|e| RepositoryError::Storage(e.to_string()))?)
    }

    async fn get_model_tune_history(
        &self,
        model_id: i64,
        limit: i64,
    ) -> Result<Vec<TuneCandidateResult>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT config_json, source_json, composite_score, pruned, tg_tps, task_results_json
             FROM benchmark_tune_results
             WHERE model_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(model_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        rows.iter().map(row_to_tune_result).collect()
    }

    async fn get_tune_results(
        &self,
        run_id: i64,
    ) -> Result<Vec<TuneCandidateResult>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT config_json, source_json, composite_score, pruned, tg_tps, task_results_json
             FROM benchmark_tune_results
             WHERE run_id = ?
             ORDER BY id ASC",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        rows.iter().map(row_to_tune_result).collect()
    }

    async fn mark_run_applied(
        &self,
        run_id: i64,
        applied_json: &str,
    ) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE benchmark_runs SET applied_json = ? WHERE id = ?")
            .bind(applied_json)
            .bind(run_id)
            .execute(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn save_agentic_result(
        &self,
        report: &AgenticEvalReport,
        run_id: i64,
        model_id: i64,
    ) -> Result<i64, RepositoryError> {
        let now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let report_json = serde_json::to_string(report)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;

        let rec = sqlx::query(
            "INSERT INTO benchmark_agentic_results
             (model_id, run_id, raw_composite, gglib_composite, report_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?)
             RETURNING id",
        )
        .bind(model_id)
        .bind(run_id)
        .bind(report.raw.composite)
        .bind(report.gglib.composite)
        .bind(report_json)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(rec
            .try_get(0)
            .map_err(|e| RepositoryError::Storage(e.to_string()))?)
    }

    async fn get_model_agentic_history(
        &self,
        model_id: i64,
        limit: i64,
    ) -> Result<Vec<AgenticEvalReport>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT report_json
             FROM benchmark_agentic_results
             WHERE model_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(model_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        rows.iter().map(row_to_agentic_report).collect()
    }
}

/// Rebuild an [`AgenticEvalReport`] from its stored JSON blob.
fn row_to_agentic_report(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<AgenticEvalReport, RepositoryError> {
    let json: String = row
        .try_get("report_json")
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
    serde_json::from_str(&json).map_err(|e| RepositoryError::Serialization(e.to_string()))
}

#[cfg(test)]
mod tests {
    use gglib_core::domain::benchmark::agentic::{ArmDelta, ArmScores};

    use crate::setup::setup_test_database;

    use super::*;
    use gglib_core::domain::benchmark::{DEFAULT_SEEDS, replicate_seeds};

    fn arm(composite: f64, loop_avoidance: Option<f64>) -> ArmScores {
        ArmScores {
            tool_accuracy: 0.722,
            loop_avoidance,
            loop_eligible: usize::from(loop_avoidance.is_some()),
            task_completion: 0.667,
            composite,
            tg_tps: Some(205.3),
            total_completion_tokens: Some(226_768),
            total_wall_ms: 1_104_543,
            measured_wall_ms: 1_104_543,
            mean_time_to_first_tool_call_ms: Some(2_100.0),
            seeds: 3,
            runs: 9,
            unmeasured_runs: 0,
            transport_retries: 0,
        }
    }

    fn report() -> AgenticEvalReport {
        let raw = arm(0.802, None);
        let gglib = arm(0.802, Some(1.0));
        AgenticEvalReport {
            model_name: "qwen2.5-0.5b-instruct".to_owned(),
            quantization: Some("Q8_0".to_owned()),
            param_count_b: 0.5,
            ctx_size: 131_072,
            delta: ArmDelta {
                tool_accuracy: Some(0.0),
                loop_avoidance: None,
                task_completion: Some(0.0),
                composite: Some(0.0),
                wall_time_speedup: Some(229.83),
                completion_token_ratio: Some(4_627.92),
                withheld: None,
            },
            raw,
            gglib,
            tasks: vec![],
            seeds: DEFAULT_SEEDS.to_vec(),
            // The control runs fewer seeds than the arms it validates, and a
            // fixture that gave it the same count would not round-trip the
            // per-arm sample size this table is supposed to preserve.
            control: Some(ArmScores {
                seeds: 1,
                runs: 3,
                ..arm(0.60, Some(1.0))
            }),
            raw_replicate: Some(arm(0.780, None)),
            replicate_seeds: replicate_seeds(&DEFAULT_SEEDS),
            raw_replicates: vec![arm(0.780, None)],
            replicate_seed_sets: vec![replicate_seeds(&DEFAULT_SEEDS)],
            paired: None,
        }
    }

    async fn repo() -> SqliteBenchmarkRepository {
        let pool = setup_test_database().await.expect("setup_test_database");
        sqlx::query(
            "INSERT INTO models (id, name, file_path, param_count_b, added_at, model_key)
             VALUES (1, 'm', '/tmp/m.gguf', 0.5, '2026-01-01 00:00:00', 'm')",
        )
        .execute(&pool)
        .await
        .expect("seed model");
        SqliteBenchmarkRepository::new(pool)
    }

    /// The apply record must survive both read paths. The row mapper reads
    /// `applied_json` with a forgiving `try_get(..).ok()`, so a SELECT that
    /// omits the column does not error — it silently reads back `None`, and
    /// every Outcome surface renders an em-dash. This is the regression
    /// test that makes that omission loud.
    #[tokio::test]
    async fn applied_json_survives_both_read_paths() {
        let repo = repo().await;
        let run_id = repo
            .create_run(BenchmarkRunType::Tune, &[1], None, None, None)
            .await
            .expect("create run");
        repo.mark_run_applied(run_id, r#"{"verdict":{"verdict":"uncalibrated"}}"#)
            .await
            .expect("mark applied");

        let listed = repo.list_runs(10, 0).await.expect("list");
        assert_eq!(
            listed[0].applied_json.as_deref(),
            Some(r#"{"verdict":{"verdict":"uncalibrated"}}"#),
            "list_runs must select applied_json — the activity surfaces read it from here"
        );

        let fetched = repo.get_run(run_id).await.expect("get").expect("exists");
        assert_eq!(
            fetched.applied_json.as_deref(),
            Some(r#"{"verdict":{"verdict":"uncalibrated"}}"#),
            "get_run must select applied_json — the revert path reads it from here"
        );
    }

    /// The report is stored whole, so every field a leaderboard reads back —
    /// including the ones that distinguish "unmeasured" from "zero" — has to
    /// survive the round trip.
    #[tokio::test]
    async fn an_agentic_report_round_trips() {
        let repo = repo().await;
        let run_id = repo
            .create_run(BenchmarkRunType::Agentic, &[1], None, None, None)
            .await
            .expect("create_run");
        repo.save_agentic_result(&report(), run_id, 1)
            .await
            .expect("save_agentic_result");

        let history = repo
            .get_model_agentic_history(1, 10)
            .await
            .expect("get_model_agentic_history");

        assert_eq!(history.len(), 1);
        let got = &history[0];
        assert_eq!(got.model_name, "qwen2.5-0.5b-instruct");
        assert_eq!(got.raw.loop_avoidance, None, "unmeasured must stay absent");
        assert_eq!(got.gglib.loop_avoidance, Some(1.0));
        assert_eq!(got.raw.total_completion_tokens, Some(226_768));
        assert!((got.delta.wall_time_speedup.unwrap() - 229.83).abs() < 1e-9);
        // The two calibration arms and their sample sizes. A leaderboard that
        // read a stored delta without them would be quoting a magnitude with
        // nothing behind it — the exact reading the arms were added to prevent.
        assert!((got.noise_floor().expect("A/A survived") - 0.022).abs() < 1e-9);
        assert_eq!(got.replicate_seeds, replicate_seeds(&DEFAULT_SEEDS));
        assert_eq!(
            got.control.as_ref().expect("control survived").seeds,
            1,
            "each arm's own seed count, not the run's"
        );
    }

    /// The run row must come back typed as agentic, not silently fall through
    /// the `str_to_run_type` default to `compare`.
    #[tokio::test]
    async fn an_agentic_run_keeps_its_type() {
        let repo = repo().await;
        let run_id = repo
            .create_run(BenchmarkRunType::Agentic, &[1], None, None, None)
            .await
            .expect("create_run");

        let run = repo.get_run(run_id).await.expect("get_run").expect("some");
        assert_eq!(run.run_type, BenchmarkRunType::Agentic);
    }
}
