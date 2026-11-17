//! A repository's variants as the `HuggingFace` browser shows them.

use async_trait::async_trait;
use gglib_core::domain::{GgufMetadata, HeadMetadata, MissingInput, VariantFit};
use gglib_core::download::{
    Contender, FitJudge, GgufFileRole, HeadFacts, LoadFit, MemoryFit, VariantReader, pick_variant,
};
use gglib_core::ports::HfQuantInfo;
use gglib_core::ports::huggingface::largest_projector;

use crate::types::{HfImagePreview, HfProjector, HfQuantization, HfQuantizationsResponse};

/// The listing of `model_id`: each variant with its role, the projector a
/// download fetches with it, and whether it fits this machine, judged as
/// the picker judges it and a load counts it: its weights and what is loaded
/// beside them ([`beside_bytes`]), with the head its role was read from,
/// against `budget_bytes`, the budget the picker uses (`download_budget`),
/// and by its files alone when `is_image`. A model with no head read, or a
/// machine whose memory could not be read, says what is missing rather than
/// guessing.
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
    let beside = beside_bytes(&infos, image.as_ref());
    HfQuantizationsResponse {
        image,
        model_id: model_id.to_string(),
        quantizations: quants
            .into_iter()
            .map(|(q, head)| {
                let is_model = q.role.is_none_or(GgufFileRole::is_weights);
                let projector = is_model.then(|| projector.clone()).flatten();
                let fit = is_model.then(|| {
                    variant_fit(head.as_ref(), is_image, q.total_size, budget_bytes, beside)
                });
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
                    picked: false,
                }
            })
            .collect(),
    }
}

/// The heads a listing read, as the picker reads a repository: a head is
/// all it has of each variant, so a whole header is that head's pairs.
struct ListedHeads<'a> {
    heads: Vec<Option<&'a HeadMetadata>>,
    is_image: bool,
}

#[async_trait]
impl VariantReader for ListedHeads<'_> {
    async fn head(&self, index: usize) -> Option<HeadFacts> {
        let head = (*self.heads.get(index)?)?;
        Some(HeadFacts {
            head: head.clone(),
            image: self.is_image,
        })
    }

    async fn header(&self, index: usize) -> Option<GgufMetadata> {
        self.heads
            .get(index)
            .copied()
            .flatten()
            .map(|h| h.metadata.clone())
    }
}

/// The index in `quants` of the variant `gglib model download` would take,
/// chosen by `pick_variant` over the heads already read, against
/// `budget_bytes`; `None` for a repository with no weights, and `None` when
/// the heads cannot tell: a model's head that stops before its
/// `general.file_type` (llama-quantize appends it past the first 64 KiB),
/// where the download reads further and would choose differently. The
/// listing then asks the download's own picker
/// ([`gglib_download::download_pick`]). Each size is judged with `beside`
/// counted beside its weights, what a load counts ([`beside_bytes`]).
pub(crate) async fn listed_pick(
    quants: &[(HfQuantInfo, Option<HeadMetadata>)],
    is_image: bool,
    beside: u64,
    budget_bytes: Option<u64>,
) -> Option<usize> {
    let untold = |(q, head): &(HfQuantInfo, Option<HeadMetadata>)| {
        q.role.is_none_or(GgufFileRole::is_weights)
            && head
                .as_ref()
                .is_none_or(|h| h.metadata.file_type.is_none() && !h.complete)
    };
    if !is_image && quants.iter().any(untold) {
        return None;
    }
    let contenders: Vec<Contender> = quants
        .iter()
        .map(|(q, _)| Contender {
            size: q.total_size,
            role: q.role,
        })
        .collect();
    let reader = ListedHeads {
        heads: quants.iter().map(|(_, head)| head.as_ref()).collect(),
        is_image,
    };
    let judge = budget_bytes.map(|budget_bytes| load_fit(budget_bytes, beside));
    pick_variant(
        &contenders,
        &reader,
        judge.as_ref().map(|j| j as &dyn FitJudge),
    )
    .await
    .ok()
    .map(|pick| pick.index)
}

/// What a load of any size of a repository counts beside its weights: the
/// companions an image model draws with (`image`, its preview), else the
/// projector a download fetches with each size, which a load counts
/// (`resident_bytes`). An image family's compute margin is not counted: a
/// load reserves it only to decide whether a model may stay beside another.
pub(crate) fn beside_bytes(infos: &[HfQuantInfo], image: Option<&HfImagePreview>) -> u64 {
    image.map_or_else(
        || largest_projector(infos).map_or(0, |p| p.total_size),
        |preview| preview.companions.iter().map(|c| c.size_bytes).sum(),
    )
}

/// The listing with no context of the fit's named: what it says where the
/// user's `default-context-size` outranks the fit, so a load serves that.
/// Whether each size fits is unchanged; only the number goes.
pub(crate) fn name_no_context(response: &mut HfQuantizationsResponse) {
    for variant in &mut response.quantizations {
        if let Some(fit) = variant.fit.map(VariantFit::without_context) {
            variant.fit = Some(fit);
            variant.fit_word = Some(fit.word());
        }
    }
}

/// The picker's judge against `budget_bytes`, counting `beside` beside
/// every size's weights.
fn load_fit(budget_bytes: u64, beside: u64) -> LoadFit {
    MemoryFit { budget_bytes }.loading().beside(beside)
}

/// Whether a model whose weights take `weights_bytes` fits in
/// `budget_bytes` with `beside` loaded beside them, judged as the picker
/// judges it.
fn variant_fit(
    head: Option<&HeadMetadata>,
    is_image: bool,
    weights_bytes: u64,
    budget_bytes: Option<u64>,
    beside: u64,
) -> VariantFit {
    let Some(budget_bytes) = budget_bytes else {
        return VariantFit::Unknown {
            missing: MissingInput::Budget,
        };
    };
    let fit = load_fit(budget_bytes, beside);
    // An image model is judged by its files alone, header or no header: its
    // header holds no trained context to miss.
    if is_image {
        return fit.files_only(weights_bytes).1;
    }
    let Some(head) = head else {
        return VariantFit::Unknown {
            missing: MissingInput::TrainedContext,
        };
    };
    fit.judge(&head.metadata, false, weights_bytes).1
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gglib_core::ports::huggingface::fake_hub::{FakeHub, hub_file};

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

    /// Reads a file as a chat model's whose type, its first byte, lies past
    /// the first 64 KiB, as llama-quantize writes it.
    struct TypedPastTheHead;

    impl gglib_core::ports::GgufParserPort for TypedPastTheHead {
        fn parse(
            &self,
            path: &std::path::Path,
        ) -> Result<GgufMetadata, gglib_core::ports::GgufParseError> {
            gglib_core::ports::NoopGgufParser.parse(path)
        }
        fn detect_capabilities(
            &self,
            metadata: &GgufMetadata,
        ) -> gglib_core::ports::GgufCapabilities {
            gglib_core::ports::NoopGgufParser.detect_capabilities(metadata)
        }
        fn tensor_table(
            &self,
            path: &std::path::Path,
        ) -> Result<gglib_core::domain::TensorTable, gglib_core::ports::GgufParseError> {
            gglib_core::ports::NoopGgufParser.tensor_table(path)
        }
        fn tensor_table_of_head(
            &self,
            head: &[u8],
        ) -> Result<gglib_core::domain::TensorTable, gglib_core::ports::GgufParseError> {
            gglib_core::ports::NoopGgufParser.tensor_table_of_head(head)
        }
        fn parse_head(
            &self,
            head: &[u8],
            _file_name: &str,
        ) -> Result<HeadMetadata, gglib_core::ports::GgufParseError> {
            let mut parsed = shaped_head(true);
            parsed.complete = head.len() > 64 << 10;
            parsed.metadata.file_type = parsed
                .complete
                .then(|| gglib_core::domain::FileType(u32::from(head[0])));
            Ok(parsed)
        }
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

    /// A model's fit counts the projector a download fetches with it, as a
    /// load counts it: a size that fits alone at one context fits with its
    /// projector at less, or not at all, and the listing says the latter.
    /// The size itself is still the weights alone.
    #[test]
    fn a_models_fit_counts_the_projector_fetched_with_it() {
        let weights = 2_497_281_312;
        let budget = 8_u64 << 30;
        for projector_bytes in [3_u64 << 30, 6 << 30] {
            let listed = quantizations_response(
                "o/r",
                vec![
                    (quant("Q4_K_M", 1, weights), Some(shaped_head(true))),
                    (
                        projector("mmproj-F16.gguf", projector_bytes),
                        Some(shaped_head(false)),
                    ),
                ],
                false,
                None,
                Some(budget),
            );
            let alone = MemoryFit {
                budget_bytes: budget,
            };
            let with_projector = alone.loading().beside(projector_bytes).judge(
                &shaped_head(true).metadata,
                false,
                weights,
            );
            assert_eq!(listed.quantizations[0].fit, Some(with_projector.1));
            assert_ne!(
                with_projector.1,
                alone.judge(&shaped_head(true).metadata, false, weights).1,
                "{projector_bytes}: the projector must count"
            );
            assert_eq!(listed.quantizations[0].size_bytes, weights);
        }
    }

    /// With a `default-context-size` set a load serves that, so the listing
    /// names no context of the fit's; what fits and what does not is kept.
    #[test]
    fn a_set_default_context_size_names_no_context() {
        let mut listed = quantizations_response(
            "o/r",
            vec![
                (quant("Q4_K_M", 1, 2_497_281_312), Some(shaped_head(true))),
                (quant("F32", 1, 64 << 30), Some(shaped_head(true))),
            ],
            false,
            None,
            Some(8 << 30),
        );
        assert!(matches!(
            listed.quantizations[0].fit,
            Some(VariantFit::Fits { ctx: Some(_) })
        ));
        let too_big = listed.quantizations[1].fit;
        name_no_context(&mut listed);
        assert_eq!(
            listed.quantizations[0].fit,
            Some(VariantFit::Fits { ctx: None })
        );
        assert_eq!(listed.quantizations[0].fit_word.as_deref(), Some("fits"));
        assert_eq!(listed.quantizations[1].fit, too_big);
    }

    /// Beside an image model's weights a load counts every companion it
    /// draws with, present or not; beside a chat model's, the projector a
    /// download fetches.
    #[test]
    fn what_is_counted_beside_the_weights() {
        let companion = |size_bytes, present| crate::types::HfCompanion {
            role: gglib_core::domain::ComponentRole::Vae,
            repo: "o/vae".to_owned(),
            file_path: "vae.safetensors".to_owned(),
            size_bytes,
            present,
        };
        let preview = HfImagePreview {
            family: gglib_core::domain::ImageFamily::Flux1,
            companions: vec![companion(300, true), companion(700, false)],
            fetch_bytes: 700,
        };
        let infos = vec![quant("Q8_0", 1, 8_000), projector("mmproj-F16.gguf", 900)];
        assert_eq!(beside_bytes(&infos, Some(&preview)), 1_000);
        assert_eq!(beside_bytes(&infos, None), 900);
        assert_eq!(beside_bytes(&infos[..1], None), 0);
    }

    /// The listing's pick judges each size with the projector it is
    /// fetched with: one that fits only alone is passed over.
    #[tokio::test]
    async fn the_pick_counts_the_projector_fetched_with_it() {
        let typed = |file_type: u32| {
            let mut head = shaped_head(true);
            head.metadata.file_type = Some(gglib_core::domain::FileType(file_type));
            Some(head)
        };
        let listed = vec![
            (quant("Q4_K_M", 1, 4 << 30), typed(15)),
            (quant("Q8_0", 1, 7 << 30), typed(7)),
        ];
        let budget = Some(10_u64 << 30);
        assert_eq!(listed_pick(&listed, false, 0, budget).await, Some(1));
        assert_eq!(listed_pick(&listed, false, 2 << 30, budget).await, Some(0));
    }

    /// The listing marks the variant the download would take: the largest
    /// quantized one that fits, not a larger full-precision one; the
    /// smallest with no budget; none in a repository of helpers alone.
    #[tokio::test]
    async fn the_pick_is_the_downloads_from_the_heads_read() {
        let typed = |file_type: u32| {
            let mut head = shaped_head(true);
            head.metadata.file_type = Some(gglib_core::domain::FileType(file_type));
            // A 64 KiB head stops inside the tokenizer: the picker takes its
            // pairs for the header, and reads nothing more.
            head.complete = false;
            Some(head)
        };
        let listed = vec![
            (quant("Q4_K_M", 1, 4 << 30), typed(15)),
            (quant("Q8_0", 1, 7 << 30), typed(7)),
            (quant("BF16", 1, 8 << 30), typed(32)),
            (quant("F32", 1, 64 << 30), typed(0)),
        ];
        assert_eq!(
            listed_pick(&listed, false, 0, Some(32 << 30)).await,
            Some(1)
        );
        assert_eq!(listed_pick(&listed, false, 0, None).await, Some(0));

        // As real repositories ship them: the quantized heads stop before
        // their type. The listing's heads cannot tell, and mark nothing; the
        // download's own pick, reading each header further, names the same
        // Q8_0 the typed heads name.
        let untyped = || {
            let mut head = shaped_head(true);
            head.metadata.file_type = None;
            head.complete = false;
            Some(head)
        };
        let shipped = vec![
            (quant("Q4_K_M", 1, 4 << 30), untyped()),
            (quant("Q8_0", 1, 7 << 30), untyped()),
            (quant("BF16", 1, 8 << 30), untyped()),
        ];
        assert_eq!(listed_pick(&shipped, false, 0, Some(32 << 30)).await, None);
        let typed_past_the_head =
            |path: &str, file_type: u8| (path.to_owned(), vec![file_type; 70 << 10]);
        let hub = FakeHub {
            weights: vec![
                hub_file("m-Q4_K_M.gguf", 4 << 30, "a"),
                hub_file("m-Q8_0.gguf", 7 << 30, "b"),
                hub_file("m-BF16.gguf", 8 << 30, "c"),
            ],
            heads: vec![
                typed_past_the_head("m-Q4_K_M.gguf", 15),
                typed_past_the_head("m-Q8_0.gguf", 7),
                typed_past_the_head("m-BF16.gguf", 32),
            ],
            ..FakeHub::default()
        };
        let read_further = gglib_download::download_pick(
            Arc::new(hub),
            Arc::new(TypedPastTheHead),
            "o/r",
            Some(32 << 30),
        )
        .await
        .unwrap();
        assert_eq!(read_further.as_deref(), Some("Q8_0"));

        let mut helper = quant("MTP", 1, 1 << 30);
        helper.role = Some(GgufFileRole::Sidecar);
        assert_eq!(
            listed_pick(&[(helper, None)], false, 0, Some(32 << 30)).await,
            None
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
        // With no header read at all, the same: an image model needs none.
        let headless = quantizations_response(
            "o/r",
            vec![(quant("Q8_0", 1, 1 << 30), None)],
            true,
            None,
            Some(16 << 30),
        )
        .quantizations
        .remove(0)
        .fit;
        assert_eq!(headless, Some(VariantFit::Fits { ctx: None }));
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
        use crate::test_support::{MockDownloadManager, MockHfClient};

        let ops = DownloadOps::new(DownloadDeps {
            downloads: Arc::new(MockDownloadManager::new()),
            hf: Arc::new(MockHfClient),
            gguf_parser: Arc::new(gglib_core::ports::huggingface::fake_hub::RoleParser),
            models_directory: None,
            core: None,
        });

        let listed = ops.get_model_quantizations("o/r").await.unwrap();

        assert_eq!(
            listed.quantizations[0].projector,
            Some(HfProjector {
                file_path: "mmproj-F16.gguf".to_owned(),
                size_bytes: 7
            })
        );
        // The fake hub serves no model's head, so the listing cannot tell
        // what the download would take, and marks nothing rather than guess;
        // the download's own pick, asked apart, names its one model.
        assert!(listed.quantizations.iter().all(|q| !q.picked));
        let pick = ops.get_model_pick("o/r").await.unwrap();
        assert_eq!(pick.label.as_deref(), Some("Q4_K_M"));
    }
}
