//! Tests for [`super::pick_variant`] and [`super::MemoryFit`].

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use super::{
    Contender, Fit, FitJudge, HeadFacts, MemoryFit, NoWeights, Pick, PickReason, VariantReader,
    pick_variant,
};
use crate::cache_config::{DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V};
use crate::domain::FileType;
use crate::domain::gguf::{GgufMetadata, HeadMetadata};
use crate::domain::kv_estimate::estimate_kv_shape;
use crate::domain::phone_fit::{FitVerdict, fit_verdict};
use crate::domain::recommendation::{budget_covering, usable_share};
use crate::domain::variant_fit::VariantFit;
use crate::download::GgufFileRole;

const Q4_K_M: FileType = FileType(15);
const Q6_K: FileType = FileType(18);
const Q8_0: FileType = FileType(7);
const F16: FileType = FileType(1);
const BF16: FileType = FileType(32);

/// A variant's files as the fake hub holds them.
#[derive(Clone)]
struct Files {
    head: Option<HeadFacts>,
    header: Option<GgufMetadata>,
}

/// A reader over `files`, by index, that writes down each read.
struct Reader {
    files: Vec<Files>,
    reads: Mutex<Vec<String>>,
}

impl Reader {
    fn new(files: Vec<Files>) -> Self {
        Self {
            files,
            reads: Mutex::new(Vec::new()),
        }
    }

    fn reads(&self) -> Vec<String> {
        self.reads.lock().unwrap().clone()
    }
}

#[async_trait]
impl VariantReader for Reader {
    async fn head(&self, index: usize) -> Option<HeadFacts> {
        self.reads.lock().unwrap().push(format!("head {index}"));
        self.files[index].head.clone()
    }

    async fn header(&self, index: usize) -> Option<GgufMetadata> {
        self.reads.lock().unwrap().push(format!("header {index}"));
        self.files[index].header.clone()
    }
}

/// Fits whatever is at most `limit` bytes, when the metadata states a
/// trained context: a head without the fit's keys cannot tell.
struct AtMost(u64);

impl FitJudge for AtMost {
    fn fit(&self, metadata: &GgufMetadata, _image: bool, weights_bytes: u64) -> Fit {
        if metadata.context_length.is_none() {
            Fit::CannotTell
        } else if weights_bytes <= self.0 {
            Fit::Fits
        } else {
            Fit::DoesNotFit
        }
    }

    fn weights_fit(&self, weights_bytes: u64) -> bool {
        weights_bytes <= self.0
    }
}

fn metadata(file_type: Option<FileType>, role: GgufFileRole) -> GgufMetadata {
    GgufMetadata {
        context_length: Some(40_960),
        file_type,
        role,
        ..GgufMetadata::default()
    }
}

/// A variant whose head ends before `general.file_type` and settles no
/// role, and whose whole header states `file_type`.
fn typed_late(file_type: FileType) -> Files {
    let mut cut = metadata(None, GgufFileRole::Weights);
    cut.role = GgufFileRole::Weights;
    Files {
        head: Some(HeadFacts {
            head: HeadMetadata {
                metadata: cut,
                complete: false,
                role: None,
                ..HeadMetadata::default()
            },
            image: false,
        }),
        header: Some(metadata(Some(file_type), GgufFileRole::Weights)),
    }
}

/// A variant whose head holds its whole header.
fn whole(file_type: Option<FileType>, role: GgufFileRole) -> Files {
    let metadata = metadata(file_type, role);
    Files {
        head: Some(HeadFacts {
            head: HeadMetadata {
                metadata: metadata.clone(),
                complete: true,
                role: Some(role),
                ..HeadMetadata::default()
            },
            image: false,
        }),
        header: Some(metadata),
    }
}

fn open(size: u64) -> Contender {
    Contender { size, role: None }
}

async fn pick(contenders: &[Contender], reader: &Reader, limit: u64) -> Result<Pick, NoWeights> {
    pick_variant(contenders, reader, Some(&AtMost(limit))).await
}

#[tokio::test]
async fn the_largest_quantized_variant_that_fits_is_taken_reading_nothing_past_it() {
    // Listed smallest first, as a Hub listing often is.
    let contenders = [open(4), open(6), open(8), open(16)];
    let reader = Reader::new(vec![
        typed_late(Q4_K_M),
        typed_late(Q6_K),
        typed_late(Q8_0),
        typed_late(F16),
    ]);

    let picked = pick(&contenders, &reader, 10).await.unwrap();

    assert_eq!(
        picked,
        Pick {
            index: 2,
            reason: PickReason::Quantized(Q8_0)
        }
    );
    // F16 does not fit, so its type is never read; nothing smaller is read.
    assert_eq!(reader.reads(), ["head 3", "head 2", "header 2"]);
}

#[tokio::test]
async fn an_unquantized_variant_that_fits_gives_way_to_a_smaller_quantized_one() {
    let contenders = [open(16), open(8), open(32)];
    let reader = Reader::new(vec![typed_late(BF16), typed_late(Q8_0), typed_late(F16)]);

    let picked = pick(&contenders, &reader, 20).await.unwrap();

    assert_eq!(
        picked,
        Pick {
            index: 1,
            reason: PickReason::Quantized(Q8_0)
        }
    );
}

#[tokio::test]
async fn when_only_unquantized_variants_fit_the_largest_of_them_is_taken() {
    let contenders = [open(16), open(8), open(32)];
    let reader = Reader::new(vec![typed_late(BF16), typed_late(F16), typed_late(F16)]);

    let picked = pick(&contenders, &reader, 20).await.unwrap();

    assert_eq!(
        picked,
        Pick {
            index: 0,
            reason: PickReason::LargestFitting(Some(BF16))
        }
    );
}

#[tokio::test]
async fn a_variant_whose_header_states_no_type_is_not_counted_quantized() {
    let contenders = [open(16), open(8)];
    let reader = Reader::new(vec![
        whole(None, GgufFileRole::Weights),
        whole(None, GgufFileRole::Weights),
    ]);

    let picked = pick(&contenders, &reader, 20).await.unwrap();

    assert_eq!(
        picked,
        Pick {
            index: 0,
            reason: PickReason::LargestFitting(None)
        }
    );
}

#[tokio::test]
async fn when_none_fits_the_smallest_variant_is_taken() {
    let contenders = [open(16), open(4), open(8)];
    let reader = Reader::new(vec![typed_late(F16), typed_late(Q4_K_M), typed_late(Q8_0)]);

    let picked = pick(&contenders, &reader, 2).await.unwrap();

    assert_eq!(
        picked,
        Pick {
            index: 1,
            reason: PickReason::NoneFits
        }
    );
}

#[tokio::test]
async fn without_a_budget_the_smallest_variant_is_taken_and_no_fit_is_judged() {
    let contenders = [
        Contender {
            size: 16,
            role: Some(GgufFileRole::Weights),
        },
        Contender {
            size: 4,
            role: Some(GgufFileRole::Weights),
        },
    ];
    let reader = Reader::new(vec![typed_late(F16), typed_late(Q4_K_M)]);

    let picked = pick_variant(&contenders, &reader, None).await.unwrap();

    assert_eq!(
        picked,
        Pick {
            index: 1,
            reason: PickReason::NoBudget
        }
    );
    // The listing settled it as weights: nothing to read.
    assert!(reader.reads().is_empty());
}

#[tokio::test]
async fn a_variant_the_listing_or_its_head_shows_a_sidecar_is_never_taken() {
    // The largest is a projector by its listing, the next a draft by its
    // head; both would fit.
    let contenders = [
        Contender {
            size: 9,
            role: Some(GgufFileRole::Projector),
        },
        open(8),
        open(4),
    ];
    let reader = Reader::new(vec![
        whole(Some(F16), GgufFileRole::Projector),
        whole(Some(Q8_0), GgufFileRole::Sidecar),
        whole(Some(Q4_K_M), GgufFileRole::Weights),
    ]);

    let picked = pick(&contenders, &reader, 10).await.unwrap();

    assert_eq!(
        picked,
        Pick {
            index: 2,
            reason: PickReason::Quantized(Q4_K_M)
        }
    );
    assert!(!reader.reads().contains(&"head 0".to_owned()));
}

#[tokio::test]
async fn a_variant_whose_head_left_its_role_open_is_confirmed_from_its_whole_header() {
    // The head is cut short of the MTP keys; the whole header shows a draft.
    let mut draft = typed_late(Q8_0);
    draft.header = Some(metadata(Some(Q8_0), GgufFileRole::Sidecar));
    let contenders = [open(8), open(4)];
    let reader = Reader::new(vec![draft, typed_late(Q4_K_M)]);

    let picked = pick(&contenders, &reader, 10).await.unwrap();

    assert_eq!(
        picked,
        Pick {
            index: 1,
            reason: PickReason::Quantized(Q4_K_M)
        }
    );
}

#[tokio::test]
async fn the_smallest_variant_is_never_a_sidecar_its_header_shows() {
    let mut draft = typed_late(Q4_K_M);
    draft.header = Some(metadata(Some(Q4_K_M), GgufFileRole::Sidecar));
    let contenders = [open(2), open(8)];
    let reader = Reader::new(vec![draft, typed_late(Q8_0)]);

    let picked = pick(&contenders, &reader, 1).await.unwrap();

    assert_eq!(
        picked,
        Pick {
            index: 1,
            reason: PickReason::NoneFits
        }
    );
}

#[tokio::test]
async fn a_head_that_ends_before_the_fit_keys_is_judged_from_the_whole_header() {
    let mut late = typed_late(Q8_0);
    if let Some(head) = &mut late.head {
        head.head.metadata.context_length = None;
    }
    let contenders = [open(8)];
    let reader = Reader::new(vec![late]);

    let picked = pick(&contenders, &reader, 10).await.unwrap();

    assert_eq!(
        picked,
        Pick {
            index: 0,
            reason: PickReason::Quantized(Q8_0)
        }
    );
    // The header read for the fit is the one the type comes from.
    assert_eq!(reader.reads(), ["head 0", "header 0"]);
}

/// A variant whose fit its head cannot tell, and whose weights do not fit
/// alone, does not fit with any KV cache: its whole header is not read.
#[tokio::test]
async fn weights_too_big_alone_have_no_whole_header_read() {
    let cut = |file_type| {
        let mut late = typed_late(file_type);
        if let Some(head) = &mut late.head {
            head.head.metadata.context_length = None;
        }
        late
    };
    let contenders = [open(16), open(8)];
    let reader = Reader::new(vec![cut(Q8_0), cut(Q4_K_M)]);

    let picked = pick(&contenders, &reader, 10).await.unwrap();

    assert_eq!(
        picked,
        Pick {
            index: 1,
            reason: PickReason::Quantized(Q4_K_M)
        }
    );
    assert_eq!(reader.reads(), ["head 0", "head 1", "header 1"]);
}

/// A head that already shows a draft settles it: no whole header is read
/// for it, though its fit could not be told from the head.
#[tokio::test]
async fn a_draft_its_head_shows_has_no_whole_header_read() {
    let mut draft = typed_late(Q8_0);
    if let Some(head) = &mut draft.head {
        head.head.role = Some(GgufFileRole::Sidecar);
        head.head.metadata.context_length = None;
    }
    let contenders = [open(8), open(4)];
    let reader = Reader::new(vec![draft, typed_late(Q4_K_M)]);

    let picked = pick(&contenders, &reader, 10).await.unwrap();

    assert_eq!(
        picked,
        Pick {
            index: 1,
            reason: PickReason::Quantized(Q4_K_M)
        }
    );
    assert!(
        !reader.reads().contains(&"header 0".to_owned()),
        "{:?}",
        reader.reads()
    );
}

/// An architecture whose KV shape cannot be told even from its whole header
/// is judged by its weights alone, not passed over to the smallest.
#[tokio::test]
async fn a_shape_the_whole_header_cannot_tell_is_judged_by_the_weights() {
    let unknown = |file_type| {
        let mut files = typed_late(file_type);
        if let Some(head) = &mut files.head {
            head.head.metadata.context_length = None;
        }
        if let Some(header) = &mut files.header {
            header.context_length = None;
        }
        files
    };
    let contenders = [open(16), open(8), open(4)];
    let reader = Reader::new(vec![unknown(F16), unknown(Q8_0), unknown(Q4_K_M)]);

    let picked = pick(&contenders, &reader, 10).await.unwrap();

    assert_eq!(
        picked,
        Pick {
            index: 1,
            reason: PickReason::Quantized(Q8_0)
        }
    );
}

#[tokio::test]
async fn a_repository_of_drafts_and_projectors_holds_no_weights() {
    let contenders = [
        Contender {
            size: 8,
            role: Some(GgufFileRole::Sidecar),
        },
        Contender {
            size: 2,
            role: Some(GgufFileRole::Projector),
        },
    ];
    let reader = Reader::new(vec![
        whole(Some(Q8_0), GgufFileRole::Sidecar),
        whole(Some(F16), GgufFileRole::Projector),
    ]);

    assert_eq!(pick(&contenders, &reader, 10).await, Err(NoWeights));
    assert_eq!(pick_variant(&[], &reader, None).await, Err(NoWeights));
}

#[tokio::test]
async fn a_repository_whose_open_variants_all_turn_out_drafts_holds_no_weights() {
    let mut draft = typed_late(Q8_0);
    draft.header = Some(metadata(Some(Q8_0), GgufFileRole::Sidecar));
    let contenders = [open(8)];
    let reader = Reader::new(vec![draft]);

    assert_eq!(pick(&contenders, &reader, 10).await, Err(NoWeights));
    assert_eq!(pick(&contenders, &reader, 1).await, Err(NoWeights));
}

#[test]
fn the_message_for_no_weights_names_what_is_there() {
    assert_eq!(
        NoWeights.to_string(),
        "only draft or helper files: gglib can't run draft or helper files on their own"
    );
}

// --- MemoryFit ----------------------------------------------------------

const GIB: u64 = 1024 * 1024 * 1024;

/// `unsloth/Qwen3-4B-GGUF` `Q4_K_M`: its size on the Hub, and 36 layers x
/// 8 KV heads x 128 head dim, as its header states them.
const QWEN3_4B_Q4_K_M: u64 = 2_497_281_312;

fn qwen3_4b() -> GgufMetadata {
    let keys = [
        ("general.architecture", "qwen3"),
        ("qwen3.block_count", "36"),
        ("qwen3.attention.head_count", "32"),
        ("qwen3.attention.head_count_kv", "8"),
        ("qwen3.attention.key_length", "128"),
        ("qwen3.attention.value_length", "128"),
    ];
    GgufMetadata {
        architecture: Some("qwen3".to_owned()),
        context_length: Some(40_960),
        metadata: keys
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect::<HashMap<_, _>>(),
        ..GgufMetadata::default()
    }
}

#[test]
fn a_chat_model_fits_when_a_rung_above_the_lowest_does() {
    let model = qwen3_4b();
    let fit = MemoryFit {
        budget_bytes: 8 * GIB,
    }
    .fit(&model, false, QWEN3_4B_Q4_K_M);
    assert_eq!(fit, Fit::Fits);
}

#[test]
fn a_chat_model_that_fits_only_the_lowest_rung_does_not_count() {
    // Room for the weights and a little cache: the verdict is Tight.
    let model = qwen3_4b();
    let tight = budget_covering(QWEN3_4B_Q4_K_M + 450 * 1024 * 1024);
    let kv = estimate_kv_shape(&model.metadata, model.architecture.as_deref());
    let verdict = fit_verdict(model.context_length, Some(QWEN3_4B_Q4_K_M), kv, Some(tight)).0;
    assert!(matches!(verdict, FitVerdict::Tight { .. }), "{verdict:?}");
    let fit = MemoryFit {
        budget_bytes: tight,
    }
    .fit(&model, false, QWEN3_4B_Q4_K_M);
    assert_eq!(fit, Fit::DoesNotFit);
}

#[test]
fn a_chat_model_whose_shape_was_not_read_cannot_be_told() {
    let fit = MemoryFit {
        budget_bytes: 64 * GIB,
    };
    let mut model = qwen3_4b();
    model.context_length = None;
    assert_eq!(fit.fit(&model, false, QWEN3_4B_Q4_K_M), Fit::CannotTell);
    let mut model = qwen3_4b();
    model.metadata.clear();
    model.architecture = None;
    assert_eq!(fit.fit(&model, false, QWEN3_4B_Q4_K_M), Fit::CannotTell);
}

#[test]
fn a_chat_model_too_big_for_the_lowest_rung_does_not_fit() {
    let fit = MemoryFit {
        budget_bytes: 2 * GIB,
    };
    assert_eq!(
        fit.fit(&qwen3_4b(), false, QWEN3_4B_Q4_K_M),
        Fit::DoesNotFit
    );
}

/// A model trained for no more than the lowest rung fits once its weights
/// and the KV cache of its whole trained context do: Llama 2 at 4096,
/// `TinyLlama` at 2048, an embedding model at 512.
#[test]
fn a_model_trained_for_no_more_than_the_lowest_rung_fits_its_whole_window() {
    for trained in [512, 2048, 4096] {
        let mut model = qwen3_4b();
        model.context_length = Some(trained);
        let kv = estimate_kv_shape(&model.metadata, model.architecture.as_deref()).unwrap();
        let kv = kv.bytes(DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V);
        let window = QWEN3_4B_Q4_K_M + kv.at(trained);

        let at_edge = MemoryFit {
            budget_bytes: budget_covering(window),
        };
        let short = MemoryFit {
            budget_bytes: budget_covering(window) - 1,
        };

        assert_eq!(
            at_edge.fit(&model, false, QWEN3_4B_Q4_K_M),
            Fit::Fits,
            "{trained}"
        );
        assert_eq!(
            short.fit(&model, false, QWEN3_4B_Q4_K_M),
            Fit::DoesNotFit,
            "{trained}"
        );
        let roomy = MemoryFit {
            budget_bytes: 64 * GIB,
        };
        assert_eq!(
            roomy.fit(&model, false, QWEN3_4B_Q4_K_M),
            Fit::Fits,
            "{trained}"
        );
    }
}

/// A model trained for more than the lowest rung and fewer than the next is
/// fitted at the lowest, short of its window: it fits once its weights and
/// the KV cache of its whole trained context do, as one trained for fewer
/// does.
#[test]
fn a_model_trained_between_the_lowest_two_rungs_fits_its_whole_window() {
    for trained in [6144, 8191] {
        let mut model = qwen3_4b();
        model.context_length = Some(trained);
        let kv = estimate_kv_shape(&model.metadata, model.architecture.as_deref()).unwrap();
        let kv = kv.bytes(DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V);
        let window = QWEN3_4B_Q4_K_M + kv.at(trained);

        let at_edge = MemoryFit {
            budget_bytes: budget_covering(window),
        };
        let short = MemoryFit {
            budget_bytes: budget_covering(window) - 1,
        };

        assert_eq!(
            at_edge.fit(&model, false, QWEN3_4B_Q4_K_M),
            Fit::Fits,
            "{trained}"
        );
        assert_eq!(
            short.fit(&model, false, QWEN3_4B_Q4_K_M),
            Fit::DoesNotFit,
            "{trained}"
        );
    }
}

#[test]
fn the_weights_alone_fit_the_share_the_fit_spends() {
    let fit = MemoryFit {
        budget_bytes: 16 * GIB,
    };
    assert!(fit.weights_fit(usable_share(16 * GIB)));
    assert!(!fit.weights_fit(usable_share(16 * GIB) + 1));
}

#[test]
fn an_image_model_fits_by_its_weights_alone() {
    let budget = 16 * GIB;
    let fit = MemoryFit {
        budget_bytes: budget,
    };
    let none = GgufMetadata::default();
    assert_eq!(fit.fit(&none, true, usable_share(budget)), Fit::Fits);
    assert_eq!(
        fit.fit(&none, true, usable_share(budget) + 1),
        Fit::DoesNotFit
    );
}

/// The verdict beside the picker's answer, as a listing shows it: each arm
/// of [`MemoryFit::judge`] against the fit it rests on and the numbers a
/// `TooBig` carries, the least budget covering the load against the budget
/// there is.
#[test]
fn each_verdict_says_what_the_picker_found() {
    let model = qwen3_4b();
    let kv = estimate_kv_shape(&model.metadata, model.architecture.as_deref());
    let kv_bytes = kv
        .unwrap()
        .bytes(DEFAULT_CACHE_TYPE_K, DEFAULT_CACHE_TYPE_V);
    let per_token = kv_bytes.per_token;

    // Roomy: the ladder's own verdict, with the context it reached.
    let roomy = MemoryFit {
        budget_bytes: 64 * GIB,
    };
    let (ladder, _) = fit_verdict(
        model.context_length,
        Some(QWEN3_4B_Q4_K_M),
        kv,
        Some(64 * GIB),
    );
    assert!(matches!(ladder, FitVerdict::Fits { .. }), "{ladder:?}");
    assert_eq!(
        roomy.judge(&model, false, QWEN3_4B_Q4_K_M),
        (Fit::Fits, VariantFit::from(ladder))
    );

    // A whole window that fits: the model is served at all it was trained
    // for, whether the ladder's lowest rung was tight (4096) or too big.
    for trained in [2048, 4096, 6144] {
        let mut model = qwen3_4b();
        model.context_length = Some(trained);
        let window = QWEN3_4B_Q4_K_M + per_token * trained;
        let at_edge = MemoryFit {
            budget_bytes: budget_covering(window),
        };
        assert_eq!(
            at_edge.judge(&model, false, QWEN3_4B_Q4_K_M),
            (Fit::Fits, VariantFit::Fits { ctx: Some(trained) }),
            "{trained}"
        );
        let short = MemoryFit {
            budget_bytes: budget_covering(window) - 1,
        };
        assert_eq!(
            short.judge(&model, false, QWEN3_4B_Q4_K_M),
            (
                Fit::DoesNotFit,
                VariantFit::TooBig {
                    need_bytes: budget_covering(window),
                    have_bytes: budget_covering(window) - 1,
                }
            ),
            "{trained}"
        );
    }

    // Too big for the lowest rung of a long window: the ladder's numbers.
    let cramped = MemoryFit {
        budget_bytes: 2 * GIB,
    };
    let (fit, verdict) = cramped.judge(&model, false, QWEN3_4B_Q4_K_M);
    assert_eq!(fit, Fit::DoesNotFit);
    assert!(
        matches!(verdict, VariantFit::TooBig { have_bytes, need_bytes } if have_bytes == 2 * GIB && need_bytes > 2 * GIB),
        "{verdict:?}"
    );

    // An image model: its files alone, with no context to serve at.
    let image = MemoryFit {
        budget_bytes: 16 * GIB,
    };
    let none = GgufMetadata::default();
    assert_eq!(
        image.judge(&none, true, usable_share(16 * GIB)),
        (Fit::Fits, VariantFit::Fits { ctx: None })
    );
    assert_eq!(
        image.judge(&none, true, usable_share(16 * GIB) + 1),
        (
            Fit::DoesNotFit,
            VariantFit::TooBig {
                need_bytes: budget_covering(usable_share(16 * GIB) + 1),
                have_bytes: 16 * GIB,
            }
        )
    );
}
