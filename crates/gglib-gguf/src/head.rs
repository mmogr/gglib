//! The metadata of a GGUF file read from its first bytes, as far as they go
//! (`GgufParserPort::parse_head`).
//!
//! A file is judged before it is downloaded from a head of it, whose size
//! is fixed in advance. The keys a fit is judged by precede the tokenizer's
//! arrays in what converters write, within the first two kilobytes, while
//! the arrays and the chat template after them run to megabytes. So a head
//! ends somewhere in the metadata, and where it ends is no error: the pairs
//! it holds whole are kept.
//!
//! The magic is the one thing a head must hold, since without it there is
//! no telling the bytes are a GGUF file. After it, the head running out, or
//! a size the file declares running past the head, ends the read. What the
//! head holds is still read as strictly as a whole file: a malformed value
//! is an error, not a cut.

use std::io::{ErrorKind, Read};
use std::path::Path;

use gglib_core::HeadMetadata;
use gglib_core::domain::gguf::RawMetadata;

use crate::error::{GgufInternalError, GgufResult};
use crate::parser::extract_metadata;
use crate::reader::GgufReader;

/// The metadata `head` holds, of a file named `file_name`.
pub(crate) fn read(head: &[u8], file_name: &str) -> GgufResult<HeadMetadata> {
    let mut reader = GgufReader::over(head);
    reader.read_magic()?;
    let mut found = Found::default();
    match read_pairs(&mut reader, head.len() as u64, &mut found) {
        Ok(()) => found.complete = true,
        Err(error) if ran_out(&error) => {}
        Err(error) => return Err(error),
    }
    Ok(HeadMetadata {
        metadata: extract_metadata(&found.pairs, Path::new(file_name)),
        reached_tokenizer: found.reached_tokenizer,
        complete: found.complete,
        bytes_used: found.bytes_used,
    })
}

/// What a head has given so far.
#[derive(Default)]
struct Found {
    pairs: RawMetadata,
    reached_tokenizer: bool,
    complete: bool,
    bytes_used: u64,
}

/// Read the header after the magic and the metadata pairs into `found`,
/// until the pairs end or the head, `head_len` bytes long, does.
///
/// The metadata count is the one size not held to the bytes left: those are
/// the head's, not the file's. Nothing is reserved on it, and every pair
/// takes bytes of the head, so the head ending ends the loop.
fn read_pairs<R: Read>(
    reader: &mut GgufReader<R>,
    head_len: u64,
    found: &mut Found,
) -> GgufResult<()> {
    let used = |reader: &GgufReader<R>| head_len - reader.remaining();
    let (_tensor_count, metadata_count) = reader.read_counts()?;
    found.bytes_used = used(reader);
    for _ in 0..metadata_count {
        let key = reader.read_string()?;
        found.reached_tokenizer |= key.starts_with("tokenizer.");
        let value_type = reader.read_u32()?;
        let value = reader.read_value(value_type)?;
        found.pairs.insert(key, value);
        found.bytes_used = used(reader);
    }
    Ok(())
}

/// Whether `error` is the head running out: a read past its end, or a size
/// the file declares that the bytes left in it cannot hold.
fn ran_out(error: &GgufInternalError) -> bool {
    match error {
        GgufInternalError::Io(error) => error.kind() == ErrorKind::UnexpectedEof,
        GgufInternalError::DeclaredTooLarge { .. } => true,
        _ => false,
    }
}

#[cfg(test)]
#[path = "head_tests.rs"]
mod tests;
