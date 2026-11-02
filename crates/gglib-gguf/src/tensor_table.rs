//! A weights file's tensor table: every tensor's name and shape, read from a
//! GGUF header or a safetensors header without a byte of tensor data.
//!
//! The first four bytes choose the format. A GGUF file starts with its magic;
//! a safetensors file starts with the length of its JSON header, whose low
//! half could only spell `GGUF` for a header of over a gigabyte, which the
//! safetensors reader refuses anyway.

use std::io::Read;
use std::path::Path;

use gglib_core::domain::gguf::{GgufValue, RawMetadata};
use gglib_core::domain::{TensorInfo, TensorPayload, TensorTable, WeightsFormat};

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
    let (tensor_count, metadata_count) = reader.read_counts()?;
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

/// The alignment of a file's tensor data when `general.alignment` does not
/// set one: ggml's `GGUF_DEFAULT_ALIGNMENT`.
const DEFAULT_ALIGNMENT: u64 = 32;

/// The tensor data of a file `file_len` bytes long whose tensor-info table
/// ends at `table_end`: from the aligned start of the data to the end of the
/// file, and the elements of `tensors`. `None` when nothing follows the
/// table, as in a file written without its data, or when the shapes count
/// more elements than a `u64` holds.
pub(crate) fn payload(
    raw: &RawMetadata,
    tensors: &[TensorInfo],
    file_len: u64,
    table_end: u64,
) -> Option<TensorPayload> {
    let alignment = raw
        .get("general.alignment")
        .and_then(GgufValue::as_u64)
        .filter(|alignment| *alignment > 0)
        .unwrap_or(DEFAULT_ALIGNMENT);
    let data_start = table_end.div_ceil(alignment).checked_mul(alignment)?;
    // A count past `u64` is a file that lies about its shapes: no payload.
    let elements = tensors.iter().try_fold(0_u64, |total, tensor| {
        let tensor_elements = tensor
            .shape
            .iter()
            .try_fold(1_u64, |n, dim| n.checked_mul(*dim))?;
        total.checked_add(tensor_elements)
    })?;
    let payload = TensorPayload {
        bytes: file_len.checked_sub(data_start)?,
        elements,
    };
    payload.bits_per_weight().map(|_| payload)
}

#[cfg(test)]
#[path = "tensor_table_tests.rs"]
mod tests;
