//! Tests for [`super::CachedHub`]: what reaches the Hub behind it.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::ports::huggingface::{
    HfClientPort, HfFileInfo, HfPortResult, HfQuantInfo, HfRepoInfo, HfSearchOptions,
    HfSearchResult,
};

use super::CachedHub;

/// A Hub of one file, `len` bytes each its offset's low byte, that writes
/// down every call.
struct Hub {
    len: usize,
    calls: Mutex<Vec<String>>,
}

impl Hub {
    fn new(len: usize) -> Arc<Self> {
        Arc::new(Self {
            len,
            calls: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn call(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }

    fn bytes(&self, start: u64, max: u64) -> Vec<u8> {
        #[allow(clippy::cast_possible_truncation, reason = "a test file's offsets")]
        (start..(start + max).min(self.len as u64))
            .map(|offset| offset as u8)
            .collect()
    }
}

#[async_trait]
impl HfClientPort for Hub {
    async fn search(&self, _options: &HfSearchOptions) -> HfPortResult<HfSearchResult> {
        unimplemented!()
    }
    async fn list_quantizations(&self, _model_id: &str) -> HfPortResult<Vec<HfQuantInfo>> {
        self.call("list".to_owned());
        Ok(Vec::new())
    }
    async fn list_gguf_files(&self, _model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        self.call("files".to_owned());
        Ok(Vec::new())
    }
    async fn get_quantization_files(
        &self,
        _model_id: &str,
        _quantization: &str,
    ) -> HfPortResult<Vec<HfFileInfo>> {
        unimplemented!()
    }
    async fn get_commit_sha(&self, _model_id: &str) -> HfPortResult<String> {
        unimplemented!()
    }
    async fn get_model_info(&self, _model_id: &str) -> HfPortResult<HfRepoInfo> {
        unimplemented!()
    }
    async fn read_head(
        &self,
        _model_id: &str,
        _path: &str,
        max_bytes: u64,
    ) -> HfPortResult<Vec<u8>> {
        self.call(format!("head {max_bytes}"));
        Ok(self.bytes(0, max_bytes))
    }
    async fn read_range(
        &self,
        _model_id: &str,
        _path: &str,
        start: u64,
        max_bytes: u64,
    ) -> HfPortResult<Vec<u8>> {
        self.call(format!("range {start} {max_bytes}"));
        Ok(self.bytes(start, max_bytes))
    }
    async fn file_at(&self, _model_id: &str, _path: &str) -> HfPortResult<Option<HfFileInfo>> {
        unimplemented!()
    }
}

#[tokio::test]
async fn each_listing_reaches_the_hub_once() {
    let hub = Hub::new(0);
    let cached = CachedHub::new(hub.clone());

    for _ in 0..3 {
        cached.list_quantizations("o/r").await.unwrap();
        cached.list_gguf_files("o/r").await.unwrap();
    }

    assert_eq!(hub.calls(), ["list", "files"]);
}

/// A head is read once; a shorter one is cut from it, and a longer one
/// reads on from where it ended.
#[tokio::test]
async fn a_head_is_read_once_and_read_on_from_where_it_ended() {
    let hub = Hub::new(1_000);
    let cached = CachedHub::new(hub.clone());

    let head = cached.read_head("o/r", "m.gguf", 100).await.unwrap();
    let shorter = cached.read_head("o/r", "m.gguf", 10).await.unwrap();
    let longer = cached.read_head("o/r", "m.gguf", 300).await.unwrap();

    assert_eq!(head, hub.bytes(0, 100));
    assert_eq!(shorter, hub.bytes(0, 10));
    assert_eq!(longer, hub.bytes(0, 300));
    assert_eq!(hub.calls(), ["head 100", "range 100 200"]);
}

/// A file shorter than a read is all read: no longer read asks again.
#[tokio::test]
async fn a_file_read_whole_is_not_read_again() {
    let hub = Hub::new(50);
    let cached = CachedHub::new(hub.clone());

    cached.read_head("o/r", "m.gguf", 100).await.unwrap();
    let again = cached.read_head("o/r", "m.gguf", 1_000).await.unwrap();

    assert_eq!(again, hub.bytes(0, 50));
    assert_eq!(hub.calls(), ["head 100"]);
}

/// A head that ends exactly where the file does reads on to nothing: that
/// empty read is the end of the file, and no longer read asks again.
#[tokio::test]
async fn an_empty_read_on_is_the_end_of_the_file() {
    let hub = Hub::new(100);
    let cached = CachedHub::new(hub.clone());

    cached.read_head("o/r", "m.gguf", 100).await.unwrap();
    let longer = cached.read_head("o/r", "m.gguf", 300).await.unwrap();
    let again = cached.read_head("o/r", "m.gguf", 1_000).await.unwrap();

    assert_eq!(longer, hub.bytes(0, 100));
    assert_eq!(again, hub.bytes(0, 100));
    assert_eq!(hub.calls(), ["head 100", "range 100 200"]);
}
