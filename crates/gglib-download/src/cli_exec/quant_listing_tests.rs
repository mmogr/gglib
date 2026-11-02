//! Tests for the text `--list-quants` prints.

use super::*;

fn quant(label: &str, shard_count: usize, total_size: u64) -> HfQuantInfo {
    HfQuantInfo {
        label: label.to_owned(),
        role: Some(GgufFileRole::Weights),
        total_size,
        file_paths: (1..=shard_count)
            .map(|shard| format!("{label}-{shard:05}-of-{shard_count:05}.gguf"))
            .collect(),
    }
}

fn of_role(role: GgufFileRole, path: &str, total_size: u64) -> HfQuantInfo {
    HfQuantInfo {
        role: Some(role),
        file_paths: vec![path.to_owned()],
        ..quant(path.trim_end_matches(".gguf"), 1, total_size)
    }
}

fn projector(path: &str, size: u64) -> HfQuantInfo {
    of_role(GgufFileRole::Projector, path, size)
}

const MIB: u64 = 1_048_576;

#[test]
fn a_repository_without_projectors_lists_its_quantizations_alone() {
    let listing = quant_listing(
        "o/r",
        &[quant("Q4_K_M", 1, 1536 * MIB), quant("Q8_0", 2, 4096 * MIB)],
    );

    assert_eq!(
        listing,
        "✓ Found 2 variants:\n\
         \x20 Q4_K_M (1536.0 MiB)\n\
         \x20 Q8_0 (4096.0 MiB) (2 shards)\n\
         \n\
         To download a specific variant, use:\n\
         \x20 gglib model download o/r -q Q4_K_M\n\
         \x20 gglib model download o/r -q Q8_0\n"
    );
}

/// Each projector is listed apart with its size, in the unit the variants
/// use, and the largest, the one every download fetches, is marked.
#[test]
fn the_projectors_are_listed_apart_and_the_largest_is_marked() {
    let listing = quant_listing(
        "o/r",
        &[
            quant("Q4_K_M", 1, 1536 * MIB),
            projector("X.mmproj-Q8_0.gguf", 600 * MIB),
            projector("mmproj-BF16.gguf", 900 * MIB + MIB / 2),
            projector("mmproj-F16.gguf", 900 * MIB),
            quant("Q8_0", 1, 4096 * MIB),
        ],
    );

    assert_eq!(
        listing,
        "✓ Found 2 variants:\n\
         \x20 Q4_K_M (1536.0 MiB)\n\
         \x20 Q8_0 (4096.0 MiB)\n\
         \n\
         Projectors (for image input; a download fetches the largest with the model):\n\
         \x20 X.mmproj-Q8_0.gguf (600.0 MiB)\n\
         \x20 mmproj-BF16.gguf (900.5 MiB) <- fetched with every variant\n\
         \x20 mmproj-F16.gguf (900.0 MiB)\n\
         \n\
         To download a specific variant, use:\n\
         \x20 gglib model download o/r -q Q4_K_M\n\
         \x20 gglib model download o/r -q Q8_0\n"
    );
}

/// A file named as a projector whose header says it is weights is a model,
/// and one named as anything whose header says projector is a projector.
#[test]
fn a_projector_is_what_its_header_says() {
    let listing = quant_listing(
        "o/r",
        &[
            quant("mmproj-looking", 1, MIB),
            projector("vision.gguf", 2 * MIB),
        ],
    );

    assert!(
        listing.contains("  mmproj-looking (1.0 MiB)\n"),
        "{listing}"
    );
    assert!(
        listing.contains("  vision.gguf (2.0 MiB) <- fetched with every variant\n"),
        "{listing}"
    );
}

/// Projectors with no weights beside them are not a model to download.
#[test]
fn a_repository_of_projectors_alone_has_no_gguf_model() {
    let listing = quant_listing("o/r", &[projector("mmproj-F16.gguf", MIB)]);

    assert_eq!(listing, "✗ No GGUF model found in this repository.\n");
}

/// A draft is listed and marked, and no command downloads it.
#[test]
fn a_draft_is_marked_and_offered_no_download() {
    let listing = quant_listing(
        "o/r",
        &[
            of_role(GgufFileRole::Sidecar, "FastMTP-32K.gguf", MIB),
            quant("Q4_K_P", 1, MIB),
        ],
    );

    assert_eq!(
        listing,
        "✓ Found 2 variants:\n\
         \x20 FastMTP-32K (1.0 MiB) [draft or helper: not downloadable on its own]\n\
         \x20 Q4_K_P (1.0 MiB)\n\
         \n\
         To download a specific variant, use:\n\
         \x20 gglib model download o/r -q Q4_K_P\n"
    );
}

#[test]
fn a_repository_of_drafts_alone_says_so() {
    let listing = quant_listing("o/r", &[of_role(GgufFileRole::Sidecar, "dflash.gguf", MIB)]);

    assert!(
        listing.ends_with(&format!(
            "only draft or helper files: {HELPER_FILES_REFUSAL}.\n"
        )),
        "{listing}"
    );
}
