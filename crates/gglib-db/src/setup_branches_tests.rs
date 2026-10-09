//! The branch columns on a database that predates them.

use sqlx::SqlitePool;

use super::super::create_schema;

async fn columns(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT \
           (SELECT COUNT(*) FROM pragma_table_info('chat_messages') WHERE name = 'origin_id') + \
           (SELECT COUNT(*) FROM pragma_table_info('chat_conversations') \
              WHERE name IN ('lineage_id', 'branch_of'))",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// A chat saved before branches reads as what it is: an original, the
/// first of a family of one.
#[tokio::test]
async fn a_database_from_before_branches_gains_them_and_its_chats_read_as_originals() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    create_schema(&pool).await.unwrap();
    sqlx::query("DROP INDEX idx_conversations_lineage")
        .execute(&pool)
        .await
        .unwrap();
    for (table, column) in [
        ("chat_messages", "origin_id"),
        ("chat_conversations", "lineage_id"),
        ("chat_conversations", "branch_of"),
    ] {
        sqlx::query(&format!("ALTER TABLE {table} DROP COLUMN {column}"))
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO chat_conversations (id, title) VALUES (5, 'old')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO chat_messages (conversation_id, role, content) VALUES (5, 'user', 'Q')",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(columns(&pool).await, 0);

    create_schema(&pool).await.unwrap();

    assert_eq!(columns(&pool).await, 3);
    let (lineage, branch_of): (Option<i64>, Option<i64>) =
        sqlx::query_as("SELECT lineage_id, branch_of FROM chat_conversations WHERE id = 5")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((lineage, branch_of), (None, None));
    let origin: Option<i64> = sqlx::query_scalar("SELECT origin_id FROM chat_messages")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(origin, None);
}

#[tokio::test]
async fn adding_the_branch_columns_twice_changes_nothing() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    create_schema(&pool).await.unwrap();
    create_schema(&pool).await.unwrap();
    assert_eq!(columns(&pool).await, 3);
}
