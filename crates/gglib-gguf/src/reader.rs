//! GGUF file reader abstraction.
//!
//! This module provides a unified API for reading GGUF files,
//! hiding the underlying I/O strategy (mmap or standard file I/O).

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use gglib_core::domain::gguf::GgufValue;

use crate::error::{GgufInternalError, GgufResult};
use crate::format::GGUF_MAGIC;

/// A reader for GGUF files.
///
/// Abstracts file I/O, potentially using memory mapping for better performance.
pub struct GgufReader<R: Read> {
    reader: R,
}

impl GgufReader<BufReader<File>> {
    /// Open a GGUF file for reading.
    ///
    /// Uses buffered I/O for standard reading. Memory mapping is handled
    /// separately when the `mmap` feature is enabled.
    pub fn open(path: &Path) -> GgufResult<Self> {
        let file = File::open(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                GgufInternalError::FileNotFound(path.display().to_string())
            } else {
                GgufInternalError::Io(e)
            }
        })?;
        let reader = BufReader::new(file);
        Ok(Self { reader })
    }
}

impl<R: Read> GgufReader<R> {
    /// Create a reader from any Read implementation (useful for testing).
    #[cfg(test)]
    const fn from_reader(reader: R) -> Self {
        Self { reader }
    }

    /// Read and validate the GGUF magic number.
    pub fn read_magic(&mut self) -> GgufResult<()> {
        let mut magic = [0u8; 4];
        self.reader.read_exact(&mut magic)?;
        if magic != GGUF_MAGIC {
            return Err(GgufInternalError::InvalidMagic);
        }
        Ok(())
    }

    /// Read and validate the GGUF version.
    pub fn read_version(&mut self) -> GgufResult<u32> {
        let version = self.read_u32()?;
        if !(1..=3).contains(&version) {
            return Err(GgufInternalError::UnsupportedVersion(version));
        }
        Ok(version)
    }

    /// Read a u8 value.
    pub fn read_u8(&mut self) -> GgufResult<u8> {
        let mut buf = [0u8; 1];
        self.reader.read_exact(&mut buf)?;
        Ok(buf[0])
    }

    /// Read an i8 value.
    #[allow(clippy::cast_possible_wrap)]
    pub fn read_i8(&mut self) -> GgufResult<i8> {
        Ok(self.read_u8()? as i8)
    }

    /// Read a u16 value (little-endian).
    pub fn read_u16(&mut self) -> GgufResult<u16> {
        let mut buf = [0u8; 2];
        self.reader.read_exact(&mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    /// Read an i16 value (little-endian).
    #[allow(clippy::cast_possible_wrap)]
    pub fn read_i16(&mut self) -> GgufResult<i16> {
        Ok(self.read_u16()? as i16)
    }

    /// Read a u32 value (little-endian).
    pub fn read_u32(&mut self) -> GgufResult<u32> {
        let mut buf = [0u8; 4];
        self.reader.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    /// Read an i32 value (little-endian).
    #[allow(clippy::cast_possible_wrap)]
    pub fn read_i32(&mut self) -> GgufResult<i32> {
        Ok(self.read_u32()? as i32)
    }

    /// Read a u64 value (little-endian).
    pub fn read_u64(&mut self) -> GgufResult<u64> {
        let mut buf = [0u8; 8];
        self.reader.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    /// Read an i64 value (little-endian).
    #[allow(clippy::cast_possible_wrap)]
    pub fn read_i64(&mut self) -> GgufResult<i64> {
        Ok(self.read_u64()? as i64)
    }

    /// Read an f32 value (little-endian).
    pub fn read_f32(&mut self) -> GgufResult<f32> {
        let mut buf = [0u8; 4];
        self.reader.read_exact(&mut buf)?;
        Ok(f32::from_le_bytes(buf))
    }

    /// Read an f64 value (little-endian).
    pub fn read_f64(&mut self) -> GgufResult<f64> {
        let mut buf = [0u8; 8];
        self.reader.read_exact(&mut buf)?;
        Ok(f64::from_le_bytes(buf))
    }

    /// Read a bool value.
    pub fn read_bool(&mut self) -> GgufResult<bool> {
        Ok(self.read_u8()? != 0)
    }

    /// Read a string (u64 length prefix followed by UTF-8 bytes).
    #[allow(clippy::cast_possible_truncation)]
    pub fn read_string(&mut self) -> GgufResult<String> {
        let len = self.read_u64()? as usize;
        let mut buf = vec![0u8; len];
        self.reader.read_exact(&mut buf)?;
        String::from_utf8(buf).map_err(|_| GgufInternalError::Utf8Error)
    }

    /// Read a GGUF value based on its type code.
    #[allow(clippy::cast_possible_truncation)]
    pub fn read_value(&mut self, value_type: u32) -> GgufResult<GgufValue> {
        match value_type {
            0 => Ok(GgufValue::U8(self.read_u8()?)),
            1 => Ok(GgufValue::I8(self.read_i8()?)),
            2 => Ok(GgufValue::U16(self.read_u16()?)),
            3 => Ok(GgufValue::I16(self.read_i16()?)),
            4 => Ok(GgufValue::U32(self.read_u32()?)),
            5 => Ok(GgufValue::I32(self.read_i32()?)),
            6 => Ok(GgufValue::F32(self.read_f32()?)),
            7 => Ok(GgufValue::Bool(self.read_bool()?)),
            8 => Ok(GgufValue::String(self.read_string()?)),
            9 => {
                // Array type
                let element_type = self.read_u32()?;
                let count = self.read_u64()? as usize;
                let mut elements = Vec::with_capacity(count);

                for _ in 0..count {
                    elements.push(self.read_value(element_type)?);
                }

                Ok(GgufValue::Array(elements))
            }
            10 => Ok(GgufValue::U64(self.read_u64()?)),
            11 => Ok(GgufValue::I64(self.read_i64()?)),
            12 => Ok(GgufValue::F64(self.read_f64()?)),
            _ => Err(GgufInternalError::InvalidValueType(value_type)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_read_u32() {
        let data = [0x01, 0x02, 0x03, 0x04];
        let mut reader = GgufReader::from_reader(Cursor::new(data));
        assert_eq!(reader.read_u32().unwrap(), 0x0403_0201);
    }

    #[test]
    fn test_read_string() {
        // Length (u64 LE) = 5, then "hello"
        let data = [
            0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'h', b'e', b'l', b'l', b'o',
        ];
        let mut reader = GgufReader::from_reader(Cursor::new(data));
        assert_eq!(reader.read_string().unwrap(), "hello");
    }

    #[test]
    fn test_read_magic_valid() {
        let mut reader = GgufReader::from_reader(Cursor::new(GGUF_MAGIC));
        assert!(reader.read_magic().is_ok());
    }

    #[test]
    fn test_read_magic_invalid() {
        let mut reader = GgufReader::from_reader(Cursor::new([0x00, 0x00, 0x00, 0x00]));
        assert!(matches!(
            reader.read_magic(),
            Err(GgufInternalError::InvalidMagic)
        ));
    }

    #[test]
    fn test_read_version_valid() {
        let data = [0x02, 0x00, 0x00, 0x00]; // version 2
        let mut reader = GgufReader::from_reader(Cursor::new(data));
        assert_eq!(reader.read_version().unwrap(), 2);
    }

    #[test]
    fn test_read_version_invalid() {
        let data = [0x05, 0x00, 0x00, 0x00]; // version 5 - unsupported
        let mut reader = GgufReader::from_reader(Cursor::new(data));
        assert!(matches!(
            reader.read_version(),
            Err(GgufInternalError::UnsupportedVersion(5))
        ));
    }

    #[test]
    fn test_read_value_u32() {
        let data = [0x2A, 0x00, 0x00, 0x00]; // 42
        let mut reader = GgufReader::from_reader(Cursor::new(data));
        let value = reader.read_value(4).unwrap();
        assert!(matches!(value, GgufValue::U32(42)));
    }

    #[test]
    fn test_read_value_bool() {
        let data = [0x01];
        let mut reader = GgufReader::from_reader(Cursor::new(data));
        let value = reader.read_value(7).unwrap();
        assert!(matches!(value, GgufValue::Bool(true)));
    }
}
