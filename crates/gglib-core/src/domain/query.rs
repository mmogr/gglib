//! Model list query, filter, and sort types.
//!
//! This module is the **single source of truth** for model filtering and
//! sorting logic. Both the CLI (direct-mode) and the Axum HTTP handler
//! delegate here; the GUI sends HTTP query parameters that are deserialized
//! into [`ModelListQuery`] on the server side. No filter/sort logic is
//! duplicated in the frontend.

use serde::{Deserialize, Serialize};

use crate::domain::Model;

// ─────────────────────────────────────────────────────────────────────────────
// Sort / order enums
// ─────────────────────────────────────────────────────────────────────────────

/// The field to sort the model list by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelSortBy {
    /// Sort by when the model was added (most recent first by default).
    #[default]
    AddedAt,
    /// Sort alphabetically by model name.
    Name,
    /// Sort by parameter count (in billions).
    ParamCount,
    /// Sort by the most recent token-generation throughput from benchmarks.
    LatestTgTps,
}

/// Direction for sorting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    /// Largest / most-recent first.
    #[default]
    Desc,
    /// Smallest / oldest first.
    Asc,
}

// ─────────────────────────────────────────────────────────────────────────────
// Query struct
// ─────────────────────────────────────────────────────────────────────────────

/// Complete filter + sort specification for the model list.
///
/// All filter fields are optional — absent means "no constraint applied".
/// `sort_by` and `order` always have sensible defaults (`AddedAt`, `Desc`).
///
/// This struct is used in three contexts:
/// 1. **HTTP handler**: `Query<ModelListQueryParams>` is converted here.
/// 2. **CLI direct-mode**: CLI flags are parsed directly into this struct.
/// 3. **CLI proxy-mode**: serialised as HTTP query parameters sent to the daemon.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelListQuery {
    /// Field to sort results by.
    #[serde(default)]
    pub sort_by: ModelSortBy,
    /// Sort direction.
    #[serde(default)]
    pub order: SortOrder,
    /// Inclusive minimum parameter count in billions (`param_count_b >= min_params`).
    pub min_params: Option<f64>,
    /// Inclusive maximum parameter count in billions (`param_count_b <= max_params`).
    pub max_params: Option<f64>,
    /// Inclusive minimum context length.
    pub min_context: Option<f64>,
    /// Inclusive maximum context length.
    pub max_context: Option<f64>,
    /// Quantization allowlist. A model passes if its quantization matches
    /// any listed value (case-sensitive). Models with no quantization are
    /// excluded when this filter is active.
    pub quantizations: Option<Vec<String>>,
    /// Required tags. A model passes only if it has **all** listed tags.
    pub tags: Option<Vec<String>>,
    /// Inclusive minimum `latest_tg_tps`. Models with no benchmark data are
    /// **excluded** when either speed bound is set.
    pub min_speed: Option<f64>,
    /// Inclusive maximum `latest_tg_tps`. Models with no benchmark data are
    /// **excluded** when either speed bound is set.
    pub max_speed: Option<f64>,
}

// ─────────────────────────────────────────────────────────────────────────────
// apply_query
// ─────────────────────────────────────────────────────────────────────────────

/// Apply a [`ModelListQuery`] to a model list, returning a filtered and sorted
/// copy.
///
/// ## Filter rules
///
/// - **`min_params`/`max_params`**: model's `param_count_b` must fall within
///   the range (inclusive).
/// - **`min_context`/`max_context`**: model's `context_length` must fall
///   within the range when it is set. Models without a context length are
///   **not** excluded by a context range filter.
/// - **`quantizations`**: model's quantization must match one of the listed
///   values. Models with no quantization are excluded when this filter is
///   active.
/// - **`tags`**: model must have *all* listed tags (AND semantics).
/// - **`min_speed`/`max_speed`**: model's `benchmark_summary.latest_tg_tps`
///   must be within the range. Models with no benchmark data are **excluded**
///   when either speed bound is active.
///
/// ## Sort behaviour
///
/// Default: `AddedAt Desc` (most recently added first). When sorting by
/// `LatestTgTps`, models without benchmark data sort **last** in both
/// ascending and descending orders.
#[must_use]
pub fn apply_query(mut models: Vec<Model>, query: &ModelListQuery) -> Vec<Model> {
    models.retain(|m| matches_query(m, query));

    match (query.sort_by, query.order) {
        (ModelSortBy::Name, SortOrder::Asc) => {
            models.sort_by(|a, b| a.name.cmp(&b.name));
        }
        (ModelSortBy::Name, SortOrder::Desc) => {
            models.sort_by(|a, b| b.name.cmp(&a.name));
        }
        (ModelSortBy::ParamCount, SortOrder::Asc) => {
            models.sort_by(|a, b| {
                a.param_count_b
                    .partial_cmp(&b.param_count_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        (ModelSortBy::ParamCount, SortOrder::Desc) => {
            models.sort_by(|a, b| {
                b.param_count_b
                    .partial_cmp(&a.param_count_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        (ModelSortBy::LatestTgTps, SortOrder::Asc) => {
            models.sort_by(|a, b| cmp_tps_asc(tps(a), tps(b)));
        }
        (ModelSortBy::LatestTgTps, SortOrder::Desc) => {
            models.sort_by(|a, b| cmp_tps_desc(tps(a), tps(b)));
        }
        (ModelSortBy::AddedAt, SortOrder::Asc) => {
            models.sort_by_key(|a| a.added_at);
        }
        (ModelSortBy::AddedAt, SortOrder::Desc) => {
            models.sort_by_key(|b| std::cmp::Reverse(b.added_at));
        }
    }

    models
}

/// Returns `true` when `model` satisfies all active filter constraints.
fn matches_query(m: &Model, query: &ModelListQuery) -> bool {
    // Param range
    if let Some(min) = query.min_params {
        if m.param_count_b < min {
            return false;
        }
    }
    if let Some(max) = query.max_params {
        if m.param_count_b > max {
            return false;
        }
    }

    // Context range — models without a context length pass through
    if let Some(ctx) = m.context_length {
        #[allow(clippy::cast_precision_loss)]
        let ctx_f = ctx as f64;
        if let Some(min) = query.min_context {
            if ctx_f < min {
                return false;
            }
        }
        if let Some(max) = query.max_context {
            if ctx_f > max {
                return false;
            }
        }
    }

    // Quantization allowlist
    if let Some(quants) = &query.quantizations {
        if !quants.is_empty() {
            match &m.quantization {
                Some(q) if quants.contains(q) => {}
                _ => return false,
            }
        }
    }

    // Tags — model must carry ALL listed tags
    if let Some(tags) = &query.tags {
        if !tags.is_empty() && !tags.iter().all(|t| m.tags.contains(t)) {
            return false;
        }
    }

    // Speed filter — models without benchmark data are excluded when active
    let speed_active = query.min_speed.is_some() || query.max_speed.is_some();
    if speed_active {
        match m.benchmark_summary.as_ref().and_then(|s| s.latest_tg_tps) {
            None => return false,
            Some(v) => {
                if query.min_speed.is_some_and(|min| v < min) {
                    return false;
                }
                if query.max_speed.is_some_and(|max| v > max) {
                    return false;
                }
            }
        }
    }

    true
}

/// Extract the latest TPS from a model's benchmark summary.
fn tps(m: &Model) -> Option<f64> {
    m.benchmark_summary.as_ref()?.latest_tg_tps
}

/// Compare optional TPS values ascending; `None` sorts last.
fn cmp_tps_asc(a: Option<f64>, b: Option<f64>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
    }
}

/// Compare optional TPS values descending; `None` sorts last.
fn cmp_tps_desc(a: Option<f64>, b: Option<f64>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => b.partial_cmp(&a).unwrap_or(std::cmp::Ordering::Equal),
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::benchmark::ModelBenchmarkSummary;
    use crate::ModelCapabilities;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn make_model(id: i64, name: &str, params: f64) -> Model {
        Model {
            id,
            name: name.to_string(),
            file_path: PathBuf::from(format!("/models/{name}.gguf")),
            param_count_b: params,
            architecture: None,
            quantization: None,
            context_length: None,
            expert_count: None,
            expert_used_count: None,
            expert_shared_count: None,
            metadata: HashMap::new(),
            added_at: Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: vec![],
            capabilities: ModelCapabilities::default(),
            inference_defaults: None,
            benchmark_summary: None,
        }
    }

    fn with_quant(mut m: Model, quant: &str) -> Model {
        m.quantization = Some(quant.to_string());
        m
    }

    fn with_tags(mut m: Model, tags: &[&str]) -> Model {
        m.tags = tags.iter().map(ToString::to_string).collect();
        m
    }

    fn with_tps(mut m: Model, tps: f64) -> Model {
        use chrono::Utc;
        m.benchmark_summary = Some(ModelBenchmarkSummary {
            model_id: m.id,
            best_tg_tps: Some(tps),
            best_pp_tps: None,
            latest_tg_tps: Some(tps),
            latest_pp_tps: None,
            latest_backend: None,
            perf_run_count: 1,
            compare_run_count: 0,
            last_benchmarked_at: Utc::now(),
            updated_at: Utc::now(),
        });
        m
    }

    fn models() -> Vec<Model> {
        vec![
            with_quant(make_model(1, "alpha", 7.0), "Q4_K_M"),
            with_quant(make_model(2, "beta", 13.0), "Q8_0"),
            with_quant(make_model(3, "gamma", 70.0), "Q4_K_M"),
        ]
    }

    #[test]
    fn default_query_preserves_all_models() {
        let result = apply_query(models(), &ModelListQuery::default());
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn param_range_filters_correctly() {
        let query = ModelListQuery {
            min_params: Some(8.0),
            max_params: Some(20.0),
            ..Default::default()
        };
        let result = apply_query(models(), &query);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "beta");
    }

    #[test]
    fn quantization_filter_keeps_matching() {
        let query = ModelListQuery {
            quantizations: Some(vec!["Q4_K_M".to_string()]),
            ..Default::default()
        };
        let result = apply_query(models(), &query);
        assert_eq!(result.len(), 2);
        assert!(
            result
                .iter()
                .all(|m| m.quantization.as_deref() == Some("Q4_K_M"))
        );
    }

    #[test]
    fn tag_filter_uses_and_semantics() {
        let tagged = vec![
            with_tags(make_model(1, "a", 7.0), &["chat", "code"]),
            with_tags(make_model(2, "b", 13.0), &["chat"]),
            with_tags(make_model(3, "c", 70.0), &["code"]),
        ];
        let query = ModelListQuery {
            tags: Some(vec!["chat".to_string(), "code".to_string()]),
            ..Default::default()
        };
        let result = apply_query(tagged, &query);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "a");
    }

    #[test]
    fn speed_filter_excludes_models_without_benchmark() {
        let ms = vec![
            with_tps(make_model(1, "fast", 7.0), 80.0),
            make_model(2, "no-bench", 13.0), // no benchmark
            with_tps(make_model(3, "slow", 70.0), 10.0),
        ];
        let query = ModelListQuery {
            min_speed: Some(50.0),
            ..Default::default()
        };
        let result = apply_query(ms, &query);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "fast");
    }

    #[test]
    fn sort_by_name_asc() {
        let query = ModelListQuery {
            sort_by: ModelSortBy::Name,
            order: SortOrder::Asc,
            ..Default::default()
        };
        let result = apply_query(models(), &query);
        assert_eq!(result[0].name, "alpha");
        assert_eq!(result[1].name, "beta");
        assert_eq!(result[2].name, "gamma");
    }

    #[test]
    fn sort_by_tps_desc_puts_none_last() {
        let ms = vec![
            with_tps(make_model(1, "fast", 7.0), 80.0),
            make_model(2, "no-bench", 13.0),
            with_tps(make_model(3, "slow", 70.0), 10.0),
        ];
        let query = ModelListQuery {
            sort_by: ModelSortBy::LatestTgTps,
            order: SortOrder::Desc,
            ..Default::default()
        };
        let result = apply_query(ms, &query);
        assert_eq!(result[0].name, "fast");
        assert_eq!(result[1].name, "slow");
        assert_eq!(result[2].name, "no-bench");
    }

    #[test]
    fn empty_quantizations_vec_passes_all() {
        let query = ModelListQuery {
            quantizations: Some(vec![]),
            ..Default::default()
        };
        let result = apply_query(models(), &query);
        assert_eq!(result.len(), 3);
    }
}
