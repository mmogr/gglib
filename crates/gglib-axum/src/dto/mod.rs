#![doc = include_str!("README.md")]

// MIGRATION: content extracted to README.md — remove this //! block after review
//! Data Transfer Objects (DTOs) for HTTP API contract.
//!
//! These types define the stable HTTP API contract with explicit serialization
//! control. They decouple internal domain types from external API representation.

pub mod system;

pub use system::SystemMemoryInfoDto;
