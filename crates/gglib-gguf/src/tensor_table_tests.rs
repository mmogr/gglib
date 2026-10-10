//! Tests for [`super`]: a GGUF tensor-info table read from a file and from
//! its head, and the malformed tables it refuses before reserving for them.

use gglib_core::domain::{TensorInfo, WeightsFormat};
use gglib_core::{GgufParseError, GgufParserPort};

use super::*;
use crate::{GgufParser, write_safetensors, write_tensor_gguf};

/// The bytes of a GGUF file written by the fixture.
fn gguf_bytes(pairs: &[(&str, &str)], tensors: &[(&str, &[u64])]) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("model.gguf");
    write_tensor_gguf(&path, pairs, tensors);
    std::fs::read(path).unwrap()
}

/// A GGUF v3 header with no metadata that declares `tensor_count` tensors,
/// followed by `rest`, written byte by byte rather than by the fixture.
fn header(tensor_count: u64, rest: &[u8]) -> Vec<u8> {
    [
        &GGUF_MAGIC[..],
        &3_u32.to_le_bytes(),
        &tensor_count.to_le_bytes(),
        &0_u64.to_le_bytes(),
        rest,
    ]
    .concat()
}

/// One tensor-info entry as a file holds it, with `dims` as ggml stores
/// them: innermost first.
fn entry(name: &str, dims: &[u64]) -> Vec<u8> {
    let mut bytes = (name.len() as u64).to_le_bytes().to_vec();
    bytes.extend_from_slice(name.as_bytes());
    bytes.extend_from_slice(&u32::try_from(dims.len()).unwrap().to_le_bytes());
    for dim in dims {
        bytes.extend_from_slice(&dim.to_le_bytes());
    }
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u64.to_le_bytes());
    bytes
}

fn refusal(bytes: &[u8]) -> String {
    match GgufParser::new().tensor_table_of_head(bytes) {
        Err(GgufParseError::InvalidFormat(message)) => message,
        other => panic!("the table is not refused as malformed: {other:?}"),
    }
}

fn info(name: &str, shape: &[u64]) -> TensorInfo {
    TensorInfo {
        name: name.to_owned(),
        shape: shape.to_vec(),
    }
}

/// The table holds every tensor's name and shape in file order, and the
/// architecture from the metadata; the other metadata is passed over.
#[test]
fn a_gguf_table_reads_back_the_tensors_and_architecture_it_was_written_with() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("model.gguf");
    write_tensor_gguf(
        &path,
        &[
            ("general.name", "Qwen3 VL"),
            ("general.architecture", "qwen3vl"),
        ],
        &[
            ("token_embd.weight", &[151_936, 4096]),
            ("blk.0.attn_k_norm.weight", &[128]),
            ("decoder.conv_in.weight", &[512, 16, 3, 3]),
        ],
    );

    let table = GgufParser::new().tensor_table(&path).unwrap();

    assert_eq!(table.format, WeightsFormat::Gguf);
    assert_eq!(table.architecture.as_deref(), Some("qwen3vl"));
    assert_eq!(
        table.tensors,
        vec![
            info("token_embd.weight", &[151_936, 4096]),
            info("blk.0.attn_k_norm.weight", &[128]),
            info("decoder.conv_in.weight", &[512, 16, 3, 3]),
        ]
    );
}

#[test]
fn a_gguf_with_no_architecture_reads_as_none() {
    let bytes = gguf_bytes(&[], &[("img_in.weight", &[3072, 64])]);
    let table = GgufParser::new().tensor_table_of_head(&bytes).unwrap();
    assert_eq!(table.architecture, None);
}

/// ggml stores `ne` innermost first: Flux's `img_in.weight` is `[64, 3072]`
/// on disk and `3072x64` outermost first. Written by hand, so the fixture's
/// own reversal cannot hide a reader that skips it.
#[test]
fn a_gguf_shape_reads_outermost_first() {
    let bytes = header(1, &entry("img_in.weight", &[64, 3072]));
    let table = GgufParser::new().tensor_table_of_head(&bytes).unwrap();
    assert_eq!(table.tensors, vec![info("img_in.weight", &[3072, 64])]);
}

/// A count of 1,000 tensors in a file that holds none would loop on reads
/// past the end; it is refused on its word, by name, before any of that.
#[test]
fn a_tensor_count_the_file_cannot_hold_is_refused_before_reserving() {
    let message = refusal(&header(1000, &[]));
    assert_eq!(
        message,
        "tensor count 1000 is more than the 0 bytes left can hold"
    );
}

#[test]
fn a_tensor_count_the_file_can_just_hold_is_read() {
    let bytes = header(2, &[entry("a", &[]), entry("b", &[])].concat());
    let table = GgufParser::new().tensor_table_of_head(&bytes).unwrap();
    assert_eq!(table.tensors, vec![info("a", &[]), info("b", &[])]);
}

/// ggml has four dimensions at most, and five is refused before a dimension
/// is read; four reads.
#[test]
fn a_tensor_of_more_than_four_dimensions_is_refused() {
    let five = header(1, &entry("decoder.conv1.weight", &[3, 3, 1, 64, 1152]));
    assert_eq!(
        refusal(&five),
        "tensor decoder.conv1.weight has 5 dimensions, more than 4"
    );

    let four = header(1, &entry("decoder.conv_in.weight", &[3, 3, 16, 512]));
    let table = GgufParser::new().tensor_table_of_head(&four).unwrap();
    assert_eq!(table.tensors[0].shape, vec![512, 16, 3, 3]);
}

/// A head that stops anywhere short of the end of the table is an error,
/// never a panic and never a shorter table.
#[test]
fn a_cut_gguf_head_errs_wherever_it_is_cut() {
    let bytes = gguf_bytes(
        &[("general.architecture", "flux")],
        &[
            ("double_blocks.0.img_attn.qkv.weight", &[9216, 3072]),
            ("img_in.weight", &[3072, 64]),
        ],
    );
    let parser = GgufParser::new();
    assert!(parser.tensor_table_of_head(&bytes).is_ok());
    for cut in 0..bytes.len() {
        assert!(
            parser.tensor_table_of_head(&bytes[..cut]).is_err(),
            "a head cut at {cut} of {} was read",
            bytes.len()
        );
    }
}

/// The head form reads what the file form reads, and the bytes after the
/// table (where tensor data would be) are not looked at.
#[test]
fn a_head_reads_the_table_the_file_does() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("model.gguf");
    write_tensor_gguf(
        &path,
        &[],
        &[("single_blocks.0.linear1.weight", &[21504, 3072])],
    );
    let mut bytes = std::fs::read(&path).unwrap();
    let parser = GgufParser::new();
    let from_file = parser.tensor_table(&path).unwrap();
    bytes.extend_from_slice(&[0xAB; 64]);
    assert_eq!(parser.tensor_table_of_head(&bytes).unwrap(), from_file);
}

/// Anything not starting with the GGUF magic is read as safetensors.
#[test]
fn a_file_without_the_gguf_magic_is_read_as_safetensors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ae.safetensors");
    write_safetensors(&path, &[("decoder.conv_in.weight", &[512, 16, 3, 3])]);
    let table = GgufParser::new().tensor_table(&path).unwrap();
    assert_eq!(table.format, WeightsFormat::Safetensors);
    assert_eq!(
        table.tensors,
        vec![info("decoder.conv_in.weight", &[512, 16, 3, 3])]
    );
}

#[test]
fn a_missing_file_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let result = GgufParser::new().tensor_table(&dir.path().join("absent.gguf"));
    assert!(
        matches!(result, Err(GgufParseError::NotFound(_))),
        "{result:?}"
    );
}

/// Two tensors in 30 bytes would fit at a byte each but not at the 24 bytes
/// an entry takes at least, so the count is held to the entry's width.
#[test]
fn a_tensor_count_is_held_to_the_width_of_an_entry() {
    let message = refusal(&header(2, &[0; 30]));
    assert_eq!(
        message,
        "tensor count 2 is more than the 30 bytes left can hold"
    );
}

/// A version 2 header holds its counts as `u64`, as version 3 does.
#[test]
fn a_version_2_header_reads_its_counts_as_u64() {
    let mut bytes = header(1, &entry("img_in.weight", &[64, 3072]));
    bytes[4..8].copy_from_slice(&2_u32.to_le_bytes());
    let table = GgufParser::new().tensor_table_of_head(&bytes).unwrap();
    assert_eq!(table.tensors, vec![info("img_in.weight", &[3072, 64])]);
}
