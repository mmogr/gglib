//! Tests for the text `--list-quants` prints, from the listing the daemon
//! serves: the fit beside each model in the daemon's words, the projectors
//! apart with the fetched one marked, and a repository of helpers alone.

use gglib_app_services::types::HfProjector;
use gglib_core::domain::{MissingInput, VariantFit};

use super::*;

const MIB: u64 = 1_048_576;

fn variant(label: &str, role: GgufFileRole, size_bytes: u64) -> HfQuantization {
    HfQuantization {
        label: label.to_owned(),
        role,
        file_path: format!("{label}.gguf"),
        size_bytes,
        size_mb: 0.0,
        is_sharded: false,
        shard_count: None,
        projector: None,
        fit: None,
        fit_word: None,
        picked: false,
    }
}

/// A model that fits with `ctx`, as the daemon lists it.
fn model(label: &str, size_bytes: u64, fit: VariantFit) -> HfQuantization {
    HfQuantization {
        fit: Some(fit),
        fit_word: Some(fit.word()),
        ..variant(label, GgufFileRole::Weights, size_bytes)
    }
}

fn projector(path: &str, size_bytes: u64) -> HfQuantization {
    HfQuantization {
        file_path: path.to_owned(),
        ..variant(
            path.trim_end_matches(".gguf"),
            GgufFileRole::Projector,
            size_bytes,
        )
    }
}

fn listing(quantizations: Vec<HfQuantization>) -> HfQuantizationsResponse {
    HfQuantizationsResponse {
        model_id: "o/r".to_owned(),
        quantizations,
        image: None,
    }
}

#[test]
fn each_model_is_listed_with_its_size_shards_fit_and_the_pick_in_the_daemons_words() {
    let mut sharded = model("Q8_0", 4096 * MIB, VariantFit::Tight { ctx: 4096 });
    sharded.is_sharded = true;
    sharded.shard_count = Some(2);
    sharded.picked = true;
    let text = quant_listing(&listing(vec![
        model("Q4_K_M", 1536 * MIB, VariantFit::Fits { ctx: Some(32_768) }),
        sharded,
        model(
            "F16",
            16_384 * MIB,
            VariantFit::Unknown {
                missing: MissingInput::Budget,
            },
        ),
    ]));

    assert_eq!(
        text,
        "✓ Found 3 variants:\n\
         \x20 Q4_K_M (1536.0 MiB) — fits with 32K\n\
         \x20 Q8_0 (4096.0 MiB) (2 shards) — tight at 4K ★ gglib's pick\n\
         \x20 F16 (16384.0 MiB) — unknown: this machine's memory could not be read\n\
         \n\
         To download a specific variant, use:\n\
         \x20 gglib model download o/r -q Q4_K_M\n\
         \x20 gglib model download o/r -q Q8_0\n\
         \x20 gglib model download o/r -q F16\n"
    );
}

/// Each projector is listed apart with its size, in the unit the variants
/// use, and the one every download fetches, named on each model, is marked.
#[test]
fn the_projectors_are_listed_apart_and_the_fetched_one_is_marked() {
    let fetched = HfProjector {
        file_path: "mmproj-BF16.gguf".to_owned(),
        size_bytes: 900 * MIB + MIB / 2,
    };
    let mut q4 = model("Q4_K_M", 1536 * MIB, VariantFit::Fits { ctx: Some(8192) });
    q4.projector = Some(fetched);
    let text = quant_listing(&listing(vec![
        q4,
        projector("X.mmproj-Q8_0.gguf", 600 * MIB),
        projector("mmproj-BF16.gguf", 900 * MIB + MIB / 2),
        projector("mmproj-F16.gguf", 900 * MIB),
    ]));

    assert_eq!(
        text,
        "✓ Found 1 variants:\n\
         \x20 Q4_K_M (1536.0 MiB) — fits with 8K\n\
         \n\
         Projectors (for image input; a download fetches the largest with the model):\n\
         \x20 X.mmproj-Q8_0.gguf (600.0 MiB)\n\
         \x20 mmproj-BF16.gguf (900.5 MiB) <- fetched with every variant\n\
         \x20 mmproj-F16.gguf (900.0 MiB)\n\
         \n\
         To download a specific variant, use:\n\
         \x20 gglib model download o/r -q Q4_K_M\n"
    );
}

/// A draft or helper is marked, offers no fit and no download command, and
/// a repository of them alone says so.
#[test]
fn a_helper_is_marked_and_a_repository_of_helpers_alone_offers_no_download() {
    let text = quant_listing(&listing(vec![variant(
        "draft",
        GgufFileRole::Sidecar,
        300 * MIB,
    )]));

    assert_eq!(
        text,
        format!(
            "✓ Found 1 variants:\n\
             \x20 draft (300.0 MiB) [draft or helper: not downloadable on its own]\n\
             \n\
             ✗ No model here, only draft or helper files: {HELPER_FILES_REFUSAL}.\n"
        )
    );
    assert_eq!(
        quant_listing(&listing(vec![])),
        "✗ No GGUF model found in this repository.\n"
    );
}

/// A repository of projectors alone holds no model to list or download.
#[test]
fn a_repository_of_projectors_alone_has_no_gguf_model() {
    assert_eq!(
        quant_listing(&listing(vec![projector("mmproj-F16.gguf", MIB)])),
        "✗ No GGUF model found in this repository.\n"
    );
}

/// A size is written to a tenth of a MiB, rounded to the nearest and a
/// tie upward, in integers.
#[test]
fn a_size_is_written_to_the_nearest_tenth_of_a_mib_a_tie_upward() {
    assert_eq!(mib(262_144), "0.3 MiB");
    assert_eq!(mib(1_310_720), "1.3 MiB");
    assert_eq!(mib(MIB + 52_427), "1.0 MiB");
    assert_eq!(mib(1536 * MIB), "1536.0 MiB");
}
