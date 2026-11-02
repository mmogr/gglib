//! A [`GgufParserPort`] that reads a file's role from its first bytes, for
//! tests.

use std::path::Path;

use super::{
    GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort, HeadMetadata, TensorTable,
};
use crate::download::GgufFileRole;

/// Reads a file's role from its first bytes, `projector` or `weights`, as
/// the real parser reads it from the header, and a head's the same way.
///
/// A file or head that starts with anything else is not a GGUF, and a file
/// that is not there cannot be read, so either is an error, as the real
/// parser's would be. A head is read as a whole header. No tensor table is
/// read.
#[derive(Debug, Clone, Copy, Default)]
pub struct FirstBytesParser;

impl GgufParserPort for FirstBytesParser {
    fn parse(&self, file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        let bytes = std::fs::read(file_path).map_err(|e| GgufParseError::Io(e.to_string()))?;
        let role = role_of(&bytes)?;
        Ok(GgufMetadata {
            role,
            ..Default::default()
        })
    }

    fn detect_capabilities(&self, _metadata: &GgufMetadata) -> GgufCapabilities {
        GgufCapabilities::empty()
    }

    fn tensor_table(&self, _path: &Path) -> Result<TensorTable, GgufParseError> {
        Err(GgufParseError::InvalidFormat("no tensor table".to_owned()))
    }

    fn tensor_table_of_head(&self, _head: &[u8]) -> Result<TensorTable, GgufParseError> {
        Err(GgufParseError::InvalidFormat("no tensor table".to_owned()))
    }

    fn parse_head(&self, head: &[u8], _file_name: &str) -> Result<HeadMetadata, GgufParseError> {
        Ok(HeadMetadata {
            complete: true,
            role: Some(role_of(head)?),
            ..HeadMetadata::default()
        })
    }
}

/// The role `bytes` start with, or the error a file that is no GGUF gives.
fn role_of(bytes: &[u8]) -> Result<GgufFileRole, GgufParseError> {
    if bytes.starts_with(b"projector") {
        Ok(GgufFileRole::Projector)
    } else if bytes.starts_with(b"weights") {
        Ok(GgufFileRole::Weights)
    } else {
        Err(GgufParseError::InvalidFormat("no GGUF magic".to_owned()))
    }
}
