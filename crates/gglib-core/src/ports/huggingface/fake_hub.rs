//! A repository listing held in memory, for tests of what is fetched from it.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;

use super::client::HfClientPort;
use super::error::{HfPortError, HfPortResult};
use super::types::{HfFileInfo, HfQuantInfo, HfRepoInfo, HfSearchOptions, HfSearchResult};
use crate::download::Quantization;

/// One repository: the weights files of its one quantization, and its
/// projectors. Every quantization asked for answers the same weights.
///
/// It also holds the heads of files, for a head read, and files of other
/// repositories, for a file looked up by its path.
#[derive(Default)]
pub struct FakeHub {
    /// The weights files, which every quantization asked for answers.
    pub weights: Vec<HfFileInfo>,
    /// The repository's projectors.
    pub projectors: Vec<HfFileInfo>,
    /// How many times the projectors were listed.
    pub projector_listings: AtomicUsize,
    /// The quantization each request for weights named, oldest first.
    pub quantizations_asked: Mutex<Vec<String>>,
    /// The bytes a file's head is read as, by its path in any repository. A
    /// file with none cannot have its head read.
    pub heads: Vec<(String, Vec<u8>)>,
    /// Each head read asked of it, as repository, path and length, oldest
    /// first.
    pub heads_asked: Mutex<Vec<(String, String, u64)>>,
    /// Files looked up by repository and path.
    pub files_at: Vec<(String, HfFileInfo)>,
}

/// A GGUF file of the listing, with an OID.
#[must_use]
pub fn hub_file(path: &str, size: u64, oid: &str) -> HfFileInfo {
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
        let mut asked = self.quantizations_asked.lock().unwrap();
        asked.push(quantization.to_owned());
        drop(asked);
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
    /// The one quantization, named as the first weights file names it.
    async fn list_quantizations(&self, _model_id: &str) -> HfPortResult<Vec<HfQuantInfo>> {
        Ok(self.weights.first().map_or_else(Vec::new, |first| {
            vec![HfQuantInfo {
                name: Quantization::from_filename(&first.path).to_string(),
                shard_count: self.weights.len(),
                total_size: self.weights.iter().map(|file| file.size).sum(),
                file_paths: self.weights.iter().map(|file| file.path.clone()).collect(),
            }]
        }))
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
    /// The head held for `path`, cut at `max_bytes`.
    async fn read_head(&self, model_id: &str, path: &str, max_bytes: u64) -> HfPortResult<Vec<u8>> {
        let mut asked = self.heads_asked.lock().unwrap();
        asked.push((model_id.to_owned(), path.to_owned(), max_bytes));
        drop(asked);
        let (_, head) = self
            .heads
            .iter()
            .find(|(held, _)| held == path)
            .ok_or_else(|| HfPortError::FileNotFound {
                model_id: model_id.to_owned(),
                path: path.to_owned(),
            })?;
        let cap = usize::try_from(max_bytes).unwrap_or(usize::MAX);
        Ok(head[..head.len().min(cap)].to_vec())
    }
    async fn file_at(&self, model_id: &str, path: &str) -> HfPortResult<Option<HfFileInfo>> {
        Ok(self
            .files_at
            .iter()
            .find(|(repo, file)| repo == model_id && file.path == path)
            .map(|(_, file)| file.clone()))
    }
}
