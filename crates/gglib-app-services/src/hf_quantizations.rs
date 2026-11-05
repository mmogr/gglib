//! A repository's variants as the `HuggingFace` browser shows them.

use gglib_core::domain::{HeadMetadata, MissingInput, VariantFit};
use gglib_core::download::{GgufFileRole, MemoryFit};
use gglib_core::ports::HfQuantInfo;
use gglib_core::ports::huggingface::largest_projector;

use crate::types::{HfImagePreview, HfProjector, HfQuantization, HfQuantizationsResponse};

/// The listing of `model_id`: each variant with its role, the projector a
/// download fetches with it, and whether it fits this machine, judged as
/// the picker judges it: its weights alone (the projector counted by
/// neither), with the head its role was read from, against `budget_bytes`,
/// the budget the picker uses (`download_budget`), and by its files alone
/// when `is_image`. A model with no head read, or a machine whose memory
/// could not be read, says what is missing rather than guessing.
pub(crate) fn quantizations_response(
    model_id: &str,
    quants: Vec<(HfQuantInfo, Option<HeadMetadata>)>,
    is_image: bool,
    image: Option<HfImagePreview>,
    budget_bytes: Option<u64>,
) -> HfQuantizationsResponse {
    let infos: Vec<HfQuantInfo> = quants.iter().map(|(q, _)| q.clone()).collect();
    let projector = largest_projector(&infos).map(|p| HfProjector {
        file_path: p.file_paths.first().cloned().unwrap_or_default(),
        size_bytes: p.total_size,
    });
    HfQuantizationsResponse {
        image,
        model_id: model_id.to_string(),
        quantizations: quants
            .into_iter()
            .map(|(q, head)| {
                let is_model = q.role.is_none_or(GgufFileRole::is_weights);
                let projector = is_model.then(|| projector.clone()).flatten();
                let fit = is_model
                    .then(|| variant_fit(head.as_ref(), is_image, q.total_size, budget_bytes));
                HfQuantization {
                    file_path: q.file_paths.first().cloned().unwrap_or_default(),
                    size_bytes: q.total_size,
                    #[allow(clippy::cast_precision_loss, reason = "a size in MiB for display")]
                    size_mb: q.total_size as f64 / 1_048_576.0,
                    is_sharded: q.is_sharded(),
                    shard_count: q
                        .is_sharded()
                        .then(|| u32::try_from(q.shard_count()).unwrap_or(u32::MAX)),
                    projector,
                    fit_word: fit.map(VariantFit::word),
                    fit,
                    role: q.role.unwrap_or_default(),
                    label: q.label,
                }
            })
            .collect(),
    }
}

/// Whether a model whose weights take `weights_bytes` fits in
/// `budget_bytes`, judged as the picker judges it.
fn variant_fit(
    head: Option<&HeadMetadata>,
    is_image: bool,
    weights_bytes: u64,
    budget_bytes: Option<u64>,
) -> VariantFit {
    let Some(budget_bytes) = budget_bytes else {
        return VariantFit::Unknown {
            missing: MissingInput::Budget,
        };
    };
    let Some(head) = head else {
        return VariantFit::Unknown {
            missing: MissingInput::TrainedContext,
        };
    };
    MemoryFit { budget_bytes }
        .judge(&head.metadata, is_image, weights_bytes)
        .1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quant(label: &str, shard_count: usize, total_size: u64) -> HfQuantInfo {
        HfQuantInfo {
            label: label.to_owned(),
            role: Some(GgufFileRole::Weights),
            total_size,
            file_paths: (0..shard_count)
                .map(|i| format!("m-{label}-{i}.gguf"))
                .collect(),
        }
    }

    fn projector(path: &str, total_size: u64) -> HfQuantInfo {
        HfQuantInfo {
            label: path.trim_end_matches(".gguf").to_owned(),
            role: Some(GgufFileRole::Projector),
            total_size,
            file_paths: vec![path.to_owned()],
        }
    }

    /// Each model carries the projector every download fetches, the largest
    /// whose header says so, and its size stays the weights alone.
    #[test]
    fn each_model_names_the_largest_projector() {
        let response = quantizations_response(
            "o/r",
            vec![
                (quant("Q4_K_M", 1, 4_000), None),
                (quant("Q8_0", 2, 8_000), None),
                (projector("X.mmproj-Q8_0.gguf", 600), None),
                (projector("mmproj-F16.gguf", 900), None),
            ],
            false,
            None,
            None,
        );

        assert_eq!(response.model_id, "o/r");
        let [q4, q8, small, large] = response.quantizations.as_slice() else {
            panic!("four variants were listed");
        };
        let fetched = Some(HfProjector {
            file_path: "mmproj-F16.gguf".to_owned(),
            size_bytes: 900,
        });
        assert_eq!(q4.projector, fetched);
        assert_eq!(q8.projector, fetched);
        assert_eq!(
            (small.projector.clone(), large.projector.clone()),
            (None, None)
        );
        assert_eq!(
            (q4.size_bytes, q4.is_sharded, q4.shard_count),
            (4_000, false, None)
        );
        assert_eq!(
            (q8.size_bytes, q8.is_sharded, q8.shard_count),
            (8_000, true, Some(2))
        );
        assert_eq!(q8.file_path, "m-Q8_0-0.gguf");
    }

    #[test]
    fn a_repository_without_projectors_names_none() {
        let response = quantizations_response(
            "o/r",
            vec![(quant("Q8_0", 1, 8_000), None)],
            false,
            None,
            None,
        );

        assert_eq!(response.quantizations[0].projector, None);
    }

    /// A fit is said for a model and not for a projector; with no memory
    /// reading it says so, with no head read it says so, and with both it
    /// is the picker's judgement of the files and the header.
    #[test]
    fn each_model_says_whether_it_fits_and_what_it_cannot_tell() {
        let head = |context_length: Option<u64>| HeadMetadata {
            metadata: gglib_core::domain::GgufMetadata {
                context_length,
                ..Default::default()
            },
            role: Some(GgufFileRole::Weights),
            reached_tokenizer: true,
            complete: true,
            bytes_used: 1,
            tensor_count: 1,
        };
        let budget = Some(32_u64 << 30);

        let no_memory = quantizations_response(
            "o/r",
            vec![(quant("Q8_0", 1, 8_000), Some(head(Some(4096))))],
            false,
            None,
            None,
        );
        assert_eq!(
            no_memory.quantizations[0].fit,
            Some(VariantFit::Unknown {
                missing: MissingInput::Budget
            })
        );

        let no_head = quantizations_response(
            "o/r",
            vec![
                (quant("Q8_0", 1, 8_000), None),
                (projector("mmproj-F16.gguf", 900), None),
            ],
            false,
            None,
            budget,
        );
        assert_eq!(
            no_head.quantizations[0].fit,
            Some(VariantFit::Unknown {
                missing: MissingInput::TrainedContext
            })
        );
        assert_eq!(
            no_head.quantizations[1].fit, None,
            "a projector is not loaded alone"
        );

        // A header with a trained context and no KV shape: the picker
        // cannot tell either.
        let judged = quantizations_response(
            "o/r",
            vec![(quant("Q8_0", 1, 8_000), Some(head(Some(4096))))],
            false,
            None,
            budget,
        );
        assert_eq!(
            judged.quantizations[0].fit,
            Some(VariantFit::Unknown {
                missing: MissingInput::KvShape
            })
        );
        assert_eq!(
            judged.quantizations[0].fit_word.as_deref(),
            Some("unknown: KV shape not read from the header")
        );
    }

    /// A head with a trained context and a KV shape, as a chat model's has.
    fn shaped_head(weights: bool) -> HeadMetadata {
        let keys = [
            ("general.architecture", "qwen3"),
            ("qwen3.block_count", "36"),
            ("qwen3.attention.head_count", "32"),
            ("qwen3.attention.head_count_kv", "8"),
            ("qwen3.attention.key_length", "128"),
            ("qwen3.attention.value_length", "128"),
        ];
        HeadMetadata {
            metadata: gglib_core::domain::GgufMetadata {
                architecture: Some("qwen3".to_owned()),
                context_length: Some(40_960),
                metadata: keys
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                    .collect(),
                ..Default::default()
            },
            role: Some(if weights {
                GgufFileRole::Weights
            } else {
                GgufFileRole::Projector
            }),
            reached_tokenizer: true,
            complete: true,
            bytes_used: 1,
            tensor_count: 1,
        }
    }

    /// A model's fit is the picker's judgement of its weights alone, the
    /// one `gglib model download <repo>` picks by: the projector fetched
    /// beside them, however large, counts for neither.
    #[test]
    fn a_models_fit_is_the_pickers_judgement_of_its_weights_alone() {
        let weights = 2_497_281_312;
        let budget = 8_u64 << 30;
        let listed = quantizations_response(
            "o/r",
            vec![
                (quant("Q4_K_M", 1, weights), Some(shaped_head(true))),
                (
                    projector("mmproj-F16.gguf", 6 << 30),
                    Some(shaped_head(false)),
                ),
            ],
            false,
            None,
            Some(budget),
        );

        let picker = MemoryFit {
            budget_bytes: budget,
        }
        .judge(&shaped_head(true).metadata, false, weights);
        assert!(matches!(
            listed.quantizations[0].fit,
            Some(VariantFit::Fits { ctx: Some(_) })
        ));
        assert_eq!(listed.quantizations[0].fit, Some(picker.1));
        assert_eq!(
            listed.quantizations[0]
                .projector
                .as_ref()
                .map(|p| p.size_bytes),
            Some(6 << 30),
            "the projector is still named"
        );
    }

    /// An image model is judged by its files alone, with no context to
    /// serve at, whether or not its companions could be looked up.
    #[test]
    fn an_image_model_is_judged_by_its_files_alone() {
        let judged = |weights: u64| {
            quantizations_response(
                "o/r",
                vec![(quant("Q8_0", 1, weights), Some(shaped_head(true)))],
                true,
                None,
                Some(16 << 30),
            )
            .quantizations
            .remove(0)
            .fit
        };

        assert_eq!(judged(1 << 30), Some(VariantFit::Fits { ctx: None }));
        assert!(matches!(
            judged(15 << 30),
            Some(VariantFit::TooBig {
                have_bytes,
                ..
            }) if have_bytes == 16 << 30
        ));
    }

    /// The browser's listing finds the projector among the variants by the
    /// head of each.
    #[tokio::test]
    async fn the_listing_served_reads_the_repository_projectors() {
        use crate::downloads::{DownloadDeps, DownloadOps};
        use crate::test_support::{MockDownloadManager, MockHfClient, MockToolSupportDetector};
        use std::sync::Arc;

        let ops = DownloadOps::new(DownloadDeps {
            downloads: Arc::new(MockDownloadManager::new()),
            hf: Arc::new(MockHfClient),
            tool_detector: Arc::new(MockToolSupportDetector),
            gguf_parser: Arc::new(gglib_core::ports::huggingface::fake_hub::RoleParser),
            models_directory: None,
        });

        let listed = ops.get_model_quantizations("o/r").await.unwrap();

        assert_eq!(
            listed.quantizations[0].projector,
            Some(HfProjector {
                file_path: "mmproj-F16.gguf".to_owned(),
                size_bytes: 7
            })
        );
    }
}
