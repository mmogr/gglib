//! The fit sentence and the Context note for each shape of fit, as
//! literals, and `contracts/models/fit_sentence.json`: a fit of each shape
//! beside the words it is served with, which the GUI's contract test reads.

use std::path::PathBuf;

use super::{fit_note, fit_sentence, grouped, tokens};
use crate::domain::{ContextFit, FitInputs};
use crate::server_config::ContextSizeSource;

const GIB: u64 = 1024 * 1024 * 1024;

/// A chat model fitted at 32K of a 128K window on a 48 GiB budget: 2.5 GB of
/// weights and 1,097,000 bytes of KV cache a token, so 40,005 tokens fit in
/// the usable share before snapping.
fn known() -> FitInputs {
    FitInputs {
        budget_bytes: Some(48 * GIB),
        weights_bytes: Some(2_500_000_000),
        kv_bytes_per_token: Some(1_097_000),
        trained_ctx: Some(131_072),
        unsnapped: Some(40_005),
    }
}

fn fit(serves_at: u64, fitted: Option<u64>, inputs: &FitInputs) -> ContextFit {
    ContextFit::new(
        serves_at,
        ContextSizeSource::FittedToHardware,
        fitted,
        inputs,
    )
}

#[test]
fn a_fit_says_the_served_context_of_the_trained_window() {
    let f = fit(32_768, Some(32_768), &known());
    assert_eq!(
        f.sentence,
        "Fits this machine with 32K context, of 128K trained."
    );
    // 2.5e9 + 1,097,000 × 32,768 bytes, of the 90% of 48 GiB the fit spends.
    assert_eq!(
        f.note.as_deref(),
        Some("fits (35.81 GiB of 43.20 GiB usable)")
    );
}

#[test]
fn a_served_context_below_the_fit_still_fits() {
    let f = fit(16_384, Some(32_768), &known());
    assert_eq!(
        f.sentence,
        "Fits this machine with 16K context, of 128K trained."
    );
}

#[test]
fn a_served_context_above_the_fit_says_what_fits() {
    let f = fit(65_536, Some(32_768), &known());
    assert_eq!(
        f.sentence,
        "Serves at 64K context, of 128K trained; 32K fits this machine."
    );
    assert_eq!(
        f.note.as_deref(),
        Some("does not fit (69.28 GiB of 43.20 GiB usable)")
    );
}

#[test]
fn a_refusal_names_each_figure_it_was_missing() {
    let inputs = FitInputs {
        budget_bytes: None,
        kv_bytes_per_token: None,
        trained_ctx: None,
        unsnapped: None,
        ..known()
    };
    let f = fit(4096, None, &inputs);
    assert_eq!(
        f.sentence,
        "Serves at 4K context; gglib could not fit one to this machine: this machine's memory could not be read and the model does not say what its context costs or what it was trained for."
    );
    assert_eq!(f.note, None);

    let weights = FitInputs {
        weights_bytes: None,
        unsnapped: None,
        ..known()
    };
    assert_eq!(
        fit(4096, None, &weights).sentence,
        "Serves at 4K context, of 128K trained; gglib could not fit one to this machine: the weights' size is not known."
    );

    let three = FitInputs {
        budget_bytes: None,
        weights_bytes: None,
        kv_bytes_per_token: None,
        unsnapped: None,
        ..known()
    };
    assert_eq!(
        fit(4096, None, &three).sentence,
        "Serves at 4K context, of 128K trained; gglib could not fit one to this machine: this machine's memory could not be read, the weights' size is not known, and the model does not say what its context costs."
    );
}

#[test]
fn a_model_with_no_trained_window_is_refused_for_it_and_its_memory_still_said() {
    let inputs = FitInputs {
        trained_ctx: None,
        unsnapped: None,
        ..known()
    };
    let f = fit(4096, None, &inputs);
    assert_eq!(
        f.sentence,
        "Serves at 4K context; gglib could not fit one to this machine: the model does not say what it was trained for."
    );
    // Memory is the note's whole question, so the window does not hide it.
    assert_eq!(
        f.note.as_deref(),
        Some("fits (6.51 GiB of 43.20 GiB usable)")
    );
}

#[test]
fn a_served_context_above_the_rung_that_fits_memory_fits() {
    // 36,000 tokens need 39.11 GiB, inside the 43.20 GiB the fit spends,
    // though the rung snapped down to 32K.
    let f = fit(36_000, Some(32_768), &known());
    assert_eq!(
        f.sentence,
        "Fits this machine with 36,000 context, of 128K trained."
    );
    assert_eq!(
        f.note.as_deref(),
        Some("fits (39.11 GiB of 43.20 GiB usable)")
    );
}

#[test]
fn a_context_that_needs_exactly_the_usable_share_fits() {
    // 10 GiB spends 9,663,676,416 bytes; 4K at 1,000 bytes a token is
    // 4,096,000 of them, so these weights leave nothing over.
    let exact = FitInputs {
        budget_bytes: Some(10 * GIB),
        weights_bytes: Some(9_663_676_416 - 4_096_000),
        kv_bytes_per_token: Some(1000),
        unsnapped: Some(4096),
        ..known()
    };
    let f = fit(4096, Some(4096), &exact);
    assert_eq!(
        f.note.as_deref(),
        Some("fits (9.00 GiB of 9.00 GiB usable)")
    );
    let over = FitInputs {
        weights_bytes: exact.weights_bytes.map(|w| w + 1),
        ..exact
    };
    assert_eq!(
        fit(4096, Some(4096), &over).note.as_deref(),
        Some("does not fit (9.00 GiB of 9.00 GiB usable)")
    );
}

#[test]
fn a_rung_the_trained_window_capped_says_nothing_of_memory() {
    // Trained for 32K on 96 GiB: the rung is the window, and 64K served
    // past it still fits in memory.
    let inputs = FitInputs {
        budget_bytes: Some(96 * GIB),
        trained_ctx: Some(32_768),
        unsnapped: Some(32_768),
        ..known()
    };
    let f = fit(65_536, Some(32_768), &inputs);
    assert_eq!(
        f.sentence,
        "Fits this machine with 64K context, more than the 32K it was trained for."
    );
    assert_eq!(
        f.note.as_deref(),
        Some("fits (69.28 GiB of 86.40 GiB usable)")
    );

    // Past the window and past memory: the rung is what fits.
    let small = FitInputs {
        budget_bytes: Some(48 * GIB),
        ..inputs
    };
    assert_eq!(
        fit(65_536, Some(32_768), &small).sentence,
        "Serves at 64K context, more than the 32K it was trained for; 32K fits this machine."
    );
}

#[test]
fn a_refusal_with_every_figure_known_is_a_model_too_big_for_the_smallest_rung() {
    let inputs = FitInputs {
        weights_bytes: Some(60 * GIB),
        unsnapped: None,
        ..known()
    };
    let f = fit(4096, None, &inputs);
    assert_eq!(
        f.sentence,
        "Serves at 4K context, of 128K trained; gglib could not fit one to this machine."
    );
    assert_eq!(
        f.note.as_deref(),
        Some("does not fit (64.18 GiB of 43.20 GiB usable)")
    );
}

#[test]
fn a_context_served_at_the_whole_trained_window_is_of_it_not_more() {
    let inputs = FitInputs {
        trained_ctx: Some(32_768),
        ..known()
    };
    let f = fit(32_768, Some(32_768), &inputs);
    assert_eq!(
        f.sentence,
        "Fits this machine with 32K context, of 32K trained."
    );
}

#[test]
fn a_model_trained_for_less_than_the_smallest_rung_is_refused_for_it() {
    let inputs = FitInputs {
        trained_ctx: Some(2048),
        unsnapped: None,
        ..known()
    };
    let f = fit(4096, None, &inputs);
    assert_eq!(
        f.sentence,
        "Serves at 4K context, more than the 2K it was trained for; gglib could not fit one \
         to this machine: the smallest context gglib fits, 4K, is more than it was trained for."
    );
    // Memory is not why: the weights and a 4K cache fit.
    assert_eq!(
        f.note.as_deref(),
        Some("fits (6.51 GiB of 43.20 GiB usable)")
    );
}

#[test]
fn a_context_that_is_not_whole_thousands_is_its_number_grouped() {
    let f = fit(30_000, Some(32_768), &known());
    assert_eq!(
        f.sentence,
        "Fits this machine with 30,000 context, of 128K trained."
    );
    assert_eq!(tokens(1024), "1K");
    assert_eq!(tokens(0), "0");
    assert_eq!(tokens(1000), "1,000");
    assert_eq!(grouped(999), "999");
    assert_eq!(grouped(1_234_567), "1,234,567");
}

#[test]
fn the_constructor_and_the_functions_agree() {
    let f = fit(65_536, Some(32_768), &known());
    assert_eq!(fit_sentence(&f), f.sentence);
    assert_eq!(fit_note(&f), f.note);
}

/// The fits `contracts/models/fit_sentence.json` records, one of each
/// shape: fits, fits below the rung, serves above it and past memory, a
/// context that is not whole thousands, above the rung yet inside memory,
/// past a trained window that capped the rung, a model too big for the
/// smallest rung, a refusal for
/// each missing figure, and one for a header that says neither what its
/// context costs nor what it was trained for. The first is the GUI
/// fixtures' chat model.
fn recorded() -> Vec<ContextFit> {
    use ContextSizeSource::{BuiltInDefault, Explicit, FittedToHardware, ModelServerDefaults};
    let refused = FitInputs {
        unsnapped: None,
        ..known()
    };
    let refusal = |inputs: FitInputs| ContextFit::new(4096, BuiltInDefault, None, &inputs);
    vec![
        ContextFit::new(32_768, FittedToHardware, Some(32_768), &known()),
        ContextFit::new(16_384, Explicit, Some(32_768), &known()),
        ContextFit::new(65_536, ModelServerDefaults, Some(32_768), &known()),
        ContextFit::new(30_000, Explicit, Some(32_768), &known()),
        ContextFit::new(36_000, Explicit, Some(32_768), &known()),
        ContextFit::new(
            65_536,
            Explicit,
            Some(32_768),
            &FitInputs {
                budget_bytes: Some(96 * GIB),
                trained_ctx: Some(32_768),
                unsnapped: Some(32_768),
                ..known()
            },
        ),
        refusal(FitInputs {
            weights_bytes: Some(60 * GIB),
            ..refused
        }),
        refusal(FitInputs {
            budget_bytes: None,
            ..refused
        }),
        refusal(FitInputs {
            weights_bytes: None,
            ..refused
        }),
        refusal(FitInputs {
            kv_bytes_per_token: None,
            ..refused
        }),
        refusal(FitInputs {
            trained_ctx: None,
            ..refused
        }),
        refusal(FitInputs {
            kv_bytes_per_token: None,
            trained_ctx: None,
            ..refused
        }),
    ]
}

/// The checked-in file is exactly the fits above, each with the words it
/// is served with. Run with `GGLIB_RECORD_CONTRACTS=1` to rewrite it after
/// a deliberate change.
#[test]
fn the_checked_in_fit_sentences_are_the_ones_served() {
    let mut want = serde_json::to_string_pretty(&recorded()).expect("serialise");
    want.push('\n');
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contracts/models/fit_sentence.json");
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        std::fs::write(&path, &want).expect("write fit_sentence.json");
    }
    let have = std::fs::read_to_string(&path).expect("read contracts/models/fit_sentence.json");
    assert!(
        have == want,
        "contracts/models/fit_sentence.json is stale; rerun with GGLIB_RECORD_CONTRACTS=1\n{want}"
    );
}
