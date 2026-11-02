//! An upgrade's new files and the header's facts: the type and bits per
//! weight are read again, and the label the download asked for stays.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;

use super::tests::MockModelRepo;
use super::*;
use crate::domain::{
    FileType, GgufCapabilities, GgufMetadata, HeadMetadata, NewModel, TensorPayload, TensorTable,
};
use crate::ports::{GgufParseError, NoopGgufParser};

/// Reads every file as a `Q8_0` header over 8000 weights in 8500 bytes.
struct Q8Parser;

impl GgufParserPort for Q8Parser {
    fn parse(&self, _path: &Path) -> Result<GgufMetadata, GgufParseError> {
        Ok(GgufMetadata {
            file_type: Some(FileType(7)),
            payload: Some(TensorPayload {
                bytes: 8_500,
                elements: 8_000,
            }),
            ..Default::default()
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

/// An upgrade's new files may be another quantization under the same
/// name: their header's type and bits replace the old ones, and the
/// label the download asked for stays.
#[tokio::test]
async fn an_upgrade_reads_the_new_files_type_and_bits_and_keeps_the_label() {
    let repo = Arc::new(MockModelRepo::new());
    let mut held = NewModel::new(
        "model".to_owned(),
        PathBuf::from("/models/old.gguf"),
        8.0,
        Utc::now(),
    );
    held.variant_label = Some("UD-Q4_K_XL".to_owned());
    held.file_type = Some(FileType(15));
    held.bits_per_weight = Some(4.5);
    let id = repo.insert(&held).await.unwrap().id;
    let registrar = ModelRegistrar::new(repo, Arc::new(Q8Parser), None);

    let download = CompletedDownload {
        primary_path: PathBuf::from("/models/new.gguf"),
        all_paths: vec![PathBuf::from("/models/new.gguf")],
        projector_path: None,
        components: vec![],
        variant_label: Some("Q8_0".to_owned()),
        repo_id: "test/model".to_string(),
        commit_sha: "def456".to_string(),
        is_sharded: false,
        file_paths: None,
        hf_tags: vec![],
        hf_file_entries: vec![],
    };
    let upgraded = registrar.upgrade_model(id, &download).await.unwrap().model;
    assert_eq!(upgraded.file_type, Some(FileType(7)));
    assert_eq!(upgraded.bits_per_weight, Some(8.5));
    assert_eq!(upgraded.variant_label.as_deref(), Some("UD-Q4_K_XL"));
}
