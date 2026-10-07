//! An [`AppCore`] made from repositories alone, for tests that neither ask
//! the Hub nor download.

use std::sync::Arc;

use async_trait::async_trait;

use super::{AppCore, DownloadTriggerPort};
use crate::ports::huggingface::HfPortResult;
use crate::ports::{
    HfClientPort, HfFileInfo, HfQuantInfo, HfRepoInfo, HfSearchOptions, HfSearchResult, Repos,
};

impl AppCore {
    /// A core over `repos` with nothing else behind it. Its verification
    /// service reads the models and file rows of `repos`, and has no Hub to
    /// check for updates against and no queue to repair through: a test of
    /// either builds its core with [`AppCore::new`] and doubles of its own.
    #[must_use]
    pub fn bare(repos: Repos) -> Self {
        Self::new(repos, Arc::new(NoHub), Arc::new(NoQueue))
    }
}

/// The Hub of a core that has none.
struct NoHub;

#[async_trait]
impl HfClientPort for NoHub {
    async fn search(&self, _options: &HfSearchOptions) -> HfPortResult<HfSearchResult> {
        unimplemented!("a bare core has no Hub")
    }
    async fn list_quantizations(&self, _model_id: &str) -> HfPortResult<Vec<HfQuantInfo>> {
        unimplemented!("a bare core has no Hub")
    }
    async fn list_projectors(&self, _model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        unimplemented!("a bare core has no Hub")
    }
    async fn list_gguf_files(&self, _model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        unimplemented!("a bare core has no Hub")
    }
    async fn get_quantization_files(
        &self,
        _model_id: &str,
        _quantization: &str,
    ) -> HfPortResult<Vec<HfFileInfo>> {
        unimplemented!("a bare core has no Hub")
    }
    async fn get_commit_sha(&self, _model_id: &str) -> HfPortResult<String> {
        unimplemented!("a bare core has no Hub")
    }
    async fn get_model_info(&self, _model_id: &str) -> HfPortResult<HfRepoInfo> {
        unimplemented!("a bare core has no Hub")
    }
}

/// The download queue of a core that has none.
struct NoQueue;

#[async_trait]
impl DownloadTriggerPort for NoQueue {
    async fn queue_download(
        &self,
        _repo_id: String,
        _quantization: Option<String>,
    ) -> anyhow::Result<String> {
        unimplemented!("a bare core has no download queue")
    }
}
