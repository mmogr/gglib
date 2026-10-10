//! One file of a repository, named by its path: its first bytes, and its
//! size and OID.

use crate::error::HfResult;
use crate::http::HttpBackend;
use crate::models::{HfFileEntry, HfRepoRef};
use crate::parsing::parse_tree_entries;
use crate::url::{build_paths_info_url, build_resolve_url};

use super::HfClient;

impl<B: HttpBackend> HfClient<B> {
    /// The first `max_bytes` bytes of the file at `path`, read from the
    /// address a download reads the whole file from.
    pub(crate) async fn read_head(
        &self,
        repo: &HfRepoRef,
        path: &str,
        max_bytes: u64,
    ) -> HfResult<Vec<u8>> {
        let url = build_resolve_url(&self.config, repo, path);
        self.backend.get_head(&url, max_bytes).await
    }

    /// The file at `path` on `main`, or `None` when the repository holds no
    /// file there (the path is absent, or is a folder).
    pub(crate) async fn file_at(
        &self,
        repo: &HfRepoRef,
        path: &str,
    ) -> HfResult<Option<HfFileEntry>> {
        let url = build_paths_info_url(&self.config, repo);
        let json: serde_json::Value = self
            .backend
            .post_form_json(&url, &[("paths", path)])
            .await?;
        Ok(parse_tree_entries(&json)?
            .into_iter()
            .find(|entry| entry.path == path && !entry.is_directory()))
    }
}

#[cfg(test)]
#[path = "one_file_tests.rs"]
mod tests;
