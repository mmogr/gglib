//! In-process built-in tool executor.
//!
//! Implements [`ToolExecutorPort`] for tools that run directly inside the
//! server process rather than through an external MCP child process.
//!
//! # Tool-name format
//!
//! Names are qualified with `"builtin:"` (e.g. `"builtin:get_current_time"`),
//! matching the convention used by [`McpToolExecutorAdapter`] where names are
//! qualified with the numeric server id (e.g. `"3:read_file"`).
//! [`CombinedToolExecutor`] routes calls with the `"builtin:"` prefix here.

mod fs_grep;
mod fs_list;
mod fs_read;
pub(crate) mod sandboxing;
mod time;

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::anyhow;
use async_trait::async_trait;
use gglib_core::ports::ToolExecutorPort;
use gglib_core::{McpTool, ToolCall, ToolDefinition, ToolResult};
use serde_json::{Value, json};

/// Prefix applied to all tool names produced by this executor.
pub const BUILTIN_PREFIX: &str = "builtin:";

// =============================================================================
// Adapter
// =============================================================================

/// Executor for built-in tools.
///
/// When `sandbox_root` is set, filesystem tools (`read_file`, `list_directory`,
/// `grep_search`) are available and confined to that directory. When `None`,
/// only non-filesystem tools (`get_current_time`) are exposed.
#[derive(Debug, Default, Clone)]
pub struct BuiltinToolExecutorAdapter {
    sandbox_root: Option<PathBuf>,
}

impl BuiltinToolExecutorAdapter {
    /// Create an adapter with filesystem tools sandboxed to `root`.
    pub const fn with_sandbox(root: PathBuf) -> Self {
        Self {
            sandbox_root: Some(root),
        }
    }

    /// Bare (unprefixed) tool definitions for the HTTP discovery endpoint.
    ///
    /// These use the exact same schema as [`ToolExecutorPort::list_tools`] but
    /// without the `"builtin:"` prefix so the frontend can register them as
    /// `originalName = bare_name, serverId = "builtin"`.
    pub fn bare_definitions() -> Vec<McpTool> {
        Self::all_definitions()
    }

    /// All tool definitions including filesystem tools.
    fn all_definitions() -> Vec<McpTool> {
        let mut defs = vec![
            McpTool::new("get_current_time")
                .with_description(
                    "Get the current date and time. Can return time in different \
                     timezones and formats. Useful for time-sensitive queries or scheduling.",
                )
                .with_input_schema(json!({
                    "type": "object",
                    "properties": {
                        "timezone": {
                            "type": "string",
                            "description": "IANA timezone name (e.g. \"America/New_York\", \
                                            \"Europe/London\"). Defaults to UTC."
                        },
                        "format": {
                            "type": "string",
                            "description": "Output format: \"iso\" for ISO 8601, \
                                            \"human\" for human-readable, \
                                            \"unix\" for Unix timestamp.",
                            "enum": ["iso", "human", "unix"],
                            "default": "human"
                        }
                    },
                    "required": []
                })),
        ];

        defs.extend(Self::fs_definitions());

        defs
    }

    /// Tool definitions available only when a sandbox root is set.
    fn fs_definitions() -> Vec<McpTool> {
        vec![
            McpTool::new("read_file")
                .with_description(
                    "Read the contents of a text file. Binary files are rejected. \
                     Very large files are truncated. Path is relative to the \
                     working directory.",
                )
                .with_input_schema(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file (relative to working directory)"
                        }
                    },
                    "required": ["path"]
                })),
            McpTool::new("list_directory")
                .with_description(
                    "List entries in a directory. Directories end with '/'. \
                     Hidden files are excluded by default. Path is relative \
                     to the working directory.",
                )
                .with_input_schema(json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to list (default: current directory)",
                            "default": "."
                        },
                        "include_hidden": {
                            "type": "boolean",
                            "description": "Include hidden files (starting with '.')",
                            "default": false
                        }
                    },
                    "required": []
                })),
            McpTool::new("grep_search")
                .with_description(
                    "Search for a text pattern in files. Case-insensitive substring \
                     search. Skips binary files and common noise directories \
                     (node_modules, .git, target). Returns matching lines with \
                     file paths and line numbers.",
                )
                .with_input_schema(json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Text pattern to search for (case-insensitive)"
                        },
                        "path": {
                            "type": "string",
                            "description": "Directory or file to search (default: current directory)",
                            "default": "."
                        }
                    },
                    "required": ["pattern"]
                })),
        ]
    }
}

// =============================================================================
// ToolExecutorPort
// =============================================================================

#[async_trait]
impl ToolExecutorPort for BuiltinToolExecutorAdapter {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        let defs = if self.sandbox_root.is_some() {
            Self::all_definitions()
        } else {
            // Without sandbox, only expose non-filesystem tools
            vec![Self::all_definitions().into_iter().next().unwrap()]
        };

        defs.into_iter()
            .map(|t| ToolDefinition {
                name: format!("{BUILTIN_PREFIX}{}", t.name),
                description: t.description,
                input_schema: t.input_schema,
            })
            .collect()
    }

    async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
        let bare = call.name.strip_prefix(BUILTIN_PREFIX).ok_or_else(|| {
            anyhow!(
                "builtin executor received name without prefix: '{}'",
                call.name
            )
        })?;

        let args = parse_args(call)?;

        match bare {
            "get_current_time" => {
                let content = time::get_current_time(&args);
                Ok(ToolResult {
                    tool_call_id: call.id.clone(),
                    content: content.to_string(),
                    success: true,
                })
            }
            "read_file" | "list_directory" | "grep_search" => {
                let root = self
                    .sandbox_root
                    .as_ref()
                    .ok_or_else(|| anyhow!("filesystem tools require a sandbox root"))?;
                let result = match bare {
                    "read_file" => fs_read::read_file(&args, root),
                    "list_directory" => fs_list::list_directory(&args, root),
                    "grep_search" => fs_grep::grep_search(&args, root),
                    _ => unreachable!(),
                };
                match result {
                    Ok(content) => Ok(ToolResult {
                        tool_call_id: call.id.clone(),
                        content,
                        success: true,
                    }),
                    Err(msg) => Ok(ToolResult {
                        tool_call_id: call.id.clone(),
                        content: msg,
                        success: false,
                    }),
                }
            }
            _ => Err(anyhow!("unknown builtin tool '{bare}'")),
        }
    }
}

// =============================================================================
// Shared helpers
// =============================================================================

/// Parse a [`ToolCall`]'s arguments into a `HashMap`.
///
/// Accepts a JSON object or null (empty map).  Returns an error for any
/// other JSON type.
pub(crate) fn parse_args(call: &ToolCall) -> anyhow::Result<HashMap<String, Value>> {
    match &call.arguments {
        Value::Object(map) => Ok(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
        Value::Null => Ok(HashMap::new()),
        other => Err(anyhow!(
            "tool '{}' arguments must be a JSON object; got {}",
            call.name,
            other
        )),
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::ToolCall;

    fn make_call(name: &str, args: Value) -> ToolCall {
        ToolCall {
            id: "test-id".to_owned(),
            name: name.to_owned(),
            arguments: args,
        }
    }

    #[tokio::test]
    async fn list_tools_returns_prefixed_names() {
        let adapter = BuiltinToolExecutorAdapter::default();
        let tools = adapter.list_tools().await;
        assert!(!tools.is_empty());
        assert!(tools.iter().all(|t| t.name.starts_with(BUILTIN_PREFIX)));
    }

    #[tokio::test]
    async fn bare_definitions_have_no_prefix() {
        for t in BuiltinToolExecutorAdapter::bare_definitions() {
            assert!(!t.name.starts_with(BUILTIN_PREFIX));
        }
    }

    #[tokio::test]
    async fn get_current_time_human_format_returns_time_object() {
        let adapter = BuiltinToolExecutorAdapter::default();
        let result = adapter
            .execute(&make_call("builtin:get_current_time", json!({})))
            .await
            .unwrap();
        assert!(result.success);
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert!(parsed.get("time").is_some());
        assert!(parsed.get("timezone").is_some());
        assert!(parsed.get("format").is_some());
    }

    #[tokio::test]
    async fn get_current_time_iso_format() {
        let adapter = BuiltinToolExecutorAdapter::default();
        let result = adapter
            .execute(&make_call(
                "builtin:get_current_time",
                json!({ "format": "iso" }),
            ))
            .await
            .unwrap();
        assert!(result.success);
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["format"], "iso");
    }

    #[tokio::test]
    async fn get_current_time_unix_format_is_number() {
        let adapter = BuiltinToolExecutorAdapter::default();
        let result = adapter
            .execute(&make_call(
                "builtin:get_current_time",
                json!({ "format": "unix" }),
            ))
            .await
            .unwrap();
        assert!(result.success);
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert!(parsed["time"].is_number());
    }

    #[tokio::test]
    async fn unknown_tool_returns_error() {
        let adapter = BuiltinToolExecutorAdapter::default();
        let err = adapter
            .execute(&make_call("builtin:nonexistent", json!({})))
            .await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn missing_prefix_returns_error() {
        let adapter = BuiltinToolExecutorAdapter::default();
        let err = adapter
            .execute(&make_call("get_current_time", json!({})))
            .await;
        assert!(err.is_err());
    }
}
