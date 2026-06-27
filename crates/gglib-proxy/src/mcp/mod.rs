#![doc = include_str!("README.md")]
// MIGRATION: content extracted to README.md — remove this //! block after review
//! MCP Streamable HTTP gateway for the proxy.
//!
//! Implements the MCP Streamable HTTP transport (spec 2025-03-26) so
//! external clients like OpenWebUI can discover and invoke gglib's
//! MCP tools through the same proxy that serves OpenAI-compatible
//! chat completions.
//!
//! # Module layout
//!
//! | Module       | Responsibility                                      |
//! |--------------|-----------------------------------------------------|
//! | `types`      | JSON-RPC 2.0 and MCP wire types (serde structs)     |
//! | `session`    | `Mcp-Session-Id` tracking and validation            |
//! | `meta_tools` | Progressive-disclosure index + 3 meta-tool specs    |
//! | `handlers`   | Axum route handlers for POST/GET/DELETE `/mcp`      |

pub mod handlers;
pub(crate) mod meta_tools;
pub mod session;
pub mod types;
