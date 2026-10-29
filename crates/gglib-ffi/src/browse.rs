//! Finding a model to download: the shortlist gglib suggests, a search of
//! Hugging Face, and a repository's quantizations, each judged against the
//! device's budget.

use std::sync::Arc;

use futures_util::{StreamExt, TryStreamExt, stream};
use gglib_core::domain::recommendation::PHONE_SHORTLIST;
use gglib_core::ports::{
    GgufParserPort, HfClientPort, HfModelKind, HfQuantInfo, HfRepoInfo, HfSearchOptions,
};

use crate::errors::GglibError;
use crate::verdict::{FitVerdict, FitWhy, MissingInput, ModelShape};

/// How much of a quantization's first file is read to judge it: 64 KiB.
///
/// A converter writes the keys a fit needs (the trained context, the layer
/// and head counts and dimensions) before the tokenizer's arrays, which run
/// to megabytes. Read on 2026-10-29 from seven shortlist and split files on
/// the Hub, every one of them ended within the first 1.6 KB.
pub(crate) const FIT_HEAD_BYTES: u64 = 64 * 1024;

/// How many quantizations' heads are read at once.
const HEADS_AT_ONCE: usize = 4;

/// How many repositories a page of a search holds.
const SEARCH_PAGE: u32 = 30;

/// A model gglib suggests for the device.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Suggestion {
    /// The Hugging Face repository, `owner/name`.
    pub repo: String,
    /// The quantization to download from it.
    pub quantization: String,
    /// The size of its weights, in bytes.
    pub bytes: u64,
    /// Whether it fits the device.
    pub verdict: FitVerdict,
    /// What the verdict was reached from.
    pub why: FitWhy,
    /// Why this model, in a person's terms.
    pub rationale: String,
}

/// One repository a search found.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SearchHit {
    /// The repository, `owner/name`.
    pub repo: String,
    /// How many people liked it.
    pub likes: u64,
    /// How many times it was downloaded.
    pub downloads: u64,
    /// Its parameter count in billions, when the Hub knows it.
    pub parameters_b: Option<f64>,
    /// The context its model was trained for, when the Hub read it from the
    /// repository's GGUF header.
    pub trained_ctx: Option<u64>,
}

/// A page of a search.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SearchPage {
    /// The repositories on this page, in the Hub's order (most downloaded
    /// first).
    pub hits: Vec<SearchHit>,
    /// Where the next page starts, or `None` on the last.
    pub next_cursor: Option<String>,
}

/// One quantization a repository publishes.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct QuantOffer {
    /// Its name: `Q4_K_M`, `Q8_0` and so on.
    pub name: String,
    /// The size of every file of it, in bytes.
    pub total_bytes: u64,
    /// How many files it is split into.
    pub shard_count: u32,
    /// Whether it fits the device.
    pub verdict: FitVerdict,
    /// What the verdict was reached from.
    pub why: FitWhy,
}

/// The phone shortlist, each row with its verdict at `budget`, in the
/// table's order: largest first. Nothing is asked of the Hub.
pub(crate) fn suggestions(budget: Option<u64>) -> Vec<Suggestion> {
    PHONE_SHORTLIST
        .iter()
        .map(|row| {
            let (verdict, why) =
                ModelShape::of_candidate(row).judge(Some(row.weights_bytes), budget);
            Suggestion {
                repo: row.repo.to_owned(),
                quantization: row.quantization.to_owned(),
                bytes: row.weights_bytes,
                verdict,
                why,
                rationale: row.rationale.to_owned(),
            }
        })
        .collect()
}

/// A page of chat models matching `query`, from `cursor` or the first.
pub(crate) async fn search(
    hub: &dyn HfClientPort,
    query: &str,
    cursor: Option<String>,
) -> Result<SearchPage, GglibError> {
    let options = HfSearchOptions {
        cursor,
        ..HfSearchOptions::new()
            .with_query(query)
            .with_limit(SEARCH_PAGE)
            .with_kind(HfModelKind::Chat)
    };
    let found = hub.search(&options).await?;
    Ok(SearchPage {
        hits: found.items.into_iter().map(hit).collect(),
        next_cursor: found.next_cursor,
    })
}

fn hit(repo: HfRepoInfo) -> SearchHit {
    SearchHit {
        repo: repo.model_id,
        likes: repo.likes,
        downloads: repo.downloads,
        parameters_b: repo.parameters_b,
        trained_ctx: repo.context_length,
    }
}

/// Each quantization `repo` publishes, in the Hub's order, judged at
/// `budget` from the first [`FIT_HEAD_BYTES`] of its first file.
///
/// A head the Hub refuses fails the call with the Hub's error. A head that
/// is not a GGUF header leaves its quantization unknown.
pub(crate) async fn quantizations(
    hub: Arc<dyn HfClientPort>,
    parser: Arc<dyn GgufParserPort>,
    repo: &str,
    budget: Option<u64>,
) -> Result<Vec<QuantOffer>, GglibError> {
    let quants = hub.list_quantizations(repo).await?;
    stream::iter(quants)
        .map(|quant| offer(Arc::clone(&hub), Arc::clone(&parser), repo, quant, budget))
        .buffered(HEADS_AT_ONCE)
        .try_collect()
        .await
}

async fn offer(
    hub: Arc<dyn HfClientPort>,
    parser: Arc<dyn GgufParserPort>,
    repo: &str,
    quant: HfQuantInfo,
    budget: Option<u64>,
) -> Result<QuantOffer, GglibError> {
    let weights = Some(quant.total_size);
    let (verdict, why) = match quant.file_paths.first() {
        Some(first) => {
            let head = hub.read_head(repo, first, FIT_HEAD_BYTES).await?;
            let file_name = first.rsplit('/').next().unwrap_or(first);
            parser.parse_head(&head, file_name).map_or_else(
                |_| unreadable(weights, budget),
                |head| ModelShape::of_head(&head).judge(weights, budget),
            )
        }
        None => unreadable(weights, budget),
    };
    Ok(QuantOffer {
        name: quant.name,
        total_bytes: quant.total_size,
        shard_count: u32::try_from(quant.shard_count).unwrap_or(u32::MAX),
        verdict,
        why,
    })
}

/// The verdict on a quantization whose header could not be read: its
/// trained context is unknown, and no download makes it known.
fn unreadable(weights_bytes: Option<u64>, budget_bytes: Option<u64>) -> (FitVerdict, FitWhy) {
    (
        FitVerdict::Unknown {
            missing: MissingInput::TrainedContext,
            cut_off: false,
        },
        FitWhy {
            weights_bytes,
            kv_bytes_per_token: None,
            budget_bytes,
            trained_ctx: None,
        },
    )
}

#[cfg(test)]
#[path = "browse_tests.rs"]
mod tests;
