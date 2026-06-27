#![doc = include_str!("README.md")]

// MIGRATION: content extracted to README.md — remove this //! block after review
//! Submodule index for concrete [`super::parser::ToolCallParser`] implementations.
//!
//! Each parser lives in its own file and is named after the dialect it
//! handles.  This is the only place — together with
//! [`super::registry`] — where the set of available parsers is enumerated.

pub mod qwen_xml;
pub mod standard;
