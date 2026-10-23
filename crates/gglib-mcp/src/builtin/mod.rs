#![doc = include_str!("README.md")]
mod fs_grep;
mod fs_list;
mod fs_read;
pub(crate) mod generate_image;
pub(crate) mod sandboxing;
mod time;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;
use gglib_core::domain::agent::{NoProgress, ToolProgressSink};
use gglib_core::ports::{IMAGE_JOB_DEADLINE, ToolExecutorPort};
use gglib_core::{McpTool, ToolCall, ToolDefinition, ToolResult};
use serde_json::{Value, json};

pub use generate_image::{
    DrawArm, DrawWith, DrawingTool, NO_WORDS as DRAW_NO_WORDS, NOT_ASKED as DRAW_NOT_ASKED,
    THEN_SAY as DRAW_THEN_SAY, THEN_SAY_FAILED as DRAW_THEN_SAY_FAILED,
    request_of as image_request_of,
};

/// Prefix applied to all tool names produced by this executor.
pub const BUILTIN_PREFIX: &str = "builtin:";

/// gglib's own file tools, unprefixed: the builtins offered only with a
/// sandbox root, each reading under it. The table names them from here.
pub const FILE_TOOLS: [&str; 3] = ["read_file", "list_directory", "grep_search"];

/// How the refusal of a file tool in a chat with no working folder ends,
/// after the tool's quoted name.
const NO_FOLDER: &str = "reads a working folder, and this chat has none";

// =============================================================================
// The table
// =============================================================================

/// One builtin, and what it needs before it is offered.
struct Builtin {
    /// Its name, unprefixed.
    name: &'static str,
    /// Offered only with a sandbox root: it reads files under it.
    needs_sandbox: bool,
    /// Offered only with a drawing tool, armed for this message.
    needs_drawing: bool,
    /// How it is described to the model, given the sandbox root a file tool
    /// names as its folder.
    definition: fn(Option<&Path>) -> McpTool,
    /// How long a call may take, when longer than a session's tool timeout.
    deadline: Option<Duration>,
}

/// Every builtin. Listing and calling both read this table, so a tool is
/// called only where it would be listed.
const BUILTINS: [Builtin; 5] = [
    Builtin {
        name: "get_current_time",
        needs_sandbox: false,
        needs_drawing: false,
        definition: time_definition,
        deadline: None,
    },
    Builtin {
        name: FILE_TOOLS[0],
        needs_sandbox: true,
        needs_drawing: false,
        definition: read_file_definition,
        deadline: None,
    },
    Builtin {
        name: FILE_TOOLS[1],
        needs_sandbox: true,
        needs_drawing: false,
        definition: list_directory_definition,
        deadline: None,
    },
    Builtin {
        name: FILE_TOOLS[2],
        needs_sandbox: true,
        needs_drawing: false,
        definition: grep_search_definition,
        deadline: None,
    },
    Builtin {
        name: generate_image::NAME,
        needs_sandbox: false,
        needs_drawing: true,
        definition: generate_image::definition,
        deadline: Some(IMAGE_JOB_DEADLINE),
    },
];

// =============================================================================
// Adapter
// =============================================================================

/// Executor for built-in tools.
///
/// When `sandbox_root` is set, filesystem tools (`read_file`, `list_directory`,
/// `grep_search`) are available and confined to that directory. With a
/// drawing tool armed for this message, `generate_image` is too. Otherwise
/// only `get_current_time` is exposed.
#[derive(Debug, Default, Clone)]
pub struct BuiltinToolExecutorAdapter {
    sandbox_root: Option<PathBuf>,
    drawing: Option<DrawingTool>,
    armed: DrawArm,
}

impl BuiltinToolExecutorAdapter {
    /// Create an adapter with filesystem tools sandboxed to `root`.
    pub fn with_sandbox(root: PathBuf) -> Self {
        Self {
            sandbox_root: Some(root),
            ..Self::default()
        }
    }

    /// Offer `generate_image` through `tool` whenever `armed` says so: a run
    /// sent with `draw: true`, or the message after `/draw`. `None` never
    /// offers it.
    #[must_use]
    pub fn with_drawing(mut self, drawing: Option<(DrawingTool, DrawArm)>) -> Self {
        (self.drawing, self.armed) = drawing.map_or_else(
            || (None, DrawArm::default()),
            |(tool, armed)| (Some(tool), armed),
        );
        self
    }

    /// Bare (unprefixed) tool definitions for the HTTP discovery endpoint.
    ///
    /// These use the exact same schema as [`ToolExecutorPort::list_tools`] but
    /// without the `"builtin:"` prefix so the frontend can register them as
    /// `originalName = bare_name, serverId = "builtin"`.
    ///
    /// Only the tools that need nothing from the run: a tool listed here can
    /// be switched on in the page's tool picker and sent with any message.
    /// Never `generate_image`, which is offered only to a message sent with
    /// Draw pressed, and never the file tools, which are offered only to a
    /// run that has a folder; the server decides both.
    pub fn bare_definitions() -> Vec<McpTool> {
        BUILTINS
            .iter()
            .filter(|tool| !tool.needs_drawing && !tool.needs_sandbox)
            .map(|tool| (tool.definition)(None))
            .collect()
    }

    /// Whether `tool` is offered right now.
    fn offers(&self, tool: &Builtin) -> bool {
        (!tool.needs_sandbox || self.sandbox_root.is_some())
            && (!tool.needs_drawing || (self.drawing.is_some() && self.armed.is_armed()))
    }
}

fn time_definition(_root: Option<&Path>) -> McpTool {
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
        }))
}

/// The sentence a file tool's description ends with: its folder, by the
/// absolute path it was given in backticks with any control character
/// escaped, and that a path is relative to it. A file tool is offered only
/// with a root, so the `None` form is never shown to a model; it exists
/// because every tool's definition shares one signature.
fn folder_sentence(root: Option<&Path>) -> String {
    let Some(root) = root else {
        return "A path is relative to the working folder.".to_owned();
    };
    let shown: String = root
        .display()
        .to_string()
        .chars()
        .map(|c| {
            if c.is_control() {
                c.escape_default().to_string()
            } else {
                c.to_string()
            }
        })
        .collect();
    format!("The working folder is `{shown}`; a path is relative to it.")
}

fn read_file_definition(root: Option<&Path>) -> McpTool {
    McpTool::new("read_file")
        .with_description(format!(
            "Read a text file. Binary files are refused and a very long file is cut short. {}",
            folder_sentence(root)
        ))
        .with_input_schema(json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path of the file, relative to the working folder"
                }
            },
            "required": ["path"]
        }))
}

fn list_directory_definition(root: Option<&Path>) -> McpTool {
    McpTool::new("list_directory")
        .with_description(format!(
            "List what is in a folder. A folder's name ends with '/'. Hidden entries are left \
             out unless include_hidden is true. {}",
            folder_sentence(root)
        ))
        .with_input_schema(json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Folder to list, relative to the working folder; \
                                    \".\" is the working folder itself",
                    "default": "."
                },
                "include_hidden": {
                    "type": "boolean",
                    "description": "Include hidden entries (names starting with '.')",
                    "default": false
                }
            },
            "required": []
        }))
}

fn grep_search_definition(root: Option<&Path>) -> McpTool {
    McpTool::new("grep_search")
        .with_description(format!(
            "Find lines that contain the given text, ignoring case. Skips binary files, \
             hidden entries and the folders {}. Each match is path:line:text. {}",
            fs_grep::skipped_folders(),
            folder_sentence(root)
        ))
        .with_input_schema(json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Text to find (case-insensitive)"
                },
                "path": {
                    "type": "string",
                    "description": "Folder or file to search, relative to the working folder; \
                                    \".\" is the working folder itself",
                    "default": "."
                }
            },
            "required": ["pattern"]
        }))
}

// =============================================================================
// ToolExecutorPort
// =============================================================================

#[async_trait]
impl ToolExecutorPort for BuiltinToolExecutorAdapter {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        BUILTINS
            .iter()
            .filter(|tool| self.offers(tool))
            .map(|tool| {
                let t = (tool.definition)(self.sandbox_root.as_deref());
                ToolDefinition {
                    name: format!("{BUILTIN_PREFIX}{}", t.name),
                    description: t.description,
                    input_schema: t.input_schema,
                    title: None,
                    deadline: tool.deadline,
                }
            })
            .collect()
    }

    async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
        self.execute_with_progress(call, &NoProgress).await
    }

    /// `get_current_time` answers at once. The file tools run on a blocking
    /// thread and report nothing; a call the tool timeout abandons stops its
    /// walk. `generate_image` reports each stage and step of its render
    /// through `sink`. A tool this adapter does not offer right now (no
    /// sandbox, or drawing not armed for this message) is refused, never
    /// run.
    async fn execute_with_progress(
        &self,
        call: &ToolCall,
        sink: &dyn ToolProgressSink,
    ) -> anyhow::Result<ToolResult> {
        let bare = call.name.strip_prefix(BUILTIN_PREFIX).ok_or_else(|| {
            anyhow!(
                "builtin executor received name without prefix: '{}'",
                call.name
            )
        })?;
        let tool = BUILTINS
            .iter()
            .find(|tool| tool.name == bare)
            .ok_or_else(|| anyhow!("unknown builtin tool '{bare}'"))?;
        if !self.offers(tool) {
            return Err(if tool.needs_drawing {
                anyhow!(
                    "'{bare}' is offered only for a message sent with Draw pressed, and this one \
                     was not"
                )
            } else {
                anyhow!("'{bare}' {NO_FOLDER}")
            });
        }

        let args = parse_args(call)?;

        match bare {
            "get_current_time" => {
                let content = time::get_current_time(&args);
                Ok(ToolResult::text(call.id.clone(), content.to_string(), true))
            }
            generate_image::NAME => match &self.drawing {
                Some(drawing) => Ok(drawing.draw(call, sink).await),
                None => Err(anyhow!("'{bare}' has no image driver")),
            },
            _ => {
                let root = self
                    .sandbox_root
                    .clone()
                    .ok_or_else(|| anyhow!("'{bare}' {NO_FOLDER}"))?;
                let name = tool.name;
                let stop = StopOnDrop::default();
                let flag = Arc::clone(&stop.0);
                let result = tokio::task::spawn_blocking(move || match name {
                    "read_file" => fs_read::read_file(&args, &root),
                    "list_directory" => fs_list::list_directory(&args, &root),
                    "grep_search" => fs_grep::grep_search(&args, &root, &flag),
                    _ => Err(format!("unknown builtin tool '{name}'")),
                })
                .await
                .map_err(|e| anyhow!("'{bare}' failed: {e}"))?;
                drop(stop);
                match result {
                    Ok(content) => Ok(ToolResult::text(call.id.clone(), content, true)),
                    Err(msg) => Ok(ToolResult::text(call.id.clone(), msg, false)),
                }
            }
        }
    }
}

// =============================================================================
// Shared helpers
// =============================================================================

/// Sets its flag when dropped. A file tool's call holds one while it waits
/// for the blocking thread, so a call the tool timeout abandons, whose future
/// is dropped mid-wait, tells the walk on that thread to stop.
#[derive(Default)]
struct StopOnDrop(Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// Parse a [`ToolCall`]'s arguments into a `HashMap`, for either executor.
///
/// Accepts a JSON object, or null for no arguments: LLMs often emit `null`
/// instead of `{}`.  Returns an error for any other JSON type.
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

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
