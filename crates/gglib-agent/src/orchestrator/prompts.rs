//! Prompt templates and JSON Schema for the orchestrator director.
//!
//! The director prompt instructs the model to decompose a high-level goal into
//! a flat list of [`TaskNode`]-shaped objects.  A simpler intermediate format
//! (array of nodes) is used instead of a HashMap so the schema and few-shot
//! examples stay unambiguous for small models.
//!
//! # Format contract
//!
//! The LLM must emit exactly:
//!
//! ```json
//! {
//!   "goal": "...",
//!   "nodes": [
//!     { "id": "...", "goal": "...", "depends_on": [...], "tool_allowlist": [...] },
//!     ...
//!   ]
//! }
//! ```
//!
//! The director function ([`super::director::plan`]) validates and converts
//! this into a fully-checked [`gglib_core::domain::orchestrator::task_graph::TaskGraph`].

// =============================================================================
// Director system prompt
// =============================================================================

/// System prompt injected for the director planning pass.
///
/// Placeholders: `{tool_catalog}`.
///
/// The tool catalog is rendered as `"- name: description"` lines so the model
/// knows which tool names it may use in `tool_allowlist` entries.
pub const DIRECTOR_SYSTEM_PROMPT: &str = "\
You are the Director Agent. Given a high-level goal, decompose it into a \
directed acyclic task graph (DAG) of worker nodes that together achieve the goal.

TOOL CATALOG (only these tool names may appear in tool_allowlist):
{tool_catalog}

OUTPUT FORMAT:
Respond with ONLY the JSON object below — no explanation, no markdown fences, \
no surrounding text:
{ \"goal\": \"...\", \"nodes\": [...] }

Each node must have:
- id: short, unique kebab-case identifier (e.g. \"research-llama\", \"write-draft\")
- goal: one sentence the worker agent should achieve
- depends_on: array of node ids whose output this node needs (empty [] for root nodes)
- tool_allowlist: array of tool names from the TOOL CATALOG the worker may use

CONSTRAINTS:
- At most 8 nodes total
- Dependency depth ≤ 3 (longest chain of depends_on links)
- Each node may depend on at most 4 other nodes
- All ids in depends_on must be ids of other nodes in the same list
- tool_allowlist entries must be names from the TOOL CATALOG above (or empty [])
- Node ids must be unique within the plan
- Do NOT include execution state fields (status, output, error)

DECOMPOSITION RULES:
- Prefer parallel subtasks at the same depth when they are independent
- Each node goal must be specific and achievable in a single agent turn
- Root nodes (depends_on: []) run first, concurrently; later nodes consume their outputs
- Synthesis or review nodes should depend on the nodes they integrate

EXAMPLES:

# Example 1 — Research and write
Goal: \"Research the history of llama.cpp and write a summary\"
Response:
{
  \"goal\": \"Research the history of llama.cpp and write a summary\",
  \"nodes\": [
    {
      \"id\": \"research\",
      \"goal\": \"Research the history and development of llama.cpp, covering its origins, key milestones, and community growth.\",
      \"depends_on\": [],
      \"tool_allowlist\": [\"web_search\"]
    },
    {
      \"id\": \"write-summary\",
      \"goal\": \"Write a clear, comprehensive summary of llama.cpp's history based on the research findings.\",
      \"depends_on\": [\"research\"],
      \"tool_allowlist\": []
    }
  ]
}

# Example 2 — Writing pipeline
Goal: \"Write a blog post about the benefits of open-source AI models\"
Response:
{
  \"goal\": \"Write a blog post about the benefits of open-source AI models\",
  \"nodes\": [
    {
      \"id\": \"outline\",
      \"goal\": \"Create a detailed outline for a blog post about open-source AI model benefits, covering key arguments and supporting evidence.\",
      \"depends_on\": [],
      \"tool_allowlist\": []
    },
    {
      \"id\": \"draft\",
      \"goal\": \"Write a full blog post draft based on the outline, covering accessibility, innovation, transparency, and community benefits.\",
      \"depends_on\": [\"outline\"],
      \"tool_allowlist\": []
    },
    {
      \"id\": \"polish\",
      \"goal\": \"Review and polish the draft: fix grammar, improve flow, strengthen the introduction and conclusion.\",
      \"depends_on\": [\"draft\"],
      \"tool_allowlist\": []
    }
  ]
}

# Example 3 — Parallel security review
Goal: \"Review a Python file for security vulnerabilities and suggest fixes\"
Response:
{
  \"goal\": \"Review a Python file for security vulnerabilities and suggest fixes\",
  \"nodes\": [
    {
      \"id\": \"scan-input\",
      \"goal\": \"Analyse input validation in the codebase: check for SQL injection, command injection, and untrusted data handling.\",
      \"depends_on\": [],
      \"tool_allowlist\": [\"read_file\", \"grep_search\"]
    },
    {
      \"id\": \"scan-auth\",
      \"goal\": \"Review authentication and authorisation patterns: check for hardcoded secrets, weak hashing, and privilege escalation risks.\",
      \"depends_on\": [],
      \"tool_allowlist\": [\"read_file\", \"grep_search\"]
    },
    {
      \"id\": \"report\",
      \"goal\": \"Combine the security scan results into a prioritised vulnerability report with specific fix recommendations.\",
      \"depends_on\": [\"scan-input\", \"scan-auth\"],
      \"tool_allowlist\": []
    }
  ]
}";

// =============================================================================
// JSON Schema for DirectorPlan
// =============================================================================

/// Build the JSON Schema for [`super::director::DirectorPlan`].
///
/// Returns a fresh [`serde_json::Value`] representing the schema.  Passed to
/// [`crate::structured_output::get_structured`] as the `ResponseFormat::JsonSchema`
/// constraint so the LLM backend can enforce the shape at the grammar level
/// when supported.
///
/// # Schema shape
///
/// ```json
/// {
///   "type": "object",
///   "properties": {
///     "goal": { "type": "string" },
///     "nodes": {
///       "type": "array",
///       "items": {
///         "type": "object",
///         "properties": { "id": ..., "goal": ..., "depends_on": ..., "tool_allowlist": ... },
///         "required": ["id", "goal", "depends_on", "tool_allowlist"]
///       }
///     }
///   },
///   "required": ["goal", "nodes"]
/// }
/// ```
pub fn director_plan_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "goal": {
                "type": "string",
                "description": "The high-level goal being decomposed."
            },
            "nodes": {
                "type": "array",
                "description": "Flat list of all task nodes in the plan.",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Short, unique kebab-case node identifier."
                        },
                        "goal": {
                            "type": "string",
                            "description": "One-sentence goal for the worker agent."
                        },
                        "depends_on": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Ids of prerequisite nodes (empty for root nodes)."
                        },
                        "tool_allowlist": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Tool names the worker may call (empty = no tools)."
                        }
                    },
                    "required": ["id", "goal", "depends_on", "tool_allowlist"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["goal", "nodes"],
        "additionalProperties": false
    })
}
