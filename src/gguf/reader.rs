//! Low-level binary readers for GGUF format.
//!
//! This module provides pure reader functions that operate on `&mut impl Read`.
//! These are the building blocks for parsing GGUF files.

use std::io::Read;

use crate::gguf::error::{GgufError, GgufResult};
use crate::gguf::types::GgufValue;

/// GGUF magic number (4 bytes): "GGUF"
pub const GGUF_MAGIC: [u8; 4] = [0x47, 0x47, 0x55, 0x46];

/// Read a u8 value from the reader.
pub fn read_u8<R: Read>(reader: &mut R) -> GgufResult<u8> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf)?;
    Ok(buf[0])
}

/// Read an i8 value from the reader.
pub fn read_i8<R: Read>(reader: &mut R) -> GgufResult<i8> {
    Ok(read_u8(reader)? as i8)
}

/// Read a u16 value from the reader (little-endian).
pub fn read_u16<R: Read>(reader: &mut R) -> GgufResult<u16> {
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf)?;
    Ok(u16::from_le_bytes(buf))
}

/// Read an i16 value from the reader (little-endian).
pub fn read_i16<R: Read>(reader: &mut R) -> GgufResult<i16> {
    Ok(read_u16(reader)? as i16)
}

/// Read a u32 value from the reader (little-endian).
pub fn read_u32<R: Read>(reader: &mut R) -> GgufResult<u32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

/// Read an i32 value from the reader (little-endian).
pub fn read_i32<R: Read>(reader: &mut R) -> GgufResult<i32> {
    Ok(read_u32(reader)? as i32)
}

/// Read a u64 value from the reader (little-endian).
pub fn read_u64<R: Read>(reader: &mut R) -> GgufResult<u64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

/// Read an i64 value from the reader (little-endian).
pub fn read_i64<R: Read>(reader: &mut R) -> GgufResult<i64> {
    Ok(read_u64(reader)? as i64)
}

/// Read an f32 value from the reader (little-endian).
pub fn read_f32<R: Read>(reader: &mut R) -> GgufResult<f32> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(f32::from_le_bytes(buf))
}

/// Read an f64 value from the reader (little-endian).
pub fn read_f64<R: Read>(reader: &mut R) -> GgufResult<f64> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(f64::from_le_bytes(buf))
}

/// Read a bool value from the reader.
pub fn read_bool<R: Read>(reader: &mut R) -> GgufResult<bool> {
    Ok(read_u8(reader)? != 0)
}

/// Read a string from the reader.
///
/// GGUF strings are prefixed with a u64 length.
pub fn read_string<R: Read>(reader: &mut R) -> GgufResult<String> {
    let len = read_u64(reader)? as usize;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|_| GgufError::Utf8Error)
}

/// Read a GGUF value based on its type code.
///
/// Type codes:
/// - 0: U8
/// - 1: I8
/// - 2: U16
/// - 3: I16
/// - 4: U32
/// - 5: I32
/// - 6: F32
/// - 7: Bool
/// - 8: String
/// - 9: Array
/// - 10: U64
/// - 11: I64
/// - 12: F64
pub fn read_value<R: Read>(reader: &mut R, value_type: u32) -> GgufResult<GgufValue> {
    match value_type {
        0 => Ok(GgufValue::U8(read_u8(reader)?)),
        1 => Ok(GgufValue::I8(read_i8(reader)?)),
        2 => Ok(GgufValue::U16(read_u16(reader)?)),
        3 => Ok(GgufValue::I16(read_i16(reader)?)),
        4 => Ok(GgufValue::U32(read_u32(reader)?)),
        5 => Ok(GgufValue::I32(read_i32(reader)?)),
        6 => Ok(GgufValue::F32(read_f32(reader)?)),
        7 => Ok(GgufValue::Bool(read_bool(reader)?)),
        8 => Ok(GgufValue::String(read_string(reader)?)),
        9 => {
            // Array type
            let element_type = read_u32(reader)?;
            let count = read_u64(reader)? as usize;
            let mut elements = Vec::with_capacity(count);

            for _ in 0..count {
                elements.push(read_value(reader, element_type)?);
            }

            Ok(GgufValue::Array(elements))
        }
        10 => Ok(GgufValue::U64(read_u64(reader)?)),
        11 => Ok(GgufValue::I64(read_i64(reader)?)),
        12 => Ok(GgufValue::F64(read_f64(reader)?)),
        _ => Err(GgufError::InvalidValueType(value_type)),
    }
}

/// Read and validate the GGUF magic number.
pub fn read_magic<R: Read>(reader: &mut R) -> GgufResult<()> {
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;
    if magic != GGUF_MAGIC {
        return Err(GgufError::InvalidMagic);
    }
    Ok(())
}

/// Read and validate the GGUF version.
///
/// Returns the version number if valid (1-3), otherwise an error.
pub fn read_version<R: Read>(reader: &mut R) -> GgufResult<u32> {
    let version = read_u32(reader)?;
    if !(1..=3).contains(&version) {
        return Err(GgufError::UnsupportedVersion(version));
    }
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_read_u32() {
        let data = [0x01, 0x02, 0x03, 0x04];
        let mut cursor = Cursor::new(data);
        assert_eq!(read_u32(&mut cursor).unwrap(), 0x04030201);
    }

    #[test]
    fn test_read_string() {
        // Length (u64 LE) = 5, then "hello"
        let data = [
            0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'h', b'e', b'l', b'l', b'o',
        ];
        let mut cursor = Cursor::new(data);
        assert_eq!(read_string(&mut cursor).unwrap(), "hello");
    }

    #[test]
    fn test_read_magic_valid() {
        let mut cursor = Cursor::new(GGUF_MAGIC);
        assert!(read_magic(&mut cursor).is_ok());
    }

    #[test]
    fn test_read_magic_invalid() {
        let mut cursor = Cursor::new([0x00, 0x00, 0x00, 0x00]);
        assert!(matches!(
            read_magic(&mut cursor),
            Err(GgufError::InvalidMagic)
        ));
    }

    #[test]
    fn test_read_version_valid() {
        let data = [0x02, 0x00, 0x00, 0x00]; // version 2
        let mut cursor = Cursor::new(data);
        assert_eq!(read_version(&mut cursor).unwrap(), 2);
    }

    #[test]
    fn test_read_version_invalid() {
        let data = [0x05, 0x00, 0x00, 0x00]; // version 5 - unsupported
        let mut cursor = Cursor::new(data);
        assert!(matches!(
            read_version(&mut cursor),
            Err(GgufError::UnsupportedVersion(5))
        ));
    }

    #[test]
    fn test_read_value_u32() {
        let data = [0x2A, 0x00, 0x00, 0x00]; // 42
        let mut cursor = Cursor::new(data);
        let value = read_value(&mut cursor, 4).unwrap();
        assert!(matches!(value, GgufValue::U32(42)));
    }

    #[test]
    fn test_read_value_bool() {
        let data = [0x01];
        let mut cursor = Cursor::new(data);
        let value = read_value(&mut cursor, 7).unwrap();
        assert!(matches!(value, GgufValue::Bool(true)));
    }
}
