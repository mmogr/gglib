//! An [`AppCore`] made from repositories alone, for tests that neither ask
//! the Hub nor download.

use std::sync::Arc;

use async_trait::async_trait;

use super::AppCore;
use crate::download::DownloadError;
use crate::ports::huggingface::HfPortResult;
use crate::ports::{
    AskedDownloads, HfClientPort, HfFileInfo, HfQuantInfo, HfRepoInfo, HfSearchOptions,
    HfSearchResult, Repos,
};

impl AppCore {
    /// A core over `repos` with nothing else behind it. Its verification
    /// service reads the models and file rows of `repos`, and has no Hub to
    /// check for updates against and no queue to repair through: a test of
    /// either builds its core with [`AppCore::new`] and doubles of its own.
    #[must_use]
    pub fn bare(repos: Repos) -> Self {
        let no_queue = DownloadError::other("a bare core has no download queue");
        let downloads = Arc::new(AskedDownloads::refusing(no_queue));
        Self::new(repos, Arc::new(NoHub), downloads)
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
    async fn read_head(
        &self,
        _model_id: &str,
        _path: &str,
        _max_bytes: u64,
    ) -> HfPortResult<Vec<u8>> {
        unimplemented!("a bare core has no Hub")
    }
    async fn file_at(&self, _model_id: &str, _path: &str) -> HfPortResult<Option<HfFileInfo>> {
        unimplemented!("a bare core has no Hub")
    }
}
