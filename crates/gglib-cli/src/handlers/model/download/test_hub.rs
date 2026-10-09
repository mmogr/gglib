//! A Hub for the tests of `model search` and `model browse`: it answers
//! every search with the hits it holds, and keeps what each search asked.

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::ports::huggingface::HfPortResult;
use gglib_core::ports::{
    HfClientPort, HfFileInfo, HfPortError, HfQuantInfo, HfRepoInfo, HfSearchOptions,
    HfSearchResult, HfSortField,
};

use crate::bootstrap::{CliContext, test_context};

/// A context over an empty library in `dir`, whose Hub is `hub`.
pub(super) async fn context(dir: &Path, hub: &Arc<Hub>) -> CliContext {
    gglib_core::paths::isolate_data_root();
    let mut ctx = test_context(dir).await;
    ctx.hf_client = Arc::clone(hub) as Arc<dyn HfClientPort>;
    ctx
}

/// The rule under a listing's heading.
pub(super) const RULE: &str =
    "────────────────────────────────────────────────────────────────────────────────";

/// `lines`, each ended.
pub(super) fn text(lines: &[&str]) -> String {
    lines.join("\n") + "\n"
}

#[derive(Default)]
pub(super) struct Hub {
    hits: Vec<HfRepoInfo>,
    /// Refuses every search.
    limited: bool,
    asked: Mutex<Vec<HfSearchOptions>>,
}

/// What a search asked the Hub: the query, the limit and the order, and
/// whether it left the page, the direction and the size bounds alone.
pub(super) type Asked = (Option<String>, u32, HfSortField, bool);

fn hit(id: &str, downloads: u64, likes: u64, description: Option<&str>) -> HfRepoInfo {
    HfRepoInfo {
        model_id: id.to_string(),
        name: id.rsplit('/').next().unwrap_or(id).to_string(),
        author: id.split('/').next().map(str::to_string),
        downloads,
        likes,
        parameters_b: Some(3.836_021_856),
        description: description.map(str::to_string),
        last_modified: None,
        chat_template: None,
        tags: vec![],
    }
}

impl Hub {
    /// The three hits of the search recorded in `gglib-hf`'s
    /// `search_fixture.json`, as `gglib-hf` reads them. The first lists two
    /// quantizations, the second none, and the third's files cannot be listed.
    pub(super) fn recorded() -> Self {
        Self::holding(vec![
            hit("MaziyarPanahi/Phi-4-mini-instruct-GGUF", 139_847, 16, None),
            hit("unsloth/Phi-4-mini-instruct-GGUF", 117_114, 153, None),
            hit(
                "bartowski/microsoft_Phi-4-mini-instruct-GGUF",
                75_435,
                47,
                None,
            ),
        ])
    }

    /// Four hits with descriptions, which the Hub's search does not send
    /// today: one of 120 letters, one of 120 two-byte letters, a short one
    /// under an ID that names no model family, and an empty one.
    pub(super) fn described() -> Self {
        Self::holding(vec![
            hit("owner/described-GGUF", 1_234_567, 9, Some(&"d".repeat(120))),
            hit("owner/accented-GGUF", 999, 0, Some(&"é".repeat(120))),
            hit("owner/plain-phi", 1_000, 2, Some("short")),
            hit("owner/empty-GGUF", 12, 3, Some("")),
        ])
    }

    /// `count` hits, named by their number.
    pub(super) fn numbered(count: u64) -> Self {
        Self::holding(
            (1..=count)
                .map(|n| hit(&format!("o/n{n}"), n, n, None))
                .collect(),
        )
    }

    pub(super) fn holding(hits: Vec<HfRepoInfo>) -> Self {
        Self {
            hits,
            ..Self::default()
        }
    }

    pub(super) fn limited() -> Self {
        Self {
            limited: true,
            ..Self::default()
        }
    }

    /// Every search made so far, oldest first.
    pub(super) fn asked(&self) -> Vec<Asked> {
        let asked = self.asked.lock().unwrap();
        asked
            .iter()
            .map(|o| {
                let untouched = o.page == 0
                    && !o.sort_ascending
                    && o.min_params_b.is_none()
                    && o.max_params_b.is_none();
                (o.query.clone(), o.limit, o.sort_by, untouched)
            })
            .collect()
    }
}

#[async_trait]
impl HfClientPort for Hub {
    async fn search(&self, options: &HfSearchOptions) -> HfPortResult<HfSearchResult> {
        self.asked.lock().unwrap().push(options.clone());
        if self.limited {
            return Err(HfPortError::RateLimited);
        }
        Ok(HfSearchResult {
            items: self.hits.clone(),
            has_more: true,
            page: options.page,
        })
    }

    async fn list_quantizations(&self, model_id: &str) -> HfPortResult<Vec<HfQuantInfo>> {
        let names: &[&str] = match model_id {
            "MaziyarPanahi/Phi-4-mini-instruct-GGUF" => &["Q4_K_M", "Q8_0"],
            "unsloth/Phi-4-mini-instruct-GGUF" => &[],
            "bartowski/microsoft_Phi-4-mini-instruct-GGUF" => {
                return Err(HfPortError::RateLimited);
            }
            _ => &["Q6_K"],
        };
        Ok(names
            .iter()
            .map(|name| HfQuantInfo {
                name: (*name).to_string(),
                shard_count: 1,
                total_size: 1,
                file_paths: vec![],
            })
            .collect())
    }

    async fn list_projectors(&self, _: &str) -> HfPortResult<Vec<HfFileInfo>> {
        unimplemented!("a search and its quantizations are all this hub answers")
    }
    async fn list_gguf_files(&self, _: &str) -> HfPortResult<Vec<HfFileInfo>> {
        unimplemented!("a search and its quantizations are all this hub answers")
    }
    async fn get_quantization_files(&self, _: &str, _: &str) -> HfPortResult<Vec<HfFileInfo>> {
        unimplemented!("a search and its quantizations are all this hub answers")
    }
    async fn get_commit_sha(&self, _: &str) -> HfPortResult<String> {
        unimplemented!("a search and its quantizations are all this hub answers")
    }
    async fn get_model_info(&self, _: &str) -> HfPortResult<HfRepoInfo> {
        unimplemented!("a search and its quantizations are all this hub answers")
    }
    async fn read_head(
        &self,
        _model_id: &str,
        _path: &str,
        _max_bytes: u64,
    ) -> HfPortResult<Vec<u8>> {
        unimplemented!("a search and its quantizations are all this hub answers")
    }
    async fn file_at(&self, _model_id: &str, _path: &str) -> HfPortResult<Option<HfFileInfo>> {
        unimplemented!("a search and its quantizations are all this hub answers")
    }
}
