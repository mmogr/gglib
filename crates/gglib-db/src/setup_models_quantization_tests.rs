//! Tests for dropping `models.quantization` in place: a library that has the
//! column loses it and gains the header's facts, empty, with every id and
//! every row that references one as it was.

use super::models_tests::{models_sql, pool_of_one, scalar};
use super::*;

/// Today's `models`, `AUTOINCREMENT` and all, put back to the shape it had
/// with `quantization`: two models (3 and 8, the sequence at 9), three
/// conversations naming them and one file record.
async fn library_with_the_old_column(pool: &SqlitePool) {
    create_schema(pool).await.unwrap();
    sqlx::raw_sql(
        "ALTER TABLE models DROP COLUMN variant_label;
         ALTER TABLE models DROP COLUMN file_type;
         ALTER TABLE models DROP COLUMN bits_per_weight;
         ALTER TABLE models ADD COLUMN quantization TEXT;
         INSERT INTO models (id, name, file_path, param_count_b, added_at, model_key, quantization)
             VALUES (3, 'A', '/m/a.gguf', 7.0, 'then', 'hf:a', 'Q4_K_M'),
                    (9, 'Gone', '/m/gone.gguf', 7.0, 'then', 'hf:gone', NULL),
                    (8, 'B', '/m/b.gguf', 7.0, 'then', 'hf:b', 'UD-Q4_K_XL');
         DELETE FROM models WHERE id = 9;
         INSERT INTO chat_conversations (title, model_id)
             VALUES ('first', 3), ('second', 8), ('third', 8), ('none', NULL);
         INSERT INTO model_files (model_id, file_path, file_index, expected_size)
             VALUES (8, '/m/b.gguf', 0, 10);",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// The column names of `models`.
async fn model_columns(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM pragma_table_info('models')")
        .fetch_all(pool)
        .await
        .unwrap()
}

/// Every conversation's id and the model it names, in id order.
async fn conversations(pool: &SqlitePool) -> Vec<(i64, Option<i64>)> {
    sqlx::query_as("SELECT id, model_id FROM chat_conversations ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn an_open_drops_the_old_column_and_keeps_every_id_and_conversation() {
    let pool = pool_of_one().await;
    library_with_the_old_column(&pool).await;
    assert!(
        model_columns(&pool)
            .await
            .contains(&"quantization".to_owned())
    );
    let chats = conversations(&pool).await;

    create_schema(&pool).await.unwrap();

    let columns = model_columns(&pool).await;
    assert!(!columns.contains(&"quantization".to_owned()));
    for column in ["variant_label", "file_type", "bits_per_weight"] {
        assert!(columns.contains(&column.to_owned()), "{column}");
    }
    // Dropped in place, not rebuilt: the table keeps AUTOINCREMENT and its
    // sequence, so the removed model's 9 is still never given out.
    assert!(models_sql(&pool).await.contains("AUTOINCREMENT"));
    assert_eq!(
        scalar(
            &pool,
            "SELECT seq FROM sqlite_sequence WHERE name = 'models'"
        )
        .await,
        9
    );
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM models ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(ids, [3, 8]);
    assert_eq!(conversations(&pool).await, chats);
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM model_files WHERE model_id = 8").await,
        1
    );
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM models \
             WHERE variant_label IS NULL AND file_type IS NULL AND bits_per_weight IS NULL"
        )
        .await,
        2,
        "nothing is carried over from the old column"
    );

    // A second open finds nothing to do.
    create_schema(&pool).await.unwrap();
    assert_eq!(model_columns(&pool).await, columns);
    assert_eq!(conversations(&pool).await, chats);
}
