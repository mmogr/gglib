//! Tests for [`super`]: the shape a header gives, layer by layer where it
//! says which layers slide, and its bytes at a context.

use std::collections::HashMap;

use super::*;

const MIB: u64 = 1024 * 1024;

fn md(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|&(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

/// A shape with no window, `k` and `v` elements a token.
const fn uniform(k: u64, v: u64) -> KvShape {
    KvShape::uniform(KvElemsPerToken { k, v })
}

/// Qwen3-shaped metadata with grouped-query attention and explicit head dims.
fn qwen_metadata() -> HashMap<String, String> {
    md(&[
        ("general.architecture", "qwen3"),
        ("qwen3.block_count", "64"),
        ("qwen3.attention.head_count", "40"),
        ("qwen3.attention.head_count_kv", "8"),
        ("qwen3.embedding_length", "5120"),
        ("qwen3.attention.key_length", "128"),
        ("qwen3.attention.value_length", "128"),
    ])
}

/// Qwen3 fixture: 64 layers × 8 kv heads × 128 head dim = 65536 elems/side.
const QWEN_ELEMS: u64 = 64 * 8 * 128;

/// Gemma-4-31B's header as llama-server printed it loading
/// `Gemma-4-31B-Humanlike-Chat-Q8_0.gguf`: 60 layers, every sixth full
/// attention with 4 KV heads of 512, the others sliding over 1024 tokens
/// with 16 KV heads of 256.
fn gemma4_metadata() -> HashMap<String, String> {
    let every_sixth = |full: &str, sliding: &str| {
        let layers: Vec<&str> = (0..60)
            .map(|i| if i % 6 == 5 { full } else { sliding })
            .collect();
        format!("[{}]", layers.join(", "))
    };
    let mut md = md(&[
        ("general.architecture", "gemma4"),
        ("gemma4.block_count", "60"),
        ("gemma4.context_length", "262144"),
        ("gemma4.embedding_length", "5376"),
        ("gemma4.attention.head_count", "32"),
        ("gemma4.attention.key_length", "512"),
        ("gemma4.attention.value_length", "512"),
        ("gemma4.attention.sliding_window", "1024"),
        ("gemma4.attention.key_length_swa", "256"),
        ("gemma4.attention.value_length_swa", "256"),
    ]);
    md.insert(
        "gemma4.attention.head_count_kv".to_owned(),
        every_sixth("4", "16"),
    );
    md.insert(
        "gemma4.attention.sliding_window_pattern".to_owned(),
        every_sixth("false", "true"),
    );
    md
}

/// The same header as a row stored before lists were kept whole.
fn gemma4_stored_lossily() -> HashMap<String, String> {
    let mut md = gemma4_metadata();
    for key in [
        "gemma4.attention.head_count_kv",
        "gemma4.attention.sliding_window_pattern",
    ] {
        md.insert(key.to_owned(), "[Array with 60 elements]".to_owned());
    }
    md
}

const fn q8(shape: KvShape) -> KvBytes {
    shape.bytes(KvCacheType::Q8_0, KvCacheType::Q8_0)
}

#[test]
fn computes_from_explicit_head_dims() {
    let got = estimate_kv_shape(&qwen_metadata(), Some("qwen3"));
    assert_eq!(got, Some(uniform(QWEN_ELEMS, QWEN_ELEMS)));
}

#[test]
fn architecture_falls_back_to_general_architecture_key() {
    let got = estimate_kv_shape(&qwen_metadata(), None);
    assert_eq!(got, Some(uniform(QWEN_ELEMS, QWEN_ELEMS)));
}

#[test]
fn architecture_lookup_is_case_insensitive() {
    let got = estimate_kv_shape(&qwen_metadata(), Some("QWEN3"));
    assert_eq!(got, Some(uniform(QWEN_ELEMS, QWEN_ELEMS)));
}

#[test]
fn derives_head_dim_from_embedding_length_when_absent() {
    let mut md = qwen_metadata();
    md.remove("qwen3.attention.key_length");
    md.remove("qwen3.attention.value_length");
    // head_dim = 5120 / 40 = 128, so the result matches the explicit case.
    let got = estimate_kv_shape(&md, Some("qwen3"));
    assert_eq!(got, Some(uniform(QWEN_ELEMS, QWEN_ELEMS)));
}

/// Without GQA metadata the full head count is the KV head count — a much
/// larger cache, which the estimate must reflect.
#[test]
fn falls_back_to_head_count_without_gqa() {
    let mut md = qwen_metadata();
    md.remove("qwen3.attention.head_count_kv");
    let expected = 64 * 40 * 128;
    assert_eq!(
        estimate_kv_shape(&md, Some("qwen3")),
        Some(uniform(expected, expected))
    );
}

#[test]
fn none_when_block_count_missing() {
    let mut md = qwen_metadata();
    md.remove("qwen3.block_count");
    assert_eq!(estimate_kv_shape(&md, Some("qwen3")), None);
}

#[test]
fn none_when_head_counts_missing() {
    let mut md = qwen_metadata();
    md.remove("qwen3.attention.head_count");
    md.remove("qwen3.attention.head_count_kv");
    assert_eq!(estimate_kv_shape(&md, Some("qwen3")), None);
}

/// Head dims are neither explicit nor derivable without `embedding_length`.
#[test]
fn none_when_head_dim_underivable() {
    let mut md = qwen_metadata();
    md.remove("qwen3.attention.key_length");
    md.remove("qwen3.attention.value_length");
    md.remove("qwen3.embedding_length");
    assert_eq!(estimate_kv_shape(&md, Some("qwen3")), None);
}

#[test]
fn none_on_non_numeric_values() {
    let mut md = qwen_metadata();
    md.insert("qwen3.block_count".to_string(), "sixty-four".to_string());
    assert_eq!(estimate_kv_shape(&md, Some("qwen3")), None);
}

#[test]
fn none_when_metadata_empty() {
    assert_eq!(estimate_kv_shape(&HashMap::new(), Some("llama")), None);
    assert_eq!(estimate_kv_shape(&HashMap::new(), None), None);
}

/// A zero layer/head count would produce a nonsense zero-elem estimate,
/// which downstream would read as "KV is free" — reject it instead.
#[test]
fn none_on_degenerate_zero_counts() {
    let mut md = qwen_metadata();
    md.insert("qwen3.block_count".to_string(), "0".to_string());
    assert_eq!(estimate_kv_shape(&md, Some("qwen3")), None);

    let mut md = qwen_metadata();
    md.insert("qwen3.attention.head_count_kv".to_string(), "0".to_string());
    assert_eq!(estimate_kv_shape(&md, Some("qwen3")), None);
}

#[test]
fn unprefixed_keys_are_accepted_as_a_fallback() {
    let md = md(&[
        ("block_count", "32"),
        ("attention.head_count", "32"),
        ("attention.head_count_kv", "8"),
        ("embedding_length", "4096"),
    ]);
    // head_dim = 4096 / 32 = 128
    let expected = 32 * 8 * 128;
    assert_eq!(
        estimate_kv_shape(&md, Some("llama")),
        Some(uniform(expected, expected))
    );
}

// ── Layer by layer ───────────────────────────────────────────────────

/// Gemma-4-31B's 10 full layers cost 4 × 512 a token each side, and its 50
/// sliding layers 16 × 256 a cell over a window of 1024.
#[test]
fn a_sliding_pattern_counts_each_layer_by_its_own_heads() {
    let shape = estimate_kv_shape(&gemma4_metadata(), None).unwrap();
    assert_eq!(
        shape,
        KvShape {
            per_token: KvElemsPerToken {
                k: 10 * 4 * 512,
                v: 10 * 4 * 512,
            },
            window: Some(SlidingWindow {
                tokens: 1024,
                per_cell: KvElemsPerToken {
                    k: 50 * 16 * 256,
                    v: 50 * 16 * 256,
                },
            }),
        }
    );
}

/// The engine's reading of the same file at `q8_0` and `-c 16384`: a 680 MiB
/// cache over 16384 cells for the 10 full layers and a 637.5 MiB one over
/// 1536 cells for the 50 sliding layers. `llama-fit-params` gave 1340 MiB
/// for the context at 16384 and 11540 MiB at 262144; these are 1.7% and
/// 0.2% under that, its context column holding a little more than the
/// caches.
#[test]
fn gemma4_at_q8_0_is_the_cache_the_engine_makes() {
    let kv = q8(estimate_kv_shape(&gemma4_metadata(), None).unwrap());
    assert_eq!(kv.per_token, 43_520);
    assert_eq!(kv.at(16_384) - kv.per_token * 16_384, 1275 * MIB / 2);
    assert_eq!(kv.at(16_384), 2635 * MIB / 2);
    assert_eq!(kv.at(262_144), 23_035 * MIB / 2);
}

/// On a 96 GiB budget, with its `Q8_0` weights (30.38 GiB) and projector
/// rounded up to 32 GiB, Gemma-4-31B fits the top rung. Read from a header
/// stored before lists were kept whole, it fits 16384, as it did.
#[test]
fn gemma4_fits_the_top_rung_on_96_gib() {
    let fit = |md: &HashMap<String, String>| {
        crate::domain::fit_context(
            Some(262_144),
            Some(32 * 1024 * MIB),
            estimate_kv_shape(md, None),
            KvCacheType::Q8_0,
            KvCacheType::Q8_0,
            Some(96 * 1024 * MIB),
        )
    };
    assert_eq!(fit(&gemma4_metadata()), Some(131_072));
    assert_eq!(fit(&gemma4_stored_lossily()), Some(16_384));
}

/// The context a fit picks for Gemma-4-31B holds its cache, the window's
/// fixed block with it, inside the usable share of every budget.
#[test]
fn a_fitted_gemma4_context_fits_with_its_window_block() {
    let shape = estimate_kv_shape(&gemma4_metadata(), None);
    let kv = q8(shape.unwrap());
    let weights = 32 * 1024 * MIB;
    for budget in (36 * 1024 * MIB..=128 * 1024 * MIB).step_by(128 * 1024 * 1024) {
        let q8_0 = KvCacheType::Q8_0;
        let fitted = crate::domain::fit_context(
            Some(262_144),
            Some(weights),
            shape,
            q8_0,
            q8_0,
            Some(budget),
        );
        let Some(fitted) = fitted else { continue };
        let usable = crate::domain::recommendation::usable_share(budget);
        assert!(weights + kv.at(fitted) <= usable, "{fitted} at {budget}");
    }
}

/// A row stored before lists were kept whole holds the list's summary. It
/// is counted as it always was: every layer at `head_count`, 2,088,960
/// bytes a token at `q8_0`.
#[test]
fn a_list_stored_as_its_summary_keeps_the_old_count() {
    let kv = q8(estimate_kv_shape(&gemma4_stored_lossily(), None).unwrap());
    assert_eq!(kv, KvBytes::uniform(2_088_960));
}

/// A list for some other number of layers, or a window of 0, is read as no
/// pattern at all: every layer per token, at the most KV heads any has.
#[test]
fn a_pattern_that_does_not_fit_the_layers_counts_every_layer() {
    let every_layer = Some(uniform(60 * 16 * 512, 60 * 16 * 512));
    let mut short = gemma4_metadata();
    short.insert(
        "gemma4.attention.sliding_window_pattern".to_owned(),
        "[true, false]".to_owned(),
    );
    assert_eq!(estimate_kv_shape(&short, None), every_layer);
    let mut no_window = gemma4_metadata();
    no_window.insert("gemma4.attention.sliding_window".to_owned(), "0".to_owned());
    assert_eq!(estimate_kv_shape(&no_window, None), every_layer);
}

/// A period makes one layer in `n` full, rounded up, every layer at the
/// most KV heads; 0 makes every layer slide.
#[test]
fn a_period_makes_one_layer_in_n_full() {
    let mut md = qwen_metadata();
    md.insert(
        "qwen3.attention.sliding_window".to_owned(),
        "4096".to_owned(),
    );
    md.insert(
        "qwen3.attention.sliding_window_pattern".to_owned(),
        "6".to_owned(),
    );
    let shape = estimate_kv_shape(&md, None).unwrap();
    // ceil(64 / 6) = 11 full layers, 53 sliding.
    assert_eq!(shape.per_token.k, 11 * 8 * 128);
    assert_eq!(shape.window.unwrap().per_cell.k, 53 * 8 * 128);

    md.insert(
        "qwen3.attention.sliding_window_pattern".to_owned(),
        "0".to_owned(),
    );
    let all = estimate_kv_shape(&md, None).unwrap();
    assert_eq!(all.per_token.k, 0);
    assert_eq!(all.window.unwrap().per_cell.k, QWEN_ELEMS);
}

/// A header's layer count sizes no allocation: a scalar is never spread
/// into a list, so an absurd count is arithmetic, and returns at once.
#[test]
fn an_absurd_layer_count_allocates_nothing() {
    let mut md = qwen_metadata();
    md.insert("qwen3.block_count".to_owned(), u64::MAX.to_string());
    assert!(estimate_kv_shape(&md, None).is_some());
    md.insert(
        "qwen3.attention.sliding_window".to_owned(),
        "1024".to_owned(),
    );
    md.insert(
        "qwen3.attention.sliding_window_pattern".to_owned(),
        "6".to_owned(),
    );
    assert!(estimate_kv_shape(&md, None).unwrap().window.is_some());
}

/// Qwen3.8-27B (`qwen35`) has no sliding window: 17 cached layers of 4 KV
/// heads of 256, 36,992 bytes a token at `q8_0` and 4624 MiB at 131072,
/// as before. The engine holds 4352 MiB for its 16 layers and 512 MiB for
/// the MTP draft's context, which the 17th layer stands in for.
#[test]
fn qwen3_8_27b_is_counted_as_before() {
    let md = md(&[
        ("general.architecture", "qwen35"),
        ("qwen35.block_count", "65"),
        ("qwen35.embedding_length", "5120"),
        ("qwen35.attention.head_count", "24"),
        ("qwen35.attention.head_count_kv", "4"),
        ("qwen35.attention.key_length", "256"),
        ("qwen35.attention.value_length", "256"),
        ("qwen35.full_attention_interval", "4"),
    ]);
    let kv = q8(estimate_kv_shape(&md, None).unwrap());
    assert_eq!(kv, KvBytes::uniform(36_992));
    assert_eq!(kv.at(131_072), 4624 * MIB);
}

/// The in-process engine keeps every sliding layer at the full context, so
/// there each costs per token what it costs per cell.
#[test]
fn a_full_size_window_costs_per_token() {
    let shape = estimate_kv_shape(&gemma4_metadata(), None).unwrap();
    let expected = 10 * 4 * 512 + 50 * 16 * 256;
    assert_eq!(shape.full_size_window(), uniform(expected, expected));
}

// ── Bytes ────────────────────────────────────────────────────────────

#[test]
fn bytes_at_f16_are_two_bytes_an_element() {
    let shape = uniform(QWEN_ELEMS, QWEN_ELEMS);
    let got = shape.bytes(KvCacheType::F16, KvCacheType::F16);
    assert_eq!(got, KvBytes::uniform((QWEN_ELEMS + QWEN_ELEMS) * 2));
}

#[test]
fn bytes_support_asymmetric_k_v_types() {
    // q8_0 K with f16 V must sum each side's own type independently.
    let shape = uniform(QWEN_ELEMS, QWEN_ELEMS);
    let mixed = shape.bytes(KvCacheType::Q8_0, KvCacheType::F16);
    let expected = KvCacheType::Q8_0.bytes_for_elems(QWEN_ELEMS)
        + KvCacheType::F16.bytes_for_elems(QWEN_ELEMS);
    assert_eq!(mixed.per_token, expected);
    assert!(q8(shape).per_token < mixed.per_token);
}

#[test]
fn bytes_at_a_context_saturate() {
    assert_eq!(KvBytes::uniform(1024).at(100), 102_400);
    assert_eq!(KvBytes::uniform(u64::MAX).at(u64::MAX), u64::MAX);
}

/// The window block is the window and a micro-batch, no more than the
/// context, padded to 256 cells.
#[test]
fn the_window_block_stops_growing_at_the_window() {
    let kv = KvBytes {
        per_token: 0,
        window: Some(SlidingWindow {
            tokens: 1024,
            per_cell: 1,
        }),
    };
    assert_eq!(kv.at(0), 0);
    assert_eq!(kv.at(1), 256);
    assert_eq!(kv.at(1000), 1024);
    assert_eq!(kv.at(1536), 1536);
    assert_eq!(kv.at(1_000_000), 1536);
}

/// The context a budget holds is the largest whose cache fits it.
#[test]
fn context_within_inverts_at() {
    let gemma = q8(estimate_kv_shape(&gemma4_metadata(), None).unwrap());
    let all_sliding = KvBytes {
        per_token: 0,
        ..gemma
    };
    for kv in [gemma, KvBytes::uniform(36_992), all_sliding] {
        for bytes in [0, 1, 600 * MIB, 640 * MIB, 54 * 1024 * MIB, u64::MAX] {
            let ctx = kv.context_within(bytes);
            assert!(kv.at(ctx) <= bytes, "{kv:?} at {ctx} is over {bytes}");
            assert!(
                ctx == u64::MAX || kv.at(ctx + 1) > bytes,
                "{kv:?} at {} still fits {bytes}",
                ctx + 1
            );
        }
    }
    assert_eq!(KvBytes::uniform(1000).context_within(4_096_999), 4096);
}
