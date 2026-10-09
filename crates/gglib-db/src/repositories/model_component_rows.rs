//! The rows of `model_components`, read and written beside a model's own
//! row by `SqliteModelRepository`.
//!
//! A path is stored normalised as `models.projector_path` is, and a role is
//! stored by its wire name. A row whose role this build does not know is
//! skipped on read rather than failing the model.

use std::collections::HashMap;

use gglib_core::RepositoryError;
use gglib_core::domain::ModelComponent;
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::row_mappers::normalized_file_path_string;

fn storage(error: &sqlx::Error) -> RepositoryError {
    RepositoryError::Storage(error.to_string())
}

/// One row as a component, when its role is one this build knows.
fn component(row: &sqlx::sqlite::SqliteRow) -> Result<Option<ModelComponent>, RepositoryError> {
    let role: String = row.try_get("role").map_err(|e| storage(&e))?;
    let path: String = row.try_get("path").map_err(|e| storage(&e))?;
    Ok(role.parse().ok().map(|role| ModelComponent {
        role,
        path: path.into(),
    }))
}

/// The components of model `model_id`, in role order.
pub(super) async fn of_model(
    pool: &SqlitePool,
    model_id: i64,
) -> Result<Vec<ModelComponent>, RepositoryError> {
    let rows = sqlx::query("SELECT role, path FROM model_components WHERE model_id = ?")
        .bind(model_id)
        .fetch_all(pool)
        .await
        .map_err(|e| storage(&e))?;
    let mut components = Vec::with_capacity(rows.len());
    for row in &rows {
        components.extend(component(row)?);
    }
    components.sort_by_key(|c| c.role);
    Ok(components)
}

/// Every model's components in one query, by model id, each in role order.
pub(super) async fn by_model(
    pool: &SqlitePool,
) -> Result<HashMap<i64, Vec<ModelComponent>>, RepositoryError> {
    let rows = sqlx::query("SELECT model_id, role, path FROM model_components")
        .fetch_all(pool)
        .await
        .map_err(|e| storage(&e))?;
    let mut by_model: HashMap<i64, Vec<ModelComponent>> = HashMap::new();
    for row in &rows {
        let model_id: i64 = row.try_get("model_id").map_err(|e| storage(&e))?;
        if let Some(component) = component(row)? {
            by_model.entry(model_id).or_default().push(component);
        }
    }
    for components in by_model.values_mut() {
        components.sort_by_key(|c| c.role);
    }
    Ok(by_model)
}

/// Write `components` for model `model_id`, keeping any role it already
/// links: a re-registration never overwrites a link, as the projector's
/// upsert coalesces.
pub(super) async fn insert_keeping(
    pool: &SqlitePool,
    model_id: i64,
    components: &[ModelComponent],
) -> Result<(), RepositoryError> {
    for component in components {
        sqlx::query(
            "INSERT OR IGNORE INTO model_components (model_id, role, path) VALUES (?, ?, ?)",
        )
        .bind(model_id)
        .bind(component.role.as_str())
        .bind(normalized_file_path_string(&component.path))
        .execute(pool)
        .await
        .map_err(|e| storage(&e))?;
    }
    Ok(())
}

/// Replace model `model_id`'s components with `components`, on `conn`, which
/// holds the transaction the model's own row is updated in.
///
/// A link the update removes or points elsewhere takes its `model_files` row
/// with it. The registrar records a companion there, by the absolute path
/// the link holds, only while the model links it; left behind, the row would
/// read as one of the model's own files, and a repair would delete a file
/// other models draw with.
pub(super) async fn replace(
    conn: &mut SqliteConnection,
    model_id: i64,
    components: &[ModelComponent],
) -> Result<(), RepositoryError> {
    let kept: Vec<(&str, String)> = components
        .iter()
        .map(|c| (c.role.as_str(), normalized_file_path_string(&c.path)))
        .collect();
    let held = sqlx::query("SELECT role, path FROM model_components WHERE model_id = ?")
        .bind(model_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| storage(&e))?;
    for row in &held {
        let role: String = row.try_get("role").map_err(|e| storage(&e))?;
        let path: String = row.try_get("path").map_err(|e| storage(&e))?;
        if !kept.iter().any(|(r, p)| *r == role && *p == path) {
            sqlx::query("DELETE FROM model_files WHERE model_id = ? AND file_path = ?")
                .bind(model_id)
                .bind(&path)
                .execute(&mut *conn)
                .await
                .map_err(|e| storage(&e))?;
        }
    }
    sqlx::query("DELETE FROM model_components WHERE model_id = ?")
        .bind(model_id)
        .execute(&mut *conn)
        .await
        .map_err(|e| storage(&e))?;
    for component in components {
        sqlx::query("INSERT INTO model_components (model_id, role, path) VALUES (?, ?, ?)")
            .bind(model_id)
            .bind(component.role.as_str())
            .bind(normalized_file_path_string(&component.path))
            .execute(&mut *conn)
            .await
            .map_err(|e| storage(&e))?;
    }
    Ok(())
}
