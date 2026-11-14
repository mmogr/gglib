//! One file of a repository, named by its path: its first bytes, and its
//! size and OID.

use crate::error::HfResult;
use crate::http::HttpBackend;
use crate::models::{HfFileEntry, HfRepoRef};
use crate::parsing::{parse_tree_entries, variants};
use crate::url::{build_paths_info_url, build_resolve_url};
use gglib_core::ports::huggingface::HfQuantInfo;

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

    /// At most `max_bytes` bytes of the file at `path` from byte `start`:
    /// a head read further without reading its first bytes again.
    pub(crate) async fn read_range(
        &self,
        repo: &HfRepoRef,
        path: &str,
        start: u64,
        max_bytes: u64,
    ) -> HfResult<Vec<u8>> {
        let url = build_resolve_url(&self.config, repo, path);
        self.backend.get_range(&url, start, max_bytes).await
    }

    /// The variants the files at `paths` on `main` make, with their sizes:
    /// one request (`paths-info`), however many files, where a listing of
    /// the tree asks one per folder. A path the repository does not hold is
    /// left out.
    pub(crate) async fn sized_variants(
        &self,
        repo: &HfRepoRef,
        paths: &[String],
    ) -> HfResult<Vec<HfQuantInfo>> {
        let url = build_paths_info_url(&self.config, repo);
        let form: Vec<(&str, &str)> = paths.iter().map(|path| ("paths", path.as_str())).collect();
        let json: serde_json::Value = self.backend.post_form_json(&url, &form).await?;
        Ok(variants(&parse_tree_entries(&json)?))
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
