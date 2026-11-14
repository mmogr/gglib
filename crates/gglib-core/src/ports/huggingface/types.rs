//! Core-owned DTOs for `HuggingFace` operations.
//!
//! These types cross the boundary between `gglib-hf` and consumers.
//! They contain only the data needed by the core domain, not internal
//! `HuggingFace` API details.

use serde::{Deserialize, Serialize};

use super::variants::HELPER_FILES_REFUSAL;
use crate::download::GgufFileRole;

/// Information about a `HuggingFace` repository/model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfRepoInfo {
    /// Full model ID (e.g., "TheBloke/Llama-2-7B-GGUF")
    pub model_id: String,
    /// Model name (derived from ID)
    pub name: String,
    /// Author/organization
    pub author: Option<String>,
    /// Total download count
    pub downloads: u64,
    /// Like count
    pub likes: u64,
    /// Parameter count in billions (if known). A search's hit's is the
    /// Hub's, unless its file names contradict it; then the one its largest
    /// weights could hold ([`super::plausible_total`]).
    pub parameters_b: Option<f64>,
    /// Short description
    pub description: Option<String>,
    /// Last modified timestamp (ISO 8601)
    pub last_modified: Option<String>,
    /// Chat template (from `gguf.chat_template`, else `config.chat_template`)
    pub chat_template: Option<String>,
    /// The architecture the Hub read from the repository's GGUF header
    /// (`gguf.architecture`), e.g. `qwen3`
    pub architecture: Option<String>,
    /// The context length the model was trained for, as the Hub read it from
    /// the repository's GGUF header (`gguf.context_length`)
    pub context_length: Option<u64>,
    /// Model tags
    #[serde(default)]
    pub tags: Vec<String>,
    /// The paths of the repository's files the answer named (its
    /// `siblings`), which a search's hit is listed by without its file tree.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_paths: Vec<String>,
}

/// Information about a file in a `HuggingFace` repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfFileInfo {
    /// Path relative to repository root
    pub path: String,
    /// File size in bytes
    pub size: u64,
    /// Whether this is a GGUF file
    pub is_gguf: bool,
    /// Git LFS OID (SHA256 hash from `HuggingFace` tree API)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oid: Option<String>,
}

/// One variant a repository publishes: a set of GGUF files that are shards
/// of one another, named as its uploader named it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HfQuantInfo {
    /// The uploader's name for it (`UD-Q4_K_XL`, `Q3_K_P`, `BF16`): its file
    /// name less what every variant's name shares. A request names it.
    pub label: String,
    /// What its first file's header says it is; `None` when the header has
    /// not been read, which a listing never does. A variant that is not a
    /// draft, a helper or a projector is a model.
    pub role: Option<GgufFileRole>,
    /// Every file of it, in shard order.
    pub file_paths: Vec<String>,
    /// The size of every file of it, in bytes.
    pub total_size: u64,
}

impl HfQuantInfo {
    /// How many files it is split into.
    #[must_use]
    pub const fn shard_count(&self) -> usize {
        self.file_paths.len()
    }

    /// Whether it is split into several files.
    #[must_use]
    pub const fn is_sharded(&self) -> bool {
        self.shard_count() > 1
    }

    /// Why it is not downloaded as a model, when its header says it is a
    /// draft, a helper or a projector; `None` for a model, and for a
    /// variant whose header has not been read.
    #[must_use]
    pub fn refusal(&self) -> Option<String> {
        match self.role? {
            GgufFileRole::Weights => None,
            GgufFileRole::Sidecar => Some(format!(
                "{} is a draft or helper file, not a model: {HELPER_FILES_REFUSAL}",
                self.label
            )),
            GgufFileRole::Projector => Some(format!(
                "{} is a projector, not a model: a download of a model fetches its projector \
                 with it",
                self.label
            )),
        }
    }
}

/// Sort field options for `HuggingFace` model search.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum HfSortField {
    /// Sort by download count (default)
    #[default]
    Downloads,
    /// Sort by number of likes
    Likes,
    /// Sort by last modified date
    Modified,
    /// Sort by creation date
    Created,
    /// Sort alphabetically by name
    #[serde(rename = "id")]
    Alphabetical,
}

/// What a search looks for: models that chat, or models that draw.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum HfModelKind {
    /// GGUF repositories of models that chat, with or without images read
    /// alongside (the default).
    #[default]
    Chat,
    /// GGUF repositories of text-to-image models.
    Image,
}

/// Options for searching `HuggingFace` models.
#[derive(Debug, Clone, Default)]
pub struct HfSearchOptions {
    /// Search query string
    pub query: Option<String>,
    /// Minimum parameter count in billions
    pub min_params_b: Option<f64>,
    /// Maximum parameter count in billions
    pub max_params_b: Option<f64>,
    /// Maximum number of results
    pub limit: u32,
    /// Where to carry on from: the cursor the page before ended with, as
    /// [`HfSearchResult::next_cursor`] gave it. `None` asks for the first page.
    pub cursor: Option<String>,
    /// Sort field
    pub sort_by: HfSortField,
    /// Sort ascending (false = descending)
    pub sort_ascending: bool,
    /// Whether to look for models that chat or models that draw.
    pub kind: HfModelKind,
}

impl HfSearchOptions {
    /// Create new search options with defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            limit: 30,
            ..Default::default()
        }
    }

    /// Set the search query.
    #[must_use]
    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.query = Some(query.into());
        self
    }

    /// Set the result limit.
    #[must_use]
    pub const fn with_limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    /// Carry on from the page before's cursor.
    #[must_use]
    pub fn with_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }

    /// Set parameter size filters.
    #[must_use]
    pub const fn with_params_filter(mut self, min: Option<f64>, max: Option<f64>) -> Self {
        self.min_params_b = min;
        self.max_params_b = max;
        self
    }

    /// Set sort options.
    #[must_use]
    pub const fn with_sort(mut self, field: HfSortField, ascending: bool) -> Self {
        self.sort_by = field;
        self.sort_ascending = ascending;
        self
    }

    /// Set what kind of model to look for.
    #[must_use]
    pub const fn with_kind(mut self, kind: HfModelKind) -> Self {
        self.kind = kind;
        self
    }
}

/// Result of a `HuggingFace` model search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfSearchResult {
    /// Models matching the search
    pub items: Vec<HfRepoInfo>,
    /// The cursor the next page starts from, when the Hub has one: the
    /// Hub pages by cursor and ignores page numbers.
    pub next_cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_options_builder() {
        let opts = HfSearchOptions::new()
            .with_query("llama")
            .with_limit(50)
            .with_cursor("abc")
            .with_params_filter(Some(7.0), Some(70.0))
            .with_sort(HfSortField::Likes, true)
            .with_kind(HfModelKind::Image);

        assert_eq!(opts.query, Some("llama".to_string()));
        assert_eq!(opts.limit, 50);
        assert_eq!(opts.cursor.as_deref(), Some("abc"));
        assert_eq!(opts.min_params_b, Some(7.0));
        assert_eq!(opts.max_params_b, Some(70.0));
        assert_eq!(opts.sort_by, HfSortField::Likes);
        assert!(opts.sort_ascending);
        assert_eq!(opts.kind, HfModelKind::Image);
        assert_eq!(HfSearchOptions::new().kind, HfModelKind::Chat);
    }

    #[test]
    fn a_variant_is_sharded_when_it_holds_several_files() {
        let variant = |paths: &[&str]| HfQuantInfo {
            label: "Q8_0".to_string(),
            role: None,
            total_size: 1,
            file_paths: paths.iter().map(ToString::to_string).collect(),
        };
        assert!(!variant(&["model.gguf"]).is_sharded());
        assert_eq!(variant(&["a.gguf", "b.gguf", "c.gguf"]).shard_count(), 3);
        assert!(variant(&["a.gguf", "b.gguf"]).is_sharded());
    }

    #[test]
    fn only_a_variant_whose_head_says_it_is_no_model_is_refused() {
        let variant = |role| HfQuantInfo {
            label: "FastMTP-32K".to_string(),
            role,
            total_size: 1,
            file_paths: vec!["x.gguf".to_string()],
        };
        assert_eq!(variant(None).refusal(), None);
        assert_eq!(variant(Some(GgufFileRole::Weights)).refusal(), None);
        let draft = variant(Some(GgufFileRole::Sidecar)).refusal().unwrap();
        assert!(
            draft.starts_with("FastMTP-32K is a draft or helper file"),
            "{draft}"
        );
        assert!(draft.ends_with(HELPER_FILES_REFUSAL), "{draft}");
        assert!(variant(Some(GgufFileRole::Projector)).refusal().is_some());
    }
}
