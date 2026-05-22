//! `SQLite` implementation of the [`OrchestratorRepositoryPort`] trait.

use async_trait::async_trait;
use sqlx::{Row, SqlitePool};

use gglib_core::domain::orchestrator::run::{
    OrchestratorRun, OrchestratorRunEvent, OrchestratorRunStatus,
};
use gglib_core::ports::{OrchestratorRepositoryPort, RepositoryError};

/// `SQLite` implementation of [`OrchestratorRepositoryPort`].
pub struct SqliteOrchestratorRepository {
    pool: SqlitePool,
}

impl SqliteOrchestratorRepository {
    /// Create a new repository from a shared connection pool.
    #[must_use]
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
}

// ---------------------------------------------------------------------------
// Helper: status ↔ string
// ---------------------------------------------------------------------------

fn status_to_str(status: &OrchestratorRunStatus) -> &'static str {
    match status {
        OrchestratorRunStatus::Running => "running",
        OrchestratorRunStatus::AwaitingApproval => "awaiting_approval",
        OrchestratorRunStatus::Interrupted => "interrupted",
        OrchestratorRunStatus::Completed => "completed",
        OrchestratorRunStatus::Failed => "failed",
    }
}

fn str_to_status(s: &str) -> OrchestratorRunStatus {
    match s {
        "awaiting_approval" => OrchestratorRunStatus::AwaitingApproval,
        "interrupted" => OrchestratorRunStatus::Interrupted,
        "completed" => OrchestratorRunStatus::Completed,
        "failed" => OrchestratorRunStatus::Failed,
        _ => OrchestratorRunStatus::Running,
    }
}

// ---------------------------------------------------------------------------
// Impl
// ---------------------------------------------------------------------------

#[async_trait]
impl OrchestratorRepositoryPort for SqliteOrchestratorRepository {
    async fn create_run(&self, run: OrchestratorRun) -> Result<(), RepositoryError> {
        let hitl_mode_json = serde_json::to_string(&run.hitl_mode)
            .map_err(|e| RepositoryError::Serialization(e.to_string()))?;
        sqlx::query(
            r#"
            INSERT INTO orchestrator_runs (id, goal, graph_json, status, hitl_mode, conversation_id, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&run.id)
        .bind(&run.goal)
        .bind(&run.graph_json)
        .bind(status_to_str(&run.status))
        .bind(&hitl_mode_json)
        .bind(run.conversation_id)
        .bind(&run.created_at)
        .bind(&run.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn update_run_status(
        &self,
        run_id: &str,
        status: OrchestratorRunStatus,
    ) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE orchestrator_runs SET status = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(status_to_str(&status))
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn update_graph(&self, run_id: &str, graph_json: &str) -> Result<(), RepositoryError> {
        sqlx::query(
            "UPDATE orchestrator_runs SET graph_json = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(graph_json)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn append_event(&self, event: OrchestratorRunEvent) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO orchestrator_events (run_id, seq, event_json, created_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(&event.run_id)
        .bind(event.seq)
        .bind(&event.event_json)
        .bind(&event.created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn get_run(&self, run_id: &str) -> Result<Option<OrchestratorRun>, RepositoryError> {
        let row = sqlx::query(
            r#"
            SELECT id, goal, graph_json, status, hitl_mode, conversation_id, created_at, updated_at
            FROM orchestrator_runs
            WHERE id = ?
            "#,
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(row.map(|r| {
            let hitl_mode_str: String = r.get("hitl_mode");
            let hitl_mode = serde_json::from_str(&hitl_mode_str).unwrap_or_default();
            let status_str: String = r.get("status");
            OrchestratorRun {
                id: r.get("id"),
                goal: r.get("goal"),
                graph_json: r.get("graph_json"),
                status: str_to_status(&status_str),
                hitl_mode,
                conversation_id: r.get("conversation_id"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            }
        }))
    }

    async fn list_runs(
        &self,
        status_filter: Option<OrchestratorRunStatus>,
    ) -> Result<Vec<OrchestratorRun>, RepositoryError> {
        let rows = if let Some(status) = status_filter {
            sqlx::query(
                r#"
                SELECT id, goal, graph_json, status, hitl_mode, conversation_id, created_at, updated_at
                FROM orchestrator_runs
                WHERE status = ?
                ORDER BY created_at DESC
                "#,
            )
            .bind(status_to_str(&status))
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?
        } else {
            sqlx::query(
                r#"
                SELECT id, goal, graph_json, status, hitl_mode, conversation_id, created_at, updated_at
                FROM orchestrator_runs
                ORDER BY created_at DESC
                "#,
            )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?
        };

        let runs = rows
            .iter()
            .map(|r| {
                let hitl_mode_str: String = r.get("hitl_mode");
                let hitl_mode = serde_json::from_str(&hitl_mode_str).unwrap_or_default();
                let status_str: String = r.get("status");
                OrchestratorRun {
                    id: r.get("id"),
                    goal: r.get("goal"),
                    graph_json: r.get("graph_json"),
                    status: str_to_status(&status_str),
                    hitl_mode,
                    conversation_id: r.get("conversation_id"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                }
            })
            .collect();

        Ok(runs)
    }

    async fn list_events(
        &self,
        run_id: &str,
    ) -> Result<Vec<OrchestratorRunEvent>, RepositoryError> {
        let rows = sqlx::query(
            r#"
            SELECT run_id, seq, event_json, created_at
            FROM orchestrator_events
            WHERE run_id = ?
            ORDER BY seq ASC
            "#,
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        let events = rows
            .iter()
            .map(|r| OrchestratorRunEvent {
                run_id: r.get("run_id"),
                seq: r.get("seq"),
                event_json: r.get("event_json"),
                created_at: r.get("created_at"),
            })
            .collect();

        Ok(events)
    }

    async fn mark_interrupted_runs(&self) -> Result<u64, RepositoryError> {
        let result = sqlx::query(
            "UPDATE orchestrator_runs SET status = 'interrupted', updated_at = datetime('now') WHERE status = 'running'",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;
        Ok(result.rows_affected())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(any(test, feature = "test-utils"))]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::setup_test_database;
    use gglib_core::domain::orchestrator::task_graph::HitlMode;

    async fn make_repo() -> SqliteOrchestratorRepository {
        let pool = setup_test_database().await.unwrap();
        SqliteOrchestratorRepository::new(pool)
    }

    fn make_run(id: &str) -> OrchestratorRun {
        OrchestratorRun {
            id: id.to_string(),
            goal: "test goal".to_string(),
            graph_json: None,
            status: OrchestratorRunStatus::Running,
            hitl_mode: HitlMode::None,
            conversation_id: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[tokio::test]
    async fn create_and_get_run() {
        let repo = make_repo().await;
        let run = make_run("run-1");
        repo.create_run(run.clone()).await.unwrap();
        let got = repo.get_run("run-1").await.unwrap().unwrap();
        assert_eq!(got.id, "run-1");
        assert_eq!(got.goal, "test goal");
        assert!(matches!(got.status, OrchestratorRunStatus::Running));
    }

    #[tokio::test]
    async fn update_run_status() {
        let repo = make_repo().await;
        repo.create_run(make_run("run-2")).await.unwrap();
        repo.update_run_status("run-2", OrchestratorRunStatus::Completed)
            .await
            .unwrap();
        let got = repo.get_run("run-2").await.unwrap().unwrap();
        assert!(matches!(got.status, OrchestratorRunStatus::Completed));
    }

    #[tokio::test]
    async fn append_and_list_events() {
        let repo = make_repo().await;
        repo.create_run(make_run("run-3")).await.unwrap();
        for i in 0..3i64 {
            repo.append_event(OrchestratorRunEvent {
                run_id: "run-3".to_string(),
                seq: i,
                event_json: format!(r#"{{"type":"node_started","seq":{i}}}"#),
                created_at: "2026-01-01T00:00:00Z".to_string(),
            })
            .await
            .unwrap();
        }
        let events = repo.list_events("run-3").await.unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[2].seq, 2);
    }

    #[tokio::test]
    async fn mark_interrupted_runs() {
        let repo = make_repo().await;
        repo.create_run(make_run("run-4")).await.unwrap();
        repo.create_run(make_run("run-5")).await.unwrap();
        // Manually complete run-5
        repo.update_run_status("run-5", OrchestratorRunStatus::Completed)
            .await
            .unwrap();

        let count = repo.mark_interrupted_runs().await.unwrap();
        assert_eq!(count, 1); // only run-4 was Running

        let r4 = repo.get_run("run-4").await.unwrap().unwrap();
        assert!(matches!(r4.status, OrchestratorRunStatus::Interrupted));
        let r5 = repo.get_run("run-5").await.unwrap().unwrap();
        assert!(matches!(r5.status, OrchestratorRunStatus::Completed));
    }

    #[tokio::test]
    async fn list_runs_with_filter() {
        let repo = make_repo().await;
        repo.create_run(make_run("run-6")).await.unwrap();
        repo.create_run(make_run("run-7")).await.unwrap();
        repo.update_run_status("run-7", OrchestratorRunStatus::Failed)
            .await
            .unwrap();

        let all = repo.list_runs(None).await.unwrap();
        assert!(all.len() >= 2);

        let failed = repo
            .list_runs(Some(OrchestratorRunStatus::Failed))
            .await
            .unwrap();
        assert!(
            failed
                .iter()
                .all(|r| matches!(r.status, OrchestratorRunStatus::Failed))
        );
        assert!(failed.iter().any(|r| r.id == "run-7"));
    }
}
