//! Tests for the text `--list-quants` prints.

use super::*;

fn quant(name: &str, shard_count: usize, total_size: u64) -> HfQuantInfo {
    HfQuantInfo {
        name: name.to_owned(),
        shard_count,
        total_size,
        file_paths: Vec::new(),
    }
}

fn projector(path: &str, size: u64) -> HfFileInfo {
    HfFileInfo {
        path: path.to_owned(),
        size,
        is_gguf: true,
        oid: None,
    }
}

const MIB: u64 = 1_048_576;

#[test]
fn a_repository_without_projectors_lists_its_quantizations_alone() {
    let listing = quant_listing(
        "o/r",
        &[quant("Q4_K_M", 1, 1536 * MIB), quant("Q8_0", 2, 4096 * MIB)],
        &[],
    );

    assert_eq!(
        listing,
        "✓ Found 2 quantizations:\n\
         \x20 Q4_K_M (1536.0 MiB)\n\
         \x20 Q8_0 (4096.0 MiB) (2 shards)\n\
         \n\
         To download a specific quantization, use:\n\
         \x20 gglib model download o/r -q Q4_K_M\n\
         \x20 gglib model download o/r -q Q8_0\n"
    );
}

/// Each projector is listed with its size, in the unit the quantizations
/// use, and marked with the quantizations whose download fetches it.
#[test]
fn the_projectors_are_listed_apart_and_the_fetched_one_is_marked() {
    let listing = quant_listing(
        "o/r",
        &[quant("Q4_K_M", 1, 1536 * MIB), quant("Q8_0", 1, 4096 * MIB)],
        &[
            projector("X.mmproj-Q8_0.gguf", 600 * MIB),
            projector("mmproj-BF16.gguf", 900 * MIB + MIB / 2),
            projector("mmproj-F16.gguf", 900 * MIB),
        ],
    );

    assert_eq!(
        listing,
        "✓ Found 2 quantizations:\n\
         \x20 Q4_K_M (1536.0 MiB)\n\
         \x20 Q8_0 (4096.0 MiB)\n\
         \n\
         Projectors (for image input; a download fetches one with the model):\n\
         \x20 X.mmproj-Q8_0.gguf (600.0 MiB) <- fetched with Q8_0\n\
         \x20 mmproj-BF16.gguf (900.5 MiB)\n\
         \x20 mmproj-F16.gguf (900.0 MiB) <- fetched with Q4_K_M\n\
         \n\
         To download a specific quantization, use:\n\
         \x20 gglib model download o/r -q Q4_K_M\n\
         \x20 gglib model download o/r -q Q8_0\n"
    );
}

/// The projector most quantizations fetch is marked "every other", and the
/// rest by name, so the marks together still say which download fetches what.
#[test]
fn the_projector_most_downloads_fetch_is_marked_every_other() {
    let listing = quant_listing(
        "o/r",
        &[
            quant("BF16", 1, MIB),
            quant("Q4_K_M", 1, MIB),
            quant("Q8_0", 1, MIB),
        ],
        &[
            projector("mmproj-BF16.gguf", MIB),
            projector("mmproj-F16.gguf", MIB),
        ],
    );

    assert!(
        listing.contains(
            "  mmproj-BF16.gguf (1.0 MiB) <- fetched with BF16\n\
             \x20 mmproj-F16.gguf (1.0 MiB) <- fetched with every other quantization\n"
        ),
        "{listing}"
    );
}

#[test]
fn one_projector_for_every_quantization_says_so() {
    let listing = quant_listing(
        "o/r",
        &[quant("Q4_K_M", 1, MIB), quant("Q8_0", 1, MIB)],
        &[projector("mmproj-F16.gguf", MIB)],
    );

    assert!(
        listing.contains("  mmproj-F16.gguf (1.0 MiB) <- fetched with every quantization\n"),
        "{listing}"
    );
}

/// Projectors with no weights beside them are not a model to download.
#[test]
fn a_repository_of_projectors_alone_has_no_gguf_model() {
    let listing = quant_listing("o/r", &[], &[projector("mmproj-F16.gguf", MIB)]);

    assert_eq!(listing, "✗ No GGUF files found in this repository.\n");
}
