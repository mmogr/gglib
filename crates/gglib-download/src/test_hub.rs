//! A repository held in memory as its list of GGUF files, for tests of what
//! is resolved and queued from it.

use std::path::Path;

use async_trait::async_trait;
use gglib_core::download::{GgufFileRole, Quantization};
use gglib_core::ports::huggingface::{
    HfClientPort, HfFileInfo, HfPortError, HfPortResult, HfQuantInfo, HfRepoInfo, HfSearchOptions,
    HfSearchResult,
};

/// One repository. A file's role is read from its name by
/// [`GgufFileRole::classify`], and its OID is `oid-<path>`.
pub(crate) struct RepoHub {
    files: Vec<HfFileInfo>,
}

impl RepoHub {
    /// A repository holding `files`, each a path and a size.
    pub(crate) fn new(files: &[(&str, u64)]) -> Self {
        let files = files
            .iter()
            .map(|(path, size)| HfFileInfo {
                path: (*path).to_string(),
                size: *size,
                is_gguf: true,
                oid: Some(format!("oid-{path}")),
            })
            .collect();
        Self { files }
    }

    /// The files of one role, by path.
    fn of_role(&self, projector: bool) -> Vec<HfFileInfo> {
        let mut files: Vec<HfFileInfo> = self
            .files
            .iter()
            .filter(|f| GgufFileRole::classify(Path::new(&f.path)).is_projector() == projector)
            .cloned()
            .collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        files
    }
}

#[async_trait]
impl HfClientPort for RepoHub {
    async fn list_quantizations(&self, _model_id: &str) -> HfPortResult<Vec<HfQuantInfo>> {
        let mut quantizations: Vec<HfQuantInfo> = Vec::new();
        for file in self.of_role(false) {
            let name = Quantization::from_filename(&file.path).to_string();
            if let Some(known) = quantizations.iter_mut().find(|q| q.name == name) {
                known.shard_count += 1;
                known.total_size += file.size;
                known.file_paths.push(file.path);
            } else {
                quantizations.push(HfQuantInfo {
                    name,
                    shard_count: 1,
                    total_size: file.size,
                    file_paths: vec![file.path],
                });
            }
        }
        Ok(quantizations)
    }

    async fn list_projectors(&self, _model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        Ok(self.of_role(true))
    }

    async fn get_quantization_files(
        &self,
        model_id: &str,
        quantization: &str,
    ) -> HfPortResult<Vec<HfFileInfo>> {
        let files: Vec<HfFileInfo> = self
            .of_role(false)
            .into_iter()
            .filter(|f| Quantization::from_filename(&f.path).to_string() == quantization)
            .collect();
        if files.is_empty() {
            return Err(HfPortError::QuantizationNotFound {
                model_id: model_id.to_string(),
                quantization: quantization.to_string(),
            });
        }
        Ok(files)
    }

    async fn search(&self, _options: &HfSearchOptions) -> HfPortResult<HfSearchResult> {
        unimplemented!("a file list is all this hub answers")
    }
    async fn list_gguf_files(&self, _model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        unimplemented!("a file list is all this hub answers")
    }
    async fn get_commit_sha(&self, _model_id: &str) -> HfPortResult<String> {
        unimplemented!("a file list is all this hub answers")
    }
    /// The repository's card is not held: a registration goes on without
    /// its tags.
    async fn get_model_info(&self, model_id: &str) -> HfPortResult<HfRepoInfo> {
        Err(HfPortError::ModelNotFound {
            model_id: model_id.to_string(),
        })
    }
}
