//! Tests for [`fill_header_facts_once`]: a write made to a row while its
//! file is read is kept.

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use gglib_core::domain::{FileType, NewModel};
use gglib_core::ports::{
    GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort, HeadMetadata, ModelRepository,
    NoopGgufParser, TensorTable,
};
use sqlx::SqlitePool;
use tokio::runtime::Handle;

use super::fill_header_facts_once;
use crate::SqliteModelRepository;
use crate::setup::setup_test_database;

/// The type another process stores while the pass reads the file.
const STORED_MEANWHILE: i64 = 99;

/// Reads every file as `Q8_0`, and while it reads one stores
/// [`STORED_MEANWHILE`] as model `id`'s type, as another process could.
struct WrittenWhileRead {
    runtime: Handle,
    pool: SqlitePool,
    id: i64,
}

impl GgufParserPort for WrittenWhileRead {
    fn parse(&self, _file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        let write = sqlx::query("UPDATE models SET file_type = ? WHERE id = ?")
            .bind(STORED_MEANWHILE)
            .bind(self.id)
            .execute(&self.pool);
        self.runtime.block_on(write).expect("the other write");
        Ok(GgufMetadata {
            file_type: Some(FileType(7)),
            ..GgufMetadata::default()
        })
    }

    fn detect_capabilities(&self, metadata: &GgufMetadata) -> GgufCapabilities {
        NoopGgufParser.detect_capabilities(metadata)
    }

    fn tensor_table(&self, path: &Path) -> Result<TensorTable, GgufParseError> {
        NoopGgufParser.tensor_table(path)
    }

    fn tensor_table_of_head(&self, head: &[u8]) -> Result<TensorTable, GgufParseError> {
        NoopGgufParser.tensor_table_of_head(head)
    }

    fn parse_head(&self, head: &[u8], file_name: &str) -> Result<HeadMetadata, GgufParseError> {
        NoopGgufParser.parse_head(head, file_name)
    }
}

/// The pass reads a row, then its file, then writes: a type stored in
/// between is not overwritten, and the row is not counted as filled.
#[tokio::test]
async fn a_type_stored_while_the_file_is_read_is_kept() {
    let pool = setup_test_database().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("zeta.gguf");
    std::fs::write(&file, b"weights").unwrap();
    let repo = SqliteModelRepository::new(pool.clone());
    let model = repo
        .insert(&NewModel::new("zeta".to_owned(), file, 7.0, Utc::now()))
        .await
        .unwrap();
    let parser = WrittenWhileRead {
        runtime: Handle::current(),
        pool: pool.clone(),
        id: model.id,
    };

    let filled = fill_header_facts_once(&pool, Arc::new(parser))
        .await
        .unwrap();

    assert_eq!(filled, Some(0));
    let kept: (Option<i64>, Option<f64>) =
        sqlx::query_as("SELECT file_type, bits_per_weight FROM models WHERE id = ?")
            .bind(model.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(kept, (Some(STORED_MEANWHILE), None));
}
