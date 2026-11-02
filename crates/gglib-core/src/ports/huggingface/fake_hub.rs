//! A repository listing held in memory, for tests of what is fetched from it.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use std::path::Path;

use async_trait::async_trait;

use super::client::HfClientPort;
use super::error::{HfPortError, HfPortResult};
use super::types::{HfFileInfo, HfQuantInfo, HfRepoInfo, HfSearchOptions, HfSearchResult};
use super::variants::variants_of;
use crate::domain::TensorTable;
use crate::download::GgufFileRole;
use crate::ports::{
    GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort, HeadMetadata, NoopGgufParser,
};

/// One repository: the weights files of its one variant, and its
/// projectors. Every label asked for answers the same weights.
///
/// It also holds the heads of files, for a head read, and files of other
/// repositories, for a file looked up by its path.
#[derive(Default)]
pub struct FakeHub {
    /// The weights files, which every quantization asked for answers.
    pub weights: Vec<HfFileInfo>,
    /// The repository's projectors.
    pub projectors: Vec<HfFileInfo>,
    /// How many times the repository's files were listed.
    pub file_listings: AtomicUsize,
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

/// The head [`FakeHub`] gives a projector, which [`RoleParser`] reads as
/// one.
pub const PROJECTOR_HEAD: &[u8] = b"projector";

/// Reads [`PROJECTOR_HEAD`] as a projector's whole header, and any other head
/// as a model's.
#[derive(Debug, Clone, Copy, Default)]
pub struct RoleParser;

impl GgufParserPort for RoleParser {
    fn parse(&self, path: &Path) -> Result<GgufMetadata, GgufParseError> {
        NoopGgufParser.parse(path)
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
    fn parse_head(&self, head: &[u8], _file_name: &str) -> Result<HeadMetadata, GgufParseError> {
        let role = if head == PROJECTOR_HEAD {
            GgufFileRole::Projector
        } else {
            GgufFileRole::Weights
        };
        Ok(HeadMetadata {
            complete: true,
            role: Some(role),
            ..HeadMetadata::default()
        })
    }
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
    async fn search(&self, _options: &HfSearchOptions) -> HfPortResult<HfSearchResult> {
        unimplemented!("a listing is all this hub answers")
    }
    /// The weights files grouped and labelled as the Hub's listing is.
    async fn list_quantizations(&self, _model_id: &str) -> HfPortResult<Vec<HfQuantInfo>> {
        Ok(variants_of(
            self.weights
                .iter()
                .map(|file| (file.path.as_str(), file.size)),
        ))
    }
    /// The weights files, then the projectors.
    async fn list_gguf_files(&self, _model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        self.file_listings.fetch_add(1, Ordering::Relaxed);
        Ok(self
            .weights
            .iter()
            .chain(&self.projectors)
            .cloned()
            .collect())
    }
    async fn get_commit_sha(&self, _model_id: &str) -> HfPortResult<String> {
        unimplemented!("a listing is all this hub answers")
    }
    async fn get_model_info(&self, _model_id: &str) -> HfPortResult<HfRepoInfo> {
        unimplemented!("a listing is all this hub answers")
    }
    /// The head held for `path`, cut at `max_bytes`; a projector's, when
    /// none is held for it, is [`PROJECTOR_HEAD`].
    async fn read_head(&self, model_id: &str, path: &str, max_bytes: u64) -> HfPortResult<Vec<u8>> {
        let mut asked = self.heads_asked.lock().unwrap();
        asked.push((model_id.to_owned(), path.to_owned(), max_bytes));
        drop(asked);
        let stored = self.heads.iter().find(|(held, _)| held == path);
        let head: &[u8] = match stored {
            Some((_, head)) => head,
            None if self.projectors.iter().any(|file| file.path == path) => PROJECTOR_HEAD,
            None => {
                return Err(HfPortError::FileNotFound {
                    model_id: model_id.to_owned(),
                    path: path.to_owned(),
                });
            }
        };
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
