//! Shared test helpers.

use std::path::Path;

pub(crate) use gglib_db::setup_test_database as setup_test_pool;

/// Write a minimal valid GGUF file containing only the given string
/// `general.*`-style metadata pairs (no tensors).
///
/// Format (matches `gglib_gguf::reader::GgufReader`): magic `b"GGUF"`,
/// version `u32` LE, tensor count `u64` LE (0), metadata count `u64` LE,
/// then per pair: key length `u64` LE + UTF-8 bytes, value type `u32` LE
/// (`8` = String), value length `u64` LE + UTF-8 bytes.
///
/// `mod common` is compiled once per integration-test binary, and not every
/// binary that includes it calls every helper -- allowed here rather than
/// split into a per-consumer module.
#[allow(dead_code)]
pub(crate) fn write_gguf_fixture(path: &Path, pairs: &[(&str, &str)]) {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"GGUF");
    buf.extend_from_slice(&3u32.to_le_bytes()); // version
    buf.extend_from_slice(&0u64.to_le_bytes()); // tensor_count
    buf.extend_from_slice(&(pairs.len() as u64).to_le_bytes()); // metadata_count

    for (key, value) in pairs {
        buf.extend_from_slice(&(key.len() as u64).to_le_bytes());
        buf.extend_from_slice(key.as_bytes());
        buf.extend_from_slice(&8u32.to_le_bytes()); // String type
        buf.extend_from_slice(&(value.len() as u64).to_le_bytes());
        buf.extend_from_slice(value.as_bytes());
    }

    std::fs::write(path, buf).unwrap();
}
