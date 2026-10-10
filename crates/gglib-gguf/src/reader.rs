//! GGUF file reader abstraction.
//!
//! This module provides a unified API for reading GGUF files over any
//! [`Read`], which for a file on disk is buffered standard I/O.
//!
//! A model file comes from the internet, so no size it declares is taken on
//! its word. Each is held to the bytes the file has left before anything is
//! reserved or looped over, what is reserved is reserved fallibly, and
//! arrays are followed only so deep. A file that declares more than it holds
//! is an error, where taking its word would end the process.

use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

use gglib_core::domain::gguf::GgufValue;

use crate::error::{GgufInternalError, GgufResult};
use crate::format::GGUF_MAGIC;

/// How deep a file may nest arrays.
///
/// Reading a value recurses once per level, and so does printing or dropping
/// it, so a file free to nest without end overflows the stack. llama.cpp
/// refuses an array of arrays, so no file it loads nests at all.
const MAX_ARRAY_DEPTH: usize = 32;

/// A reader for GGUF files.
///
/// Abstracts the byte source the GGUF primitives are read from.
pub(crate) struct GgufReader<R: Read> {
    reader: R,
    /// The bytes of the file not yet read, which every size the file
    /// declares is held to.
    remaining: u64,
}

impl GgufReader<BufReader<File>> {
    /// Open a GGUF file for reading, through a buffer.
    pub(crate) fn open(path: &Path) -> GgufResult<Self> {
        let file = File::open(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                GgufInternalError::FileNotFound(path.display().to_string())
            } else {
                GgufInternalError::Io(e)
            }
        })?;
        let remaining = file.metadata()?.len();
        let reader = BufReader::new(file);
        Ok(Self { reader, remaining })
    }
}

impl<'a> GgufReader<&'a [u8]> {
    /// A reader over `head`, the first bytes of a file, as over a file that
    /// holds exactly them: a size the file declares is held to what is left
    /// of `head`, and a read past its end is an error, never a panic.
    pub(crate) const fn over(head: &'a [u8]) -> Self {
        Self {
            reader: head,
            remaining: head.len() as u64,
        }
    }
}

impl<R: Read> GgufReader<R> {
    /// Fill `buf` from the source, and count its bytes off what the file
    /// has left.
    fn fill(&mut self, buf: &mut [u8]) -> GgufResult<()> {
        self.reader.read_exact(buf)?;
        self.count_off(buf.len() as u64);
        Ok(())
    }

    /// Count `bytes` that were read off what the file has left.
    ///
    /// A file that grew after it was opened reads past the length it had,
    /// and by this count has nothing left.
    const fn count_off(&mut self, bytes: u64) {
        self.remaining = self.remaining.saturating_sub(bytes);
    }

    /// Hold a size the file declares to the bytes it has left.
    ///
    /// `declared` items of at least `width` bytes each do not fit in fewer
    /// bytes than that, so a size the rest of the file cannot hold is
    /// refused here, under the name `what`, before anything is reserved or
    /// looped over on its word.
    pub(crate) fn declared_size(
        &self,
        what: &'static str,
        declared: u64,
        width: u64,
    ) -> GgufResult<usize> {
        match usize::try_from(declared) {
            Ok(size) if declared <= self.remaining / width => Ok(size),
            _ => Err(GgufInternalError::DeclaredTooLarge {
                what,
                declared,
                remaining: self.remaining,
            }),
        }
    }

    /// Read and validate the GGUF magic number.
    pub(crate) fn read_magic(&mut self) -> GgufResult<()> {
        if self.read_tag()? != GGUF_MAGIC {
            return Err(GgufInternalError::InvalidMagic);
        }
        Ok(())
    }

    /// Read the first four bytes of a file, whatever they are: the GGUF
    /// magic, or the low half of a safetensors header length.
    pub(crate) fn read_tag(&mut self) -> GgufResult<[u8; 4]> {
        let mut tag = [0u8; 4];
        self.fill(&mut tag)?;
        Ok(tag)
    }

    /// Read and validate the GGUF version.
    pub(crate) fn read_version(&mut self) -> GgufResult<u32> {
        let version = self.read_u32()?;
        if !(1..=3).contains(&version) {
            return Err(GgufInternalError::UnsupportedVersion(version));
        }
        Ok(version)
    }

    /// Read a u8 value.
    pub(crate) fn read_u8(&mut self) -> GgufResult<u8> {
        let mut buf = [0u8; 1];
        self.fill(&mut buf)?;
        Ok(buf[0])
    }

    /// Read an i8 value.
    #[allow(clippy::cast_possible_wrap)]
    pub(crate) fn read_i8(&mut self) -> GgufResult<i8> {
        Ok(self.read_u8()? as i8)
    }

    /// Read a u16 value (little-endian).
    pub(crate) fn read_u16(&mut self) -> GgufResult<u16> {
        let mut buf = [0u8; 2];
        self.fill(&mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    /// Read an i16 value (little-endian).
    #[allow(clippy::cast_possible_wrap)]
    pub(crate) fn read_i16(&mut self) -> GgufResult<i16> {
        Ok(self.read_u16()? as i16)
    }

    /// Read a u32 value (little-endian).
    pub(crate) fn read_u32(&mut self) -> GgufResult<u32> {
        let mut buf = [0u8; 4];
        self.fill(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    /// Read an i32 value (little-endian).
    #[allow(clippy::cast_possible_wrap)]
    pub(crate) fn read_i32(&mut self) -> GgufResult<i32> {
        Ok(self.read_u32()? as i32)
    }

    /// Read a u64 value (little-endian).
    pub(crate) fn read_u64(&mut self) -> GgufResult<u64> {
        let mut buf = [0u8; 8];
        self.fill(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    /// Read an i64 value (little-endian).
    #[allow(clippy::cast_possible_wrap)]
    pub(crate) fn read_i64(&mut self) -> GgufResult<i64> {
        Ok(self.read_u64()? as i64)
    }

    /// Read an f32 value (little-endian).
    pub(crate) fn read_f32(&mut self) -> GgufResult<f32> {
        let mut buf = [0u8; 4];
        self.fill(&mut buf)?;
        Ok(f32::from_le_bytes(buf))
    }

    /// Read an f64 value (little-endian).
    pub(crate) fn read_f64(&mut self) -> GgufResult<f64> {
        let mut buf = [0u8; 8];
        self.fill(&mut buf)?;
        Ok(f64::from_le_bytes(buf))
    }

    /// Read a bool value.
    pub(crate) fn read_bool(&mut self) -> GgufResult<bool> {
        Ok(self.read_u8()? != 0)
    }

    /// Read a string (u64 length prefix followed by UTF-8 bytes).
    pub(crate) fn read_string(&mut self) -> GgufResult<String> {
        let declared = self.read_u64()?;
        let buf = self.read_bytes("string length", declared)?;
        String::from_utf8(buf).map_err(|_| GgufInternalError::Utf8Error)
    }

    /// Read `declared` bytes, a length the file gave under the name `what`,
    /// once it is held to the bytes the file has left.
    pub(crate) fn read_bytes(&mut self, what: &'static str, declared: u64) -> GgufResult<Vec<u8>> {
        let len = self.declared_size(what, declared, 1)?;
        let mut buf = reserve(what, len)?;
        // Memory is written to only as bytes arrive, so a length the source
        // does not make good costs none.
        let read = self.reader.by_ref().take(declared).read_to_end(&mut buf)?;
        self.count_off(declared);
        if read < len {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
        }
        Ok(buf)
    }

    /// Read a GGUF value based on its type code.
    pub(crate) fn read_value(&mut self, value_type: u32) -> GgufResult<GgufValue> {
        self.read_value_at(value_type, 0)
    }

    /// Read a value that `depth` arrays enclose.
    fn read_value_at(&mut self, value_type: u32, depth: usize) -> GgufResult<GgufValue> {
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
                if depth == MAX_ARRAY_DEPTH {
                    return Err(GgufInternalError::ArraysTooDeep(MAX_ARRAY_DEPTH));
                }
                let element_type = self.read_u32()?;
                let declared = self.read_u64()?;
                let count = self.declared_size("array count", declared, min_width(element_type))?;
                let mut elements = reserve("array count", count)?;

                for _ in 0..count {
                    elements.push(self.read_value_at(element_type, depth + 1)?);
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

/// The fewest bytes one value of `value_type` takes in a file.
///
/// A string is at least its length, and an array at least its element type
/// and its count. One byte is a `u8`, an `i8` or a bool, and the floor for a
/// type the format does not have, which is refused when a value of it is
/// read.
const fn min_width(value_type: u32) -> u64 {
    match value_type {
        2 | 3 => 2,
        4..=6 => 4,
        8 | 10..=12 => 8,
        9 => 12,
        _ => 1,
    }
}

/// Room for `count` items a file declared, where the machine has it.
///
/// A size the file can hold may still be more than there is memory for, and
/// reserving that on the file's word would abort the process.
pub(crate) fn reserve<T>(what: &'static str, count: usize) -> GgufResult<Vec<T>> {
    let mut items = Vec::new();
    items.try_reserve_exact(count).map_err(|_| {
        io::Error::new(
            io::ErrorKind::OutOfMemory,
            format!("no memory for {what} {count}"),
        )
    })?;
    Ok(items)
}

#[cfg(test)]
#[path = "reader_tests.rs"]
mod tests;
