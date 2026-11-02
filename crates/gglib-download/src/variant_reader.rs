//! What [`pick_variant`](gglib_core::download::pick_variant) reads of a
//! repository's variants, read from the Hub as it asks.
//!
//! A variant's head is the first [`HEAD_BYTES`] of its first file. An image
//! model's header ends there or soon after, its tensor table with it, so a
//! head that holds the whole metadata has its tensor table sniffed for a
//! family, from a longer read when the table runs past the head. A chat
//! model's head never holds the whole metadata (its tokenizer follows), so
//! it costs no second read.
//!
//! A variant's whole header is read in steps of [`HEADER_READS`] until one
//! holds every pair the file declares: converters often write
//! `general.file_type` last, past a tokenizer of ten megabytes or more. A
//! download reads through a [`CachedHub`](crate::cached_hub::CachedHub), so
//! each step goes on from where the last ended.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::domain::ImageFamily;
use gglib_core::download::{HeadFacts, VariantReader};
use gglib_core::ports::huggingface::{HfClientPort, HfQuantInfo, SNIFF_HEAD_BYTES};
use gglib_core::ports::{GgufMetadata, GgufParserPort, HeadMetadata};

/// How much of a variant's first file its head is: 64 KiB.
pub(crate) const HEAD_BYTES: u64 = 64 * 1024;

/// The lengths a whole header is looked for in, from the start of the file:
/// the last ends the search, holding the header or not.
pub(crate) const HEADER_READS: [u64; 4] = [1 << 20, 8 << 20, 32 << 20, 128 << 20];

/// Where the pick's reader for a repository's variants comes from.
pub(crate) trait VariantSource: Send + Sync {
    /// A reader over `variants` of `repo_id`, by their index.
    fn reader<'a>(
        &'a self,
        repo_id: &'a str,
        variants: &[HfQuantInfo],
    ) -> Box<dyn VariantReader + 'a>;
}

/// The Hub, read with a client and a parser.
pub(crate) struct HubSource {
    pub(crate) client: Arc<dyn HfClientPort>,
    pub(crate) parser: Arc<dyn GgufParserPort>,
}

impl VariantSource for HubSource {
    fn reader<'a>(
        &'a self,
        repo_id: &'a str,
        variants: &[HfQuantInfo],
    ) -> Box<dyn VariantReader + 'a> {
        let first_files = variants
            .iter()
            .map(|variant| variant.file_paths.first().cloned().unwrap_or_default())
            .collect();
        Box::new(HubVariants::new(
            self.client.as_ref(),
            self.parser.as_ref(),
            repo_id,
            first_files,
        ))
    }
}

/// A repository's variants, each by the path of its first file, read from
/// the Hub.
pub(crate) struct HubVariants<'a> {
    client: &'a dyn HfClientPort,
    parser: &'a dyn GgufParserPort,
    repo: &'a str,
    first_files: Vec<String>,
    /// Each head read so far, kept so a second ask reads nothing.
    heads: Mutex<HashMap<usize, Option<HeadFacts>>>,
}

impl<'a> HubVariants<'a> {
    /// The variants of `repo` whose first files are `first_files`, in the
    /// order the pick's contenders are.
    pub(crate) fn new(
        client: &'a dyn HfClientPort,
        parser: &'a dyn GgufParserPort,
        repo: &'a str,
        first_files: Vec<String>,
    ) -> Self {
        Self {
            client,
            parser,
            repo,
            first_files,
            heads: Mutex::new(HashMap::new()),
        }
    }

    /// The first `max_bytes` of `path`, parsed as a head; `None` when they
    /// cannot be read or are not a GGUF header.
    async fn parsed(&self, path: &str, max_bytes: u64) -> Option<(Vec<u8>, HeadMetadata)> {
        let bytes = match self.client.read_head(self.repo, path, max_bytes).await {
            Ok(bytes) => bytes,
            Err(unread) => {
                tracing::debug!(repo = self.repo, path, error = %unread, "a variant's file could not be read");
                return None;
            }
        };
        let file_name = path.rsplit('/').next().unwrap_or(path);
        let head = self.parser.parse_head(&bytes, file_name).ok()?;
        Some((bytes, head))
    }

    /// Whether the tensor table of `path`, whose metadata ends inside
    /// `bytes`, names an image family. A table that runs past `bytes` is
    /// read again from [`SNIFF_HEAD_BYTES`].
    async fn image(&self, path: &str, bytes: &[u8]) -> bool {
        let sniff = |bytes: &[u8]| {
            self.parser
                .tensor_table_of_head(bytes)
                .ok()
                .map(|table| ImageFamily::sniff(&table).is_some())
        };
        if let Some(image) = sniff(bytes) {
            return image;
        }
        if (bytes.len() as u64) < HEAD_BYTES {
            // The whole file was read: there is no more of the table.
            return false;
        }
        self.client
            .read_head(self.repo, path, SNIFF_HEAD_BYTES)
            .await
            .is_ok_and(|longer| sniff(&longer).unwrap_or(false))
    }
}

#[async_trait]
impl VariantReader for HubVariants<'_> {
    async fn head(&self, index: usize) -> Option<HeadFacts> {
        if let Some(known) = self.heads.lock().ok()?.get(&index) {
            return known.clone();
        }
        let path = self.first_files.get(index)?;
        let facts = match self.parsed(path, HEAD_BYTES).await {
            Some((bytes, head)) => {
                let image = head.complete && self.image(path, &bytes).await;
                Some(HeadFacts { head, image })
            }
            None => None,
        };
        if let Ok(mut heads) = self.heads.lock() {
            heads.insert(index, facts.clone());
        }
        facts
    }

    async fn header(&self, index: usize) -> Option<GgufMetadata> {
        let path = self.first_files.get(index)?;
        let mut last = None;
        for max_bytes in HEADER_READS {
            let (bytes, head) = self.parsed(path, max_bytes).await?;
            let whole = head.complete || (bytes.len() as u64) < max_bytes;
            last = Some(head.metadata);
            if whole {
                break;
            }
        }
        last
    }
}

#[cfg(test)]
#[path = "variant_reader_tests.rs"]
mod tests;
