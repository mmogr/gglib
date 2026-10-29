//! A model's shape, read off what there is to read, gives gglib-core's
//! verdict, and says when a missing key may only have been cut off.

use std::collections::HashMap;

use gglib_core::domain::GgufMetadata;

use super::*;
use crate::test_gguf::CHAT_MODEL_KV;

const GIB: u64 = 1 << 30;

/// The metadata a head holds of a llama-shaped model: `trained_ctx` when
/// given, and the KV keys when `with_kv`.
fn head(trained_ctx: Option<u64>, with_kv: bool, complete: bool) -> HeadMetadata {
    let mut keys = HashMap::new();
    if with_kv {
        for (key, value) in [
            ("llama.block_count", "32"),
            ("llama.attention.head_count", "24"),
            ("llama.attention.head_count_kv", "8"),
            ("llama.attention.key_length", "128"),
            ("llama.attention.value_length", "128"),
        ] {
            keys.insert(key.to_owned(), value.to_owned());
        }
    }
    HeadMetadata {
        metadata: GgufMetadata {
            architecture: Some("llama".to_owned()),
            context_length: trained_ctx,
            metadata: keys,
            ..GgufMetadata::default()
        },
        reached_tokenizer: !complete,
        complete,
        bytes_used: 1024,
    }
}

#[test]
fn a_head_with_every_fit_key_gets_gglib_cores_verdict_and_its_reasons() {
    let shape = ModelShape::of_head(&head(Some(40_960), true, false));

    for (weights, budget) in [(GIB, 4 * GIB), (3 * GIB, 4 * GIB), (8 * GIB, 4 * GIB)] {
        let (verdict, why) = shape.judge(Some(weights), Some(budget));

        let (core, inputs) = fit_verdict(
            Some(40_960),
            Some(weights),
            Some(CHAT_MODEL_KV),
            Some(budget),
        );
        assert_eq!(verdict, carried(core, false));
        assert_eq!(why, FitWhy::from(inputs));
        assert_eq!(why.trained_ctx, Some(40_960));
        assert_eq!(why.weights_bytes, Some(weights));
        assert!(why.kv_bytes_per_token.is_some_and(|bytes| bytes > 0));
    }
}

#[test]
fn verdicts_run_from_fits_to_too_big_as_the_weights_grow() {
    let shape = ModelShape::of_head(&head(Some(40_960), true, false));

    let (small, _) = shape.judge(Some(GIB), Some(4 * GIB));
    let (huge, _) = shape.judge(Some(8 * GIB), Some(4 * GIB));

    assert!(
        matches!(small, FitVerdict::Fits { ctx } if ctx > 4096),
        "{small:?}"
    );
    assert!(
        matches!(huge, FitVerdict::TooBig { need_bytes, have_bytes } if need_bytes > have_bytes),
        "{huge:?}"
    );
}

/// The head ended before the header did, and the trained context is not in
/// it: the file may still hold it.
#[test]
fn a_key_the_head_did_not_reach_is_cut_off() {
    let shape = ModelShape::of_head(&head(None, true, false));

    let (verdict, why) = shape.judge(Some(GIB), Some(4 * GIB));

    assert_eq!(
        verdict,
        FitVerdict::Unknown {
            missing: MissingInput::TrainedContext,
            cut_off: true
        }
    );
    assert_eq!(why.trained_ctx, None);
}

/// The head held the whole header, so a key it lacks the file lacks.
#[test]
fn a_key_a_whole_header_lacks_is_missing_from_the_file() {
    let shape = ModelShape::of_head(&head(Some(40_960), false, true));

    let (verdict, _) = shape.judge(Some(GIB), Some(4 * GIB));

    assert_eq!(
        verdict,
        FitVerdict::Unknown {
            missing: MissingInput::KvShape,
            cut_off: false
        }
    );
}

/// Neither the weights nor the budget come from the header, so their
/// absence is never a cut.
#[test]
fn a_missing_budget_is_never_cut_off() {
    let shape = ModelShape::of_head(&head(Some(40_960), true, false));

    let (verdict, _) = shape.judge(Some(GIB), None);

    assert_eq!(
        verdict,
        FitVerdict::Unknown {
            missing: MissingInput::Budget,
            cut_off: false
        }
    );
}

/// gglib serves no context below 4096 tokens, so a model trained for fewer
/// is too big with memory to spare; the trained context is the reason.
#[test]
fn a_model_trained_for_fewer_than_the_smallest_context_is_too_big_with_room_to_spare() {
    let shape = ModelShape::of_head(&head(Some(2048), true, true));

    let (verdict, why) = shape.judge(Some(GIB / 4), Some(8 * GIB));

    assert!(
        matches!(verdict, FitVerdict::TooBig { need_bytes, have_bytes } if need_bytes <= have_bytes),
        "{verdict:?}"
    );
    assert_eq!(why.trained_ctx, Some(2048));
}

#[test]
fn a_shortlist_row_is_judged_on_the_shape_it_states() {
    let row = gglib_core::domain::recommendation::PHONE_SHORTLIST[0];

    let (verdict, why) =
        ModelShape::of_candidate(&row).judge(Some(row.weights_bytes), Some(8 * GIB));

    let (core, _) = fit_verdict(
        Some(row.trained_ctx),
        Some(row.weights_bytes),
        Some(row.kv_elems_per_token),
        Some(8 * GIB),
    );
    assert_eq!(verdict, carried(core, false));
    assert_eq!(why.trained_ctx, Some(row.trained_ctx));
}
