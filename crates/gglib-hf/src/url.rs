//! URL construction helpers for `HuggingFace` API.
//!
//! This module provides pure functions for building `HuggingFace` API URLs,
//! ensuring consistent URL construction across all API calls.

use crate::models::{HfConfig, HfRepoRef};
use gglib_core::ports::huggingface::{HfModelKind, HfSearchOptions, HfSortField};
use url::Url;

/// Fields to explicitly expand in API requests.
const EXPAND_FIELDS: &[&str] = &["siblings", "gguf", "likes", "downloads", "tags"];

/// Build the expand parameters string for API URLs.
fn build_expand_params() -> String {
    EXPAND_FIELDS
        .iter()
        .map(|field| format!("expand[]={field}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// The Hub's own name for a sort order, as its `sort` parameter takes it.
const fn sort_param(field: HfSortField) -> &'static str {
    match field {
        HfSortField::Downloads => "downloads",
        HfSortField::Likes => "likes",
        HfSortField::Modified => "lastModified",
        HfSortField::Created => "createdAt",
        HfSortField::Alphabetical => "id",
    }
}

/// The filter that keeps a search to GGUF repositories of one kind of model.
///
/// A chat search asks for the GGUF library and the text-generation pipeline.
/// An image search asks for the `gguf` and `text-to-image` tags instead: the
/// GGUF library with the text-to-image pipeline answered diffusers
/// repositories holding no GGUF file (probed 2026-10-09), and the two tags
/// answered GGUF image repositories only.
const fn kind_filter(kind: HfModelKind) -> &'static str {
    match kind {
        HfModelKind::Chat => "library=gguf&pipeline_tag=text-generation",
        HfModelKind::Image => "filter=gguf&filter=text-to-image",
    }
}

/// Build a search URL with all required parameters.
pub(crate) fn build_search_url(config: &HfConfig, query: &HfSearchOptions) -> Url {
    let direction = if query.sort_ascending { "1" } else { "-1" };

    let mut url = config.base_url.clone();

    let query_string = format!(
        "{}&{}&sort={}&direction={}&limit={}&p={}",
        kind_filter(query.kind),
        build_expand_params(),
        sort_param(query.sort_by),
        direction,
        query.limit.clamp(1, 100),
        query.page
    );

    url.set_query(Some(&query_string));

    // Always add "GGUF" to filter for repos that actually contain GGUF files
    if let Some(ref q) = query.query {
        let search = if q.to_lowercase().contains("gguf") {
            q.trim().to_string()
        } else {
            format!("{} GGUF", q.trim())
        };

        let current = url.query().unwrap_or("");
        url.set_query(Some(&format!(
            "{current}&search={}",
            urlencoding::encode(&search)
        )));
    } else {
        let current = url.query().unwrap_or("");
        url.set_query(Some(&format!("{current}&search=GGUF")));
    }

    url
}

/// Build a URL for the model tree endpoint.
pub(crate) fn build_tree_url(config: &HfConfig, repo: &HfRepoRef, path: Option<&str>) -> Url {
    let mut url = config.base_url.clone();

    let tree_path = path.map_or_else(
        || format!("{}/tree/main", repo.id()),
        |p| format!("{}/tree/main/{p}", repo.id()),
    );

    let base_path = url.path().trim_end_matches('/');
    url.set_path(&format!("{base_path}/{tree_path}"));

    url
}

/// Build a URL for the model info endpoint.
pub(crate) fn build_model_info_url(config: &HfConfig, repo: &HfRepoRef) -> Url {
    let mut url = config.base_url.clone();

    let base_path = url.path().trim_end_matches('/');
    url.set_path(&format!("{base_path}/{}", repo.id()));

    url
}

/// Build the URL of the endpoint that looks files up by path, on `main`.
pub(crate) fn build_paths_info_url(config: &HfConfig, repo: &HfRepoRef) -> Url {
    let mut url = config.base_url.clone();

    let base_path = url.path().trim_end_matches('/');
    url.set_path(&format!("{base_path}/{}/paths-info/main", repo.id()));

    url
}

/// Build the URL that serves the bytes of `file_path` on `main`, on the Hub
/// the client is configured for.
///
/// The address [`build_file_url`] gives the native downloader, with the
/// Hub's root taken from the configured API base (its path less the
/// `/api/models` the API lives under), so the default configuration gives
/// exactly the downloader's URL.
pub(crate) fn build_resolve_url(config: &HfConfig, repo: &HfRepoRef, file_path: &str) -> Url {
    let mut url = config.base_url.clone();

    let base_path = url.path().trim_end_matches('/');
    let root = base_path.strip_suffix("/api/models").unwrap_or(base_path);
    url.set_path(&format!("{root}/{}/resolve/main/{file_path}", repo.id()));
    url.set_query(None);

    url
}

/// Build the URL that serves a file's bytes, from a bare `owner/name` repo ID.
///
/// This is the resolve endpoint: `HuggingFace` answers it with a redirect to the
/// CDN for LFS objects, and the CDN honours `Range` requests. `revision` may be
/// a branch, tag, or commit SHA; `None` means `main`.
///
/// Takes the repo ID as a string rather than an [`HfRepoRef`] so callers outside
/// this crate — the native downloader in `gglib-download` — can reach it without
/// the repo types becoming public API.
pub fn build_file_url(repo_id: &str, file_path: &str, revision: Option<&str>) -> String {
    let rev = revision.unwrap_or("main");
    format!("https://huggingface.co/{repo_id}/resolve/{rev}/{file_path}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> HfConfig {
        HfConfig::default()
    }

    #[test]
    fn test_build_expand_params() {
        let params = build_expand_params();
        assert!(params.contains("expand[]=siblings"));
        assert!(params.contains("expand[]=gguf"));
        assert!(params.contains("expand[]=likes"));
        assert!(params.contains("expand[]=downloads"));
        assert!(params.contains("expand[]=tags"));
        assert_eq!(params.matches("expand[]=").count(), 5);
    }

    #[test]
    fn test_build_search_url_default() {
        let config = default_config();
        let query = HfSearchOptions::new();

        let url = build_search_url(&config, &query);
        let url_str = url.as_str();

        assert!(url_str.starts_with("https://huggingface.co/api/models"));
        assert!(url_str.contains("library=gguf"));
        assert!(url_str.contains("pipeline_tag=text-generation"));
        assert!(url_str.contains("sort=downloads"));
        assert!(url_str.contains("direction=-1"));
        assert!(url_str.contains("limit=30"));
        assert!(url_str.contains("p=0"));
        assert!(url_str.contains("search=GGUF"));
        assert!(url_str.contains("expand[]=likes"));
    }

    /// An image search asks for the `gguf` and `text-to-image` tags, and
    /// neither the GGUF library nor the text-generation pipeline.
    #[test]
    fn an_image_search_asks_for_the_gguf_and_text_to_image_tags() {
        let query = HfSearchOptions::new().with_kind(HfModelKind::Image);

        let url = build_search_url(&default_config(), &query);

        let filters: Vec<String> = url
            .query_pairs()
            .filter(|(key, _)| key == "filter")
            .map(|(_, value)| value.into_owned())
            .collect();
        assert_eq!(filters, ["gguf", "text-to-image"]);
        assert!(
            url.query_pairs()
                .all(|(key, _)| key != "library" && key != "pipeline_tag"),
            "{url}"
        );
        assert!(url.as_str().contains("search=GGUF"), "{url}");
    }

    /// A chat search, the default, asks as it always has and names no tag.
    #[test]
    fn a_chat_search_asks_for_the_gguf_library_and_text_generation() {
        let url = build_search_url(&default_config(), &HfSearchOptions::new());

        let pair = |name: &str| {
            url.query_pairs()
                .find_map(|(key, value)| (key == name).then(|| value.into_owned()))
        };
        assert_eq!(pair("library").as_deref(), Some("gguf"));
        assert_eq!(pair("pipeline_tag").as_deref(), Some("text-generation"));
        assert_eq!(pair("filter"), None);
    }

    #[test]
    fn test_build_search_url_with_query() {
        let config = default_config();
        let query = HfSearchOptions::new().with_query("llama");

        let url = build_search_url(&config, &query);
        let url_str = url.as_str();

        assert!(url_str.contains("search=llama%20GGUF"));
    }

    #[test]
    fn test_build_search_url_with_gguf_in_query() {
        let config = default_config();
        let query = HfSearchOptions::new().with_query("llama GGUF models");

        let url = build_search_url(&config, &query);
        let url_str = url.as_str();

        // Should NOT double-add GGUF
        assert!(!url_str.contains("GGUF%20GGUF"));
    }

    #[test]
    fn test_build_search_url_with_sort() {
        let config = default_config();
        let query = HfSearchOptions::new().with_sort(HfSortField::Likes, true);

        let url = build_search_url(&config, &query);
        let url_str = url.as_str();

        assert!(url_str.contains("sort=likes"));
        assert!(url_str.contains("direction=1")); // ascending
    }

    /// Each order reaches the Hub under its own name: none is sent as
    /// another's, and so none is sorted by downloads but downloads.
    #[test]
    fn every_sort_field_is_sent_as_its_own_hub_parameter() {
        let sent = [
            (HfSortField::Downloads, "downloads"),
            (HfSortField::Likes, "likes"),
            (HfSortField::Modified, "lastModified"),
            (HfSortField::Created, "createdAt"),
            (HfSortField::Alphabetical, "id"),
        ];

        for (field, param) in sent {
            let query = HfSearchOptions::new().with_sort(field, false);
            let url = build_search_url(&default_config(), &query);
            let sort = url
                .query_pairs()
                .find_map(|(key, value)| (key == "sort").then(|| value.into_owned()));
            assert_eq!(sort.as_deref(), Some(param), "{field:?}");
        }
    }

    #[test]
    fn test_build_search_url_clamps_limit() {
        let config = default_config();

        // Test upper bound
        let query = HfSearchOptions {
            limit: 999,
            ..Default::default()
        };
        let url = build_search_url(&config, &query);
        assert!(url.as_str().contains("limit=100"));

        // Test lower bound
        let query = HfSearchOptions {
            limit: 0,
            ..Default::default()
        };
        let url = build_search_url(&config, &query);
        assert!(url.as_str().contains("limit=1"));
    }

    #[test]
    fn test_build_tree_url_root() {
        let config = default_config();
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let url = build_tree_url(&config, &repo, None);

        assert_eq!(
            url.as_str(),
            "https://huggingface.co/api/models/TheBloke/Llama-2-7B-GGUF/tree/main"
        );
    }

    #[test]
    fn test_build_tree_url_subdir() {
        let config = default_config();
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let url = build_tree_url(&config, &repo, Some("Q4_K_M"));

        assert_eq!(
            url.as_str(),
            "https://huggingface.co/api/models/TheBloke/Llama-2-7B-GGUF/tree/main/Q4_K_M"
        );
    }

    #[test]
    fn test_build_model_info_url() {
        let config = default_config();
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let url = build_model_info_url(&config, &repo);

        assert_eq!(
            url.as_str(),
            "https://huggingface.co/api/models/TheBloke/Llama-2-7B-GGUF"
        );
    }

    /// A file is looked up by path at the repository's `paths-info` on main.
    #[test]
    fn the_paths_info_url_is_the_repositorys_on_main() {
        let repo = HfRepoRef::new("Comfy-Org", "Qwen-Image-2.1");

        let url = build_paths_info_url(&default_config(), &repo);

        assert_eq!(
            url.as_str(),
            "https://huggingface.co/api/models/Comfy-Org/Qwen-Image-2.1/paths-info/main"
        );
    }

    /// A head is read from the very address the native downloader fetches
    /// the whole file from, a file in a folder included.
    #[test]
    fn a_head_is_read_where_the_downloader_reads_the_file() {
        let repo = HfRepoRef::new("Comfy-Org", "Qwen-Image-2.1");
        let path = "vae/qwen_image_2.1_vae_bf16.safetensors";

        let url = build_resolve_url(&default_config(), &repo, path);

        assert_eq!(
            url.as_str(),
            build_file_url("Comfy-Org/Qwen-Image-2.1", path, None)
        );
    }

    /// A Hub at another address is asked at its own root.
    #[test]
    fn a_configured_hub_serves_files_from_its_own_root() {
        let config = HfConfig {
            base_url: Url::parse("http://127.0.0.1:9/api/models").unwrap(),
            ..HfConfig::default()
        };
        let repo = HfRepoRef::new("leejet", "FLUX.1-schnell-gguf");

        let url = build_resolve_url(&config, &repo, "flux1-schnell-q8_0.gguf");

        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:9/leejet/FLUX.1-schnell-gguf/resolve/main/flux1-schnell-q8_0.gguf"
        );
    }

    /// The resolve-endpoint shape the native downloader depends on: `None`
    /// revision means `main`, and an explicit one is substituted verbatim.
    #[test]
    fn test_build_file_url() {
        assert_eq!(
            build_file_url("TheBloke/Llama-2-7B-GGUF", "llama-2-7b.Q4_K_M.gguf", None),
            "https://huggingface.co/TheBloke/Llama-2-7B-GGUF/resolve/main/llama-2-7b.Q4_K_M.gguf"
        );

        assert_eq!(
            build_file_url("TheBloke/Llama-2-7B-GGUF", "model.gguf", Some("abc123")),
            "https://huggingface.co/TheBloke/Llama-2-7B-GGUF/resolve/abc123/model.gguf"
        );
    }
}
