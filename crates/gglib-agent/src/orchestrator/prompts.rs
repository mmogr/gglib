//! Prompt templates and JSON Schema for the orchestrator director.
//!
//! The director prompt instructs the model to decompose a high-level goal into
//! a flat list of [`TaskNode`]-shaped objects.  A simpler intermediate format
//! (array of nodes) is used instead of a `HashMap` so the schema and few-shot
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
//!
//! The Chief of Staff prompt instructs the model to decompose a goal into
//! 1–5 departments, each with a mission statement and suggested specialist roles.

// =============================================================================
// Director system prompt
// =============================================================================

/// System prompt injected for the director planning pass.
///
/// Placeholders: `{tool_catalog}`, `{role_catalog}`.
///
/// `{tool_catalog}` is rendered as `"- name: description"` lines so the model
/// knows which tool names it may use in `tool_allowlist` entries.
/// `{role_catalog}` is rendered as `"- id: display_name — fragment"` lines
/// summarising the available specialist roles; inject `"(none)"` when absent.
pub const DIRECTOR_SYSTEM_PROMPT: &str = "\
You are the Director Agent. Given a high-level goal (and optionally a department \
mission), decompose it into a directed acyclic task graph (DAG) of worker nodes \
that together achieve the goal.

TOOL CATALOG (only these tool names may appear in tool_allowlist):
{tool_catalog}

SPECIALIST ROLES (optionally assign one role per node using the role field):
{role_catalog}

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

// =============================================================================
// Worker compaction prompt
// =============================================================================

/// System prompt for the post-worker compaction pass.
///
/// Placeholders: `{goal}`, `{output}`.
///
/// The compaction agent receives the full worker output and produces a
/// compact summary (≤ 250 words) preserving facts, results, and conclusions
/// that downstream nodes may need as context.
pub const WORKER_COMPACTION_PROMPT: &str = "\
You are a precise summarizer. A worker agent was given the following goal:

GOAL: {goal}

The worker produced the following output:

---
{output}
---

Produce a concise summary (at most 250 words) that:
- Preserves all key facts, results, conclusions, and any actionable data.
- Is written in third-person past tense (e.g. \"The agent found...\").
- Omits conversational filler, tool call traces, and internal reasoning.
- Retains specific values (numbers, names, paths, URLs) that downstream agents need.

Output ONLY the summary text. No preamble, no title, no markdown fences.";

// =============================================================================
// Orchestrator synthesis prompt
// =============================================================================

/// System prompt for the final orchestrator synthesis pass.
///
/// Placeholders: `{goal}`, `{results}`.
///
/// The synthesiser receives compacted outputs from every leaf node and
/// produces a single unified answer addressing the original goal.
pub const ORCHESTRATOR_SYNTHESIS_PROMPT: &str = "\
You are synthesizing the output of a multi-agent task graph.

ORIGINAL GOAL: {goal}

The following are the results from the completed worker agents:

{results}

Produce a clear, direct, and complete answer to the original goal that integrates \
all agent outputs. Be concise and actionable. Do not repeat the goal or introduce \
each section with agent names — write a unified response as if you produced it yourself.";

// =============================================================================
// Chief of Staff system prompt
// =============================================================================

/// System prompt for the Chief of Staff structured-output call.
///
/// Placeholders: `{role_catalog}`.
///
/// The Chief of Staff receives the user goal and returns 1–5 `DepartmentBrief`
/// objects (name, mission, `suggested_roles`).  The planner fans out one
/// [`super::director::plan`] call per department in parallel.
///
/// # Expected output shape
///
/// ```json
/// {
///   "departments": [
///     {
///       "name": "research",
///       "mission": "Gather all factual evidence about llama.cpp's history.",
///       "suggested_roles": ["researcher", "fact-checker"]
///     },
///     {
///       "name": "writing",
///       "mission": "Produce and polish the final written summary.",
///       "suggested_roles": ["writer", "editor"]
///     }
///   ]
/// }
/// ```
pub const CHIEF_OF_STAFF_SYSTEM_PROMPT: &str = "\
You are the Chief of Staff. Your job is to decompose a complex goal into 1–5 \
focused departments. Each department will be handed to a specialist Director \
who will further decompose it into individual worker tasks.

AVAILABLE SPECIALIST ROLES:
{role_catalog}

OUTPUT FORMAT:
Respond with ONLY the JSON object below — no explanation, no markdown fences, \
no surrounding text:
{ \"departments\": [...] }

Each department must have:
- name: short, unique kebab-case identifier (e.g. \"research\", \"risk-review\")
- mission: one or two sentences describing what this department must accomplish
- suggested_roles: array of role ids from AVAILABLE SPECIALIST ROLES (may be empty [])

CONSTRAINTS:
- 1 to 5 departments maximum
- Department names must be unique (after trimming and lower-casing)
- Each department mission must be distinct from the others
- Do not create a department whose entire scope is covered by another department
- suggested_roles entries must be ids from AVAILABLE SPECIALIST ROLES (or empty [])

DECOMPOSITION RULES:
- Decompose by area of expertise, not by execution order
- Departments run in parallel; do not create explicit dependencies between them
- A single-discipline goal (e.g. \"summarise this article\") should produce exactly
  one department — do not over-engineer

EXAMPLES:

# Example 1 — Single department (simple goal)
Goal: \"Summarise this article about climate change\"
Response:
{
  \"departments\": [
    {
      \"name\": \"summarisation\",
      \"mission\": \"Read the article and produce a concise, accurate summary of its key claims and evidence.\",
      \"suggested_roles\": [\"researcher\", \"writer\"]
    }
  ]
}

# Example 2 — Multi-department (launch plan)
Goal: \"Write a product launch plan with marketing, engineering, and risk review\"
Response:
{
  \"departments\": [
    {
      \"name\": \"marketing\",
      \"mission\": \"Develop the go-to-market strategy, messaging, and channel plan for the product launch.\",
      \"suggested_roles\": [\"writer\", \"critic\"]
    },
    {
      \"name\": \"engineering\",
      \"mission\": \"Define the technical readiness checklist, release sequencing, and rollback plan.\",
      \"suggested_roles\": [\"researcher\", \"fact-checker\"]
    },
    {
      \"name\": \"risk-review\",
      \"mission\": \"Identify launch risks across marketing, engineering, and operations, and propose mitigations.\",
      \"suggested_roles\": [\"red-team\", \"critic\"]
    }
  ]
}";

// =============================================================================
// JSON Schema for ChiefOfStaffPlan
// =============================================================================

/// Build the JSON Schema for [`super::chief_of_staff::ChiefOfStaffPlan`].
///
/// Passed to [`crate::structured_output::get_structured`] as the
/// `ResponseFormat::JsonSchema` constraint.
///
/// # Schema shape
///
/// ```json
/// {
///   "type": "object",
///   "properties": {
///     "departments": {
///       "type": "array",
///       "items": {
///         "type": "object",
///         "properties": {
///           "name": { "type": "string" },
///           "mission": { "type": "string" },
///           "suggested_roles": { "type": "array", "items": { "type": "string" } }
///         },
///         "required": ["name", "mission", "suggested_roles"]
///       }
///     }
///   },
///   "required": ["departments"]
/// }
/// ```
pub fn chief_of_staff_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "departments": {
                "type": "array",
                "description": "List of 1–5 departments the goal is decomposed into.",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Short, unique kebab-case department identifier."
                        },
                        "mission": {
                            "type": "string",
                            "description": "One or two sentences describing the department's scope."
                        },
                        "suggested_roles": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Specialist role ids suggested for this department's nodes."
                        }
                    },
                    "required": ["name", "mission", "suggested_roles"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["departments"],
        "additionalProperties": false
    })
}

// =============================================================================
// Steering prompt + schema (Phase K)
// =============================================================================

/// System prompt for the steering LLM call.
///
/// The placeholder `<GRAPH_JSON>` is replaced with the current task graph
/// serialised as pretty-printed JSON before the call is made.
pub const STEERING_SYSTEM_PROMPT: &str = "\
You are a task-graph planner assistant.
The user will describe one change to make to the current execution plan.
Respond with exactly one JSON object that conforms to the GraphDiff schema.

Current task graph (JSON):
<GRAPH_JSON>

Available operations (use exactly one):
  add_node      — add a new node.
                  Fields: op, node (object with id, goal, depends_on, tool_allowlist, kind, status)
  remove_node   — remove an existing node and its edges.
                  Fields: op, id
  split_node    — replace one node with multiple nodes.
                  Fields: op, id, into (array of node objects)
  reroute_edge  — change one dependency edge.
                  Fields: op, node_id, old_dep, new_dep
  set_role      — set or clear a node's specialist role.
                  Fields: op, id, role (string or null)
  set_tools     — replace a node's tool allowlist.
                  Fields: op, id, tool_allowlist (array of strings)
  wrap_in_team  — wrap several nodes into a new Team node.
                  Fields: op, ids (array), team_id, team_goal

Rules:
- All node ids must be short, unique, lowercase identifiers (e.g. \"research\", \"review\").
- The op field must be exactly one of the names listed above.
- Return only the JSON object, no prose.
";

/// JSON Schema for a single [`gglib_core::domain::orchestrator::task_graph::GraphDiff`].
///
/// Deliberately flat (no `oneOf` / discriminated union) so that small models
/// can satisfy the constraint reliably.  The `op` field acts as a discriminant.
pub fn graph_diff_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "op": {
                "type": "string",
                "enum": [
                    "add_node", "remove_node", "split_node",
                    "reroute_edge", "set_role", "set_tools", "wrap_in_team"
                ]
            },
            "node":           { "type": "object" },
            "id":             { "type": "string" },
            "into":           { "type": "array", "items": { "type": "object" } },
            "node_id":        { "type": "string" },
            "old_dep":        { "type": "string" },
            "new_dep":        { "type": "string" },
            "role":           {},
            "tool_allowlist": { "type": "array", "items": { "type": "string" } },
            "ids":            { "type": "array", "items": { "type": "string" } },
            "team_id":        { "type": "string" },
            "team_goal":      { "type": "string" }
        },
        "required": ["op"]
    })
}
