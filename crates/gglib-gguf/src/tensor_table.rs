//! A weights file's tensor table: every tensor's name and shape, read from a
//! GGUF header or a safetensors header without a byte of tensor data.
//!
//! The first four bytes choose the format. A GGUF file starts with its magic;
//! a safetensors file starts with the length of its JSON header, whose low
//! half could only spell `GGUF` for a header of over a gigabyte, which the
//! safetensors reader refuses anyway.

use std::io::Read;
use std::path::Path;

use gglib_core::domain::{TensorInfo, TensorTable, WeightsFormat};

use crate::error::{GgufInternalError, GgufResult};
use crate::format::GGUF_MAGIC;
use crate::parser::MIN_PAIR_BYTES;
use crate::reader::{GgufReader, reserve};
use crate::safetensors;

/// The fewest bytes one tensor's entry takes in a GGUF tensor-info table:
/// the length of an empty name (8), the dimension count (4), the type (4)
/// and the data offset (8), for a tensor of no dimensions.
pub(crate) const MIN_TENSOR_INFO_BYTES: u64 = 8 + 4 + 4 + 8;

/// The most dimensions a tensor has: llama.cpp's `GGML_MAX_DIMS`.
pub(crate) const MAX_DIMS: u32 = 4;

/// The tensor table of the file at `path`.
pub(crate) fn read_file(path: &Path) -> GgufResult<TensorTable> {
    read(GgufReader::open(path)?)
}

/// The tensor table from `head`, a file's first bytes.
pub(crate) fn read_head(head: &[u8]) -> GgufResult<TensorTable> {
    read(GgufReader::over(head))
}

fn read<R: Read>(mut reader: GgufReader<R>) -> GgufResult<TensorTable> {
    let tag = reader.read_tag()?;
    if tag == GGUF_MAGIC {
        read_gguf(&mut reader)
    } else {
        safetensors::read(&mut reader, tag)
    }
}

/// The rest of a GGUF file after its magic: the header, the metadata, of
/// which only `general.architecture` is kept, and the tensor-info table.
fn read_gguf<R: Read>(reader: &mut GgufReader<R>) -> GgufResult<TensorTable> {
    let version = reader.read_version()?;
    let tensor_count = read_count(reader, version)?;
    let metadata_count = read_count(reader, version)?;
    let metadata_count = reader.declared_size("metadata count", metadata_count, MIN_PAIR_BYTES)?;
    let mut architecture = None;
    for _ in 0..metadata_count {
        let key = reader.read_string()?;
        let value_type = reader.read_u32()?;
        let value = reader.read_value(value_type)?;
        if key == "general.architecture" {
            architecture = value.as_str().map(str::to_owned);
        }
    }
    Ok(TensorTable {
        format: WeightsFormat::Gguf,
        architecture,
        tensors: read_tensor_infos(reader, tensor_count)?,
    })
}

/// A count in a GGUF header: a `u32` in version 1, a `u64` after.
fn read_count<R: Read>(reader: &mut GgufReader<R>, version: u32) -> GgufResult<u64> {
    if version >= 2 {
        reader.read_u64()
    } else {
        reader.read_u32().map(u64::from)
    }
}

/// The `declared` entries of a GGUF tensor-info table, which follows the
/// metadata.
///
/// The count is held to the bytes the file has left before anything is
/// reserved for it, and a dimension count over [`MAX_DIMS`] is refused before
/// its dimensions are read. ggml stores a shape innermost first; it is
/// reversed here, so a shape reads outermost first as safetensors writes it.
pub(crate) fn read_tensor_infos<R: Read>(
    reader: &mut GgufReader<R>,
    declared: u64,
) -> GgufResult<Vec<TensorInfo>> {
    let count = reader.declared_size("tensor count", declared, MIN_TENSOR_INFO_BYTES)?;
    let mut tensors = reserve("tensor count", count)?;
    for _ in 0..count {
        let name = reader.read_string()?;
        let n_dims = reader.read_u32()?;
        if n_dims > MAX_DIMS {
            return Err(GgufInternalError::TooManyDims {
                name,
                n_dims,
                limit: MAX_DIMS,
            });
        }
        let mut shape = (0..n_dims)
            .map(|_| reader.read_u64())
            .collect::<GgufResult<Vec<u64>>>()?;
        shape.reverse();
        let _tensor_type = reader.read_u32()?;
        let _data_offset = reader.read_u64()?;
        tensors.push(TensorInfo { name, shape });
    }
    Ok(tensors)
}

#[cfg(test)]
#[path = "tensor_table_tests.rs"]
mod tests;
