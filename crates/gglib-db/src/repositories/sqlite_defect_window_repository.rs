//! `SQLite` implementation of [`DefectWindowRepositoryPort`].

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Row, SqlitePool};

use gglib_core::domain::defects::{ModelDefectCounts, PersistedDefectWindow};
use gglib_core::ports::{DefectWindowRepositoryPort, RepositoryError};

use super::row_mappers::parse_datetime;

/// `SQLite` implementation of [`DefectWindowRepositoryPort`].
///
/// One row per model name. Counts are stored as INTEGER; a window is a
/// handful of events, so the `u64`↔`i64` conversions clamp rather than
/// error — a saturated value is already astronomically wrong upstream.
pub struct SqliteDefectWindowRepository {
    pool: SqlitePool,
}

impl SqliteDefectWindowRepository {
    /// Create a new defect-window repository from a shared connection pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn count_from_row(row: &sqlx::sqlite::SqliteRow, column: &str) -> u64 {
    let v: i64 = row.get(column);
    u64::try_from(v).unwrap_or(0)
}

#[allow(clippy::cast_possible_wrap)]
fn count_to_db(c: u64) -> i64 {
    i64::try_from(c).unwrap_or(i64::MAX)
}

#[async_trait]
impl DefectWindowRepositoryPort for SqliteDefectWindowRepository {
    async fn load_all(&self) -> Result<Vec<PersistedDefectWindow>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT model_name, requests, loop_guard_trips, repairs_attempted, \
             repairs_succeeded, updated_at, llama_build FROM defect_windows",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        Ok(rows
            .iter()
            .map(|row| PersistedDefectWindow {
                model_name: row.get("model_name"),
                counts: ModelDefectCounts {
                    requests: count_from_row(row, "requests"),
                    loop_guard_trips: count_from_row(row, "loop_guard_trips"),
                    repairs_attempted: count_from_row(row, "repairs_attempted"),
                    repairs_succeeded: count_from_row(row, "repairs_succeeded"),
                },
                updated_at: parse_datetime(row.get("updated_at")).unwrap_or_else(Utc::now),
                llama_build: row.get("llama_build"),
            })
            .collect())
    }

    async fn upsert_many(&self, rows: &[PersistedDefectWindow]) -> Result<(), RepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        for row in rows {
            sqlx::query(
                "INSERT INTO defect_windows \
                 (model_name, requests, loop_guard_trips, repairs_attempted, \
                  repairs_succeeded, updated_at, llama_build) \
                 VALUES (?, ?, ?, ?, ?, ?, ?) \
                 ON CONFLICT(model_name) DO UPDATE SET \
                 requests = excluded.requests, \
                 loop_guard_trips = excluded.loop_guard_trips, \
                 repairs_attempted = excluded.repairs_attempted, \
                 repairs_succeeded = excluded.repairs_succeeded, \
                 updated_at = excluded.updated_at, \
                 llama_build = excluded.llama_build",
            )
            .bind(&row.model_name)
            .bind(count_to_db(row.counts.requests))
            .bind(count_to_db(row.counts.loop_guard_trips))
            .bind(count_to_db(row.counts.repairs_attempted))
            .bind(count_to_db(row.counts.repairs_succeeded))
            .bind(row.updated_at.format("%Y-%m-%d %H:%M:%S").to_string())
            .bind(&row.llama_build)
            .execute(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))
    }

    async fn delete(&self, model_names: &[String]) -> Result<(), RepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))?;

        for name in model_names {
            sqlx::query("DELETE FROM defect_windows WHERE model_name = ?")
                .bind(name)
                .execute(&mut *tx)
                .await
                .map_err(|e| RepositoryError::Storage(e.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|e| RepositoryError::Storage(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::setup_test_database;
    use chrono::TimeZone;

    fn window(model: &str, requests: u64) -> PersistedDefectWindow {
        PersistedDefectWindow {
            model_name: model.to_owned(),
            counts: ModelDefectCounts {
                requests,
                loop_guard_trips: 2,
                repairs_attempted: 5,
                repairs_succeeded: 3,
            },
            updated_at: Utc.with_ymd_and_hms(2026, 8, 12, 10, 0, 0).unwrap(),
            llama_build: "b10327".to_owned(),
        }
    }

    #[tokio::test]
    async fn upsert_then_load_round_trips() {
        let pool = setup_test_database().await.expect("test db");
        let repo = SqliteDefectWindowRepository::new(pool);

        repo.upsert_many(&[window("m1", 100)])
            .await
            .expect("upsert");
        let rows = repo.load_all().await.expect("load");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model_name, "m1");
        assert_eq!(rows[0].counts.requests, 100);
        assert_eq!(rows[0].counts.loop_guard_trips, 2);
        assert_eq!(rows[0].counts.repairs_attempted, 5);
        assert_eq!(rows[0].counts.repairs_succeeded, 3);
        assert_eq!(
            rows[0].updated_at,
            Utc.with_ymd_and_hms(2026, 8, 12, 10, 0, 0).unwrap()
        );
        assert_eq!(rows[0].llama_build, "b10327");
    }

    #[tokio::test]
    async fn an_upsert_replaces_the_existing_row_for_a_model() {
        let pool = setup_test_database().await.expect("test db");
        let repo = SqliteDefectWindowRepository::new(pool);

        repo.upsert_many(&[window("m1", 100)]).await.expect("first");
        repo.upsert_many(&[window("m1", 250)])
            .await
            .expect("second");
        let rows = repo.load_all().await.expect("load");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].counts.requests, 250);
    }

    #[tokio::test]
    async fn delete_removes_only_the_named_rows() {
        let pool = setup_test_database().await.expect("test db");
        let repo = SqliteDefectWindowRepository::new(pool);

        repo.upsert_many(&[window("m1", 100), window("m2", 200)])
            .await
            .expect("upsert");
        repo.delete(&["m1".to_owned()]).await.expect("delete");
        let rows = repo.load_all().await.expect("load");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model_name, "m2");
    }
}
