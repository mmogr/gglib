//! A safetensors file's header, read as a tensor table.
//!
//! The format is an 8-byte little-endian header length, that many bytes of
//! JSON, then the tensors' data. The JSON is an object from each tensor's name
//! to its `dtype`, `shape` and `data_offsets`, beside an optional
//! `__metadata__` entry of free-form strings. The length is held to a cap and
//! to the bytes the file has left before anything is reserved for it.

use std::io::Read;

use gglib_core::domain::{TensorInfo, TensorTable, WeightsFormat};
use serde_json::Value;

use crate::error::{GgufInternalError, GgufResult};
use crate::reader::GgufReader;

/// The longest header read: the cap the format's reference implementation
/// puts on it. The largest measured, SDXL base 1.0's, is 402,436 bytes.
pub(crate) const MAX_SAFETENSORS_HEADER: u64 = 100_000_000;

/// The entry that holds the file's free-form metadata, not a tensor.
const METADATA_KEY: &str = "__metadata__";

/// The rest of a safetensors file, whose first four bytes, `low`, are the
/// low half of its header length.
pub(crate) fn read<R: Read>(reader: &mut GgufReader<R>, low: [u8; 4]) -> GgufResult<TensorTable> {
    let high = reader.read_tag()?;
    let declared = u64::from(u32::from_le_bytes(low)) | (u64::from(u32::from_le_bytes(high)) << 32);
    if declared > MAX_SAFETENSORS_HEADER {
        return Err(GgufInternalError::Safetensors(format!(
            "header length {declared} is over the {MAX_SAFETENSORS_HEADER} bytes a header may take"
        )));
    }
    let header = reader.read_bytes("safetensors header length", declared)?;
    let Value::Object(entries) = serde_json::from_slice(&header)
        .map_err(|e| GgufInternalError::Safetensors(format!("header is not JSON: {e}")))?
    else {
        return Err(invalid("header is not a JSON object"));
    };
    let tensors = entries
        .into_iter()
        .filter(|(name, _)| name != METADATA_KEY)
        .map(|(name, entry)| {
            let shape = shape_of(&entry).ok_or_else(|| {
                GgufInternalError::Safetensors(format!("tensor {name} has no shape of integers"))
            })?;
            Ok(TensorInfo { name, shape })
        })
        .collect::<GgufResult<Vec<TensorInfo>>>()?;
    Ok(TensorTable {
        format: WeightsFormat::Safetensors,
        architecture: None,
        tensors,
    })
}

/// An entry's `shape`, when it is an array of unsigned integers.
fn shape_of(entry: &Value) -> Option<Vec<u64>> {
    entry
        .get("shape")?
        .as_array()?
        .iter()
        .map(Value::as_u64)
        .collect()
}

fn invalid(message: &str) -> GgufInternalError {
    GgufInternalError::Safetensors(message.to_owned())
}

#[cfg(test)]
#[path = "safetensors_tests.rs"]
mod tests;
