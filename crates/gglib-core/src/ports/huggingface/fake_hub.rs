//! A repository listing held in memory, for tests of what is fetched from it.

use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;

use super::client::HfClientPort;
use super::error::{HfPortError, HfPortResult};
use super::types::{HfFileInfo, HfQuantInfo, HfRepoInfo, HfSearchOptions, HfSearchResult};

/// One repository: the weights files of its one quantization, and its
/// projectors. Every quantization asked for answers the same weights.
#[derive(Default)]
pub(crate) struct FakeHub {
    pub weights: Vec<HfFileInfo>,
    pub projectors: Vec<HfFileInfo>,
    /// How many times the projectors were listed.
    pub projector_listings: AtomicUsize,
}

/// A GGUF file of the listing, with an OID.
pub(crate) fn hub_file(path: &str, size: u64, oid: &str) -> HfFileInfo {
    HfFileInfo {
        path: path.to_owned(),
        size,
        is_gguf: true,
        oid: Some(oid.to_owned()),
    }
}

#[async_trait]
impl HfClientPort for FakeHub {
    async fn get_quantization_files(
        &self,
        model_id: &str,
        quantization: &str,
    ) -> HfPortResult<Vec<HfFileInfo>> {
        if self.weights.is_empty() {
            return Err(HfPortError::QuantizationNotFound {
                model_id: model_id.to_owned(),
                quantization: quantization.to_owned(),
            });
        }
        Ok(self.weights.clone())
    }
    async fn list_projectors(&self, _model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        self.projector_listings.fetch_add(1, Ordering::Relaxed);
        Ok(self.projectors.clone())
    }
    async fn search(&self, _options: &HfSearchOptions) -> HfPortResult<HfSearchResult> {
        unimplemented!("a listing is all this hub answers")
    }
    async fn list_quantizations(&self, _model_id: &str) -> HfPortResult<Vec<HfQuantInfo>> {
        unimplemented!("a listing is all this hub answers")
    }
    async fn list_gguf_files(&self, _model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        unimplemented!("a listing is all this hub answers")
    }
    async fn get_commit_sha(&self, _model_id: &str) -> HfPortResult<String> {
        unimplemented!("a listing is all this hub answers")
    }
    async fn get_model_info(&self, _model_id: &str) -> HfPortResult<HfRepoInfo> {
        unimplemented!("a listing is all this hub answers")
    }
}
