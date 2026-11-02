//! The Hub as one download's choice reads it: each listing asked once, and
//! each file's first bytes read once, a longer read continuing where the
//! last one ended.
//!
//! Choosing a variant reads a head of each one walked, a whole header of
//! one that fits, and the heads again to find the repository's projector;
//! resolving the choice lists the repository again. [`CachedHub`] stands
//! between those reads and the Hub for as long as one download is chosen
//! and resolved, so every byte is fetched once.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::ports::huggingface::{
    HfClientPort, HfFileInfo, HfPortResult, HfQuantInfo, HfRepoInfo, HfSearchOptions,
    HfSearchResult,
};

/// What has been read of one file: its first bytes, and whether they are
/// all of it.
#[derive(Clone, Default)]
struct Read {
    bytes: Vec<u8>,
    whole: bool,
}

/// A Hub client that keeps what it has read. See the module docs.
pub(crate) struct CachedHub {
    inner: Arc<dyn HfClientPort>,
    variants: Mutex<HashMap<String, Vec<HfQuantInfo>>>,
    files: Mutex<HashMap<String, Vec<HfFileInfo>>>,
    heads: Mutex<HashMap<(String, String), Read>>,
}

impl CachedHub {
    /// A cache in front of `inner`, holding nothing yet.
    pub(crate) fn new(inner: Arc<dyn HfClientPort>) -> Self {
        Self {
            inner,
            variants: Mutex::new(HashMap::new()),
            files: Mutex::new(HashMap::new()),
            heads: Mutex::new(HashMap::new()),
        }
    }
}

/// The value `cache` holds for `key`, else what `fetch` answers, kept.
async fn kept<T: Clone>(
    cache: &Mutex<HashMap<String, T>>,
    key: &str,
    fetch: impl Future<Output = HfPortResult<T>>,
) -> HfPortResult<T> {
    if let Some(held) = cache.lock().ok().and_then(|held| held.get(key).cloned()) {
        return Ok(held);
    }
    let fetched = fetch.await?;
    if let Ok(mut held) = cache.lock() {
        held.insert(key.to_owned(), fetched.clone());
    }
    Ok(fetched)
}

#[async_trait]
impl HfClientPort for CachedHub {
    async fn search(&self, options: &HfSearchOptions) -> HfPortResult<HfSearchResult> {
        self.inner.search(options).await
    }

    async fn list_quantizations(&self, model_id: &str) -> HfPortResult<Vec<HfQuantInfo>> {
        kept(
            &self.variants,
            model_id,
            self.inner.list_quantizations(model_id),
        )
        .await
    }

    async fn list_gguf_files(&self, model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        kept(&self.files, model_id, self.inner.list_gguf_files(model_id)).await
    }

    async fn get_quantization_files(
        &self,
        model_id: &str,
        quantization: &str,
    ) -> HfPortResult<Vec<HfFileInfo>> {
        self.inner
            .get_quantization_files(model_id, quantization)
            .await
    }

    async fn get_commit_sha(&self, model_id: &str) -> HfPortResult<String> {
        self.inner.get_commit_sha(model_id).await
    }

    async fn get_model_info(&self, model_id: &str) -> HfPortResult<HfRepoInfo> {
        self.inner.get_model_info(model_id).await
    }

    /// The first `max_bytes` of the file, from what is held when it covers
    /// them, else read on from where the last read of the file ended.
    async fn read_head(&self, model_id: &str, path: &str, max_bytes: u64) -> HfPortResult<Vec<u8>> {
        let key = (model_id.to_owned(), path.to_owned());
        let mut read = self
            .heads
            .lock()
            .ok()
            .and_then(|heads| heads.get(&key).cloned())
            .unwrap_or_default();
        let want = usize::try_from(max_bytes).unwrap_or(usize::MAX);
        if !read.whole && read.bytes.len() < want {
            let start = read.bytes.len() as u64;
            let asked = max_bytes - start;
            let more = if start == 0 {
                self.inner.read_head(model_id, path, max_bytes).await?
            } else {
                self.inner.read_range(model_id, path, start, asked).await?
            };
            read.whole = (more.len() as u64) < asked;
            read.bytes.extend_from_slice(&more);
            if let Ok(mut heads) = self.heads.lock() {
                heads.insert(key, read.clone());
            }
        }
        read.bytes.truncate(want);
        Ok(read.bytes)
    }

    fn file_url(&self, model_id: &str, path: &str, revision: &str) -> String {
        self.inner.file_url(model_id, path, revision)
    }

    async fn file_at(&self, model_id: &str, path: &str) -> HfPortResult<Option<HfFileInfo>> {
        self.inner.file_at(model_id, path).await
    }

    async fn fetch_generation_config(&self, model_id: &str) -> HfPortResult<Option<String>> {
        self.inner.fetch_generation_config(model_id).await
    }
}

#[cfg(test)]
#[path = "cached_hub_tests.rs"]
mod tests;
