//! Unit tests for schema operations.

use gglib::services::database::ensure_column_exists;

#[path = "../../common/mod.rs"]
mod common;

use common::database::setup_test_pool;

#[tokio::test]
async fn test_ensure_column_exists_adds_new_column() {
    let pool = setup_test_pool().await.unwrap();

    // Add a new column to the models table
    let result = ensure_column_exists(&pool, "models", "test_column", "TEXT").await;
    assert!(result.is_ok());

    // Verify the column exists by querying pragma
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('models') WHERE name = 'test_column'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_ensure_column_exists_noop_for_existing() {
    let pool = setup_test_pool().await.unwrap();

    // name column already exists
    let result = ensure_column_exists(&pool, "models", "name", "TEXT").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_ensure_column_exists_rejects_invalid_table() {
    let pool = setup_test_pool().await.unwrap();

    let result = ensure_column_exists(&pool, "invalid_table", "column", "TEXT").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid table name"));
}

#[tokio::test]
async fn test_ensure_column_exists_rejects_invalid_column_name() {
    let pool = setup_test_pool().await.unwrap();

    let result = ensure_column_exists(&pool, "models", "invalid-column", "TEXT").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid column name"));
}

#[tokio::test]
async fn test_ensure_column_exists_rejects_invalid_definition() {
    let pool = setup_test_pool().await.unwrap();

    let result = ensure_column_exists(&pool, "models", "new_col", "INVALID_TYPE").await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid column definition"));
}

#[tokio::test]
async fn test_ensure_column_exists_accepts_valid_types() {
    let pool = setup_test_pool().await.unwrap();

    // Test each valid type
    assert!(ensure_column_exists(&pool, "models", "col_text", "TEXT").await.is_ok());
    assert!(ensure_column_exists(&pool, "models", "col_int", "INTEGER").await.is_ok());
    assert!(ensure_column_exists(&pool, "models", "col_real", "REAL").await.is_ok());
    assert!(ensure_column_exists(&pool, "models", "col_blob", "BLOB").await.is_ok());
}
