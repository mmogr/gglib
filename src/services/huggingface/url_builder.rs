//! URL construction helpers for HuggingFace API.
//!
//! This module provides pure functions for building HuggingFace API URLs,
//! ensuring consistent URL construction across all API calls.

use super::models::{HfConfig, HfRepoRef, HfSearchQuery};
use url::Url;

/// Fields to explicitly expand in API requests.
///
/// When using `expand[]` parameters, HuggingFace API only returns
/// explicitly requested fields. We request all fields we need to
/// ensure consistent data across all API calls.
const EXPAND_FIELDS: &[&str] = &["siblings", "gguf", "likes", "downloads"];

/// Build the expand parameters string for API URLs.
///
/// Returns a string like `expand[]=siblings&expand[]=gguf&expand[]=likes&expand[]=downloads`
pub fn build_expand_params() -> String {
    EXPAND_FIELDS
        .iter()
        .map(|field| format!("expand[]={}", field))
        .collect::<Vec<_>>()
        .join("&")
}

/// Build a search URL with all required parameters.
///
/// # Arguments
///
/// * `config` - HuggingFace client configuration
/// * `query` - Search query parameters
///
/// # Returns
///
/// Returns the full API URL for the search endpoint.
pub fn build_search_url(config: &HfConfig, query: &HfSearchQuery) -> Url {
    let direction = if query.sort_ascending { "1" } else { "-1" };

    // Start with the base URL
    let mut url = config.base_url.clone();

    // Build query string
    let query_string = format!(
        "library=gguf&pipeline_tag=text-generation&{}&sort={}&direction={}&limit={}&p={}",
        build_expand_params(),
        query.sort_by.as_api_param(),
        direction,
        query.limit.max(1).min(100), // Clamp to valid range
        query.page
    );

    url.set_query(Some(&query_string));

    // Add search query if present
    // CRITICAL: Always add "GGUF" to filter for repos that actually contain GGUF files
    if let Some(ref q) = query.query {
        let search = if q.to_lowercase().contains("gguf") {
            q.trim().to_string()
        } else {
            format!("{} GGUF", q.trim())
        };

        let current = url.query().unwrap_or("");
        url.set_query(Some(&format!(
            "{}&search={}",
            current,
            urlencoding::encode(&search)
        )));
    } else {
        let current = url.query().unwrap_or("");
        url.set_query(Some(&format!("{}&search=GGUF", current)));
    }

    url
}

/// Build a URL for the model tree endpoint.
///
/// # Arguments
///
/// * `config` - HuggingFace client configuration
/// * `repo` - Repository reference
/// * `path` - Optional subdirectory path within the repo
///
/// # Returns
///
/// Returns the full API URL for the tree endpoint.
pub fn build_tree_url(config: &HfConfig, repo: &HfRepoRef, path: Option<&str>) -> Url {
    let mut url = config.base_url.clone();

    // Extend the path
    let tree_path = match path {
        Some(p) => format!("{}/tree/main/{}", repo.id(), p),
        None => format!("{}/tree/main", repo.id()),
    };

    // Append to existing path
    let base_path = url.path().trim_end_matches('/');
    url.set_path(&format!("{}/{}", base_path, tree_path));

    url
}

/// Build a URL for the model info endpoint.
///
/// # Arguments
///
/// * `config` - HuggingFace client configuration
/// * `repo` - Repository reference
///
/// # Returns
///
/// Returns the full API URL for the model info endpoint.
pub fn build_model_info_url(config: &HfConfig, repo: &HfRepoRef) -> Url {
    let mut url = config.base_url.clone();

    // Append repo ID to path
    let base_path = url.path().trim_end_matches('/');
    url.set_path(&format!("{}/{}", base_path, repo.id()));

    url
}

/// Build a URL for downloading a file from a repository.
///
/// # Arguments
///
/// * `repo` - Repository reference
/// * `file_path` - Path to the file within the repository
/// * `revision` - Optional revision (commit SHA or branch name, defaults to "main")
///
/// # Returns
///
/// Returns the full URL for downloading the file.
pub fn build_download_url(repo: &HfRepoRef, file_path: &str, revision: Option<&str>) -> Url {
    let rev = revision.unwrap_or("main");
    Url::parse(&format!(
        "https://huggingface.co/{}/resolve/{}/{}",
        repo.id(),
        rev,
        file_path
    ))
    .expect("download URL construction should not fail")
}

/// Build a tree URL from a model ID string (convenience function).
///
/// This is a simplified interface for cases where you only have a model ID string.
/// For full control over configuration, use `build_tree_url` directly.
///
/// # Arguments
///
/// * `model_id` - HuggingFace model ID (e.g., "TheBloke/Llama-2-7B-GGUF")
/// * `path` - Optional subdirectory path within the repo
///
/// # Returns
///
/// Returns the full API URL for the tree endpoint as a String.
pub fn build_tree_url_simple(model_id: &str, path: Option<&str>) -> String {
    match path {
        Some(p) => format!("https://huggingface.co/api/models/{}/tree/main/{}", model_id, p),
        None => format!("https://huggingface.co/api/models/{}/tree/main", model_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::huggingface::models::HfSortField;

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
        assert_eq!(params.matches("expand[]=").count(), 4);
    }

    #[test]
    fn test_build_search_url_default() {
        let config = default_config();
        let query = HfSearchQuery::new();

        let url = build_search_url(&config, &query);
        let url_str = url.as_str();

        assert!(url_str.starts_with("https://huggingface.co/api/models"));
        assert!(url_str.contains("library=gguf"));
        assert!(url_str.contains("pipeline_tag=text-generation"));
        assert!(url_str.contains("sort=downloads"));
        assert!(url_str.contains("direction=-1"));
        assert!(url_str.contains("limit=30"));
        assert!(url_str.contains("p=0"));
        assert!(url_str.contains("search=GGUF")); // Default search
        assert!(url_str.contains("expand[]=likes"));
    }

    #[test]
    fn test_build_search_url_with_query() {
        let config = default_config();
        let query = HfSearchQuery::new().with_query("llama");

        let url = build_search_url(&config, &query);
        let url_str = url.as_str();

        // Should append GGUF to query
        assert!(url_str.contains("search=llama%20GGUF"));
    }

    #[test]
    fn test_build_search_url_with_gguf_in_query() {
        let config = default_config();
        let query = HfSearchQuery::new().with_query("llama GGUF models");

        let url = build_search_url(&config, &query);
        let url_str = url.as_str();

        // Should NOT double-add GGUF
        assert!(!url_str.contains("GGUF%20GGUF"));
    }

    #[test]
    fn test_build_search_url_with_sort() {
        let config = default_config();
        let query = HfSearchQuery::new().with_sort(HfSortField::Likes, true);

        let url = build_search_url(&config, &query);
        let url_str = url.as_str();

        assert!(url_str.contains("sort=likes"));
        assert!(url_str.contains("direction=1")); // ascending
    }

    #[test]
    fn test_build_search_url_with_pagination() {
        let config = default_config();
        let query = HfSearchQuery::new().with_page(5).with_limit(50);

        let url = build_search_url(&config, &query);
        let url_str = url.as_str();

        assert!(url_str.contains("p=5"));
        assert!(url_str.contains("limit=50"));
    }

    #[test]
    fn test_build_search_url_clamps_limit() {
        let config = default_config();

        // Test upper bound
        let query = HfSearchQuery {
            limit: 999,
            ..Default::default()
        };
        let url = build_search_url(&config, &query);
        assert!(url.as_str().contains("limit=100"));

        // Test lower bound
        let query = HfSearchQuery {
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

    #[test]
    fn test_build_download_url() {
        let repo = HfRepoRef::new("TheBloke", "Llama-2-7B-GGUF");

        let url = build_download_url(&repo, "llama-2-7b.Q4_K_M.gguf", None);
        assert_eq!(
            url.as_str(),
            "https://huggingface.co/TheBloke/Llama-2-7B-GGUF/resolve/main/llama-2-7b.Q4_K_M.gguf"
        );

        let url = build_download_url(&repo, "model.gguf", Some("abc123"));
        assert_eq!(
            url.as_str(),
            "https://huggingface.co/TheBloke/Llama-2-7B-GGUF/resolve/abc123/model.gguf"
        );
    }
}
