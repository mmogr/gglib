//! Chief of Staff: decompose a goal into 1–5 department briefs.
//!
//! The [`brief`] function makes a single structured-output call to the LLM
//! and returns a validated `Vec<DepartmentBrief>` of up to five departments.
//!
//! # Validation
//!
//! After the LLM call the result is self-validated:
//!
//! - Trimmed to at most [`MAX_DEPARTMENTS`] entries (log-warn if truncated).
//! - Department names are lower-cased and deduplicated (first occurrence wins).
//!
//! No retry loop is needed here — the planner falls back gracefully to a
//! single-department plan when `brief` returns an error (see [`super::planner`]).
//!
//! # Example (doc-test — no LLM required)
//!
//! ```rust
//! use gglib_agent::orchestrator::chief_of_staff::DepartmentBrief;
//! use gglib_core::domain::orchestrator::role_catalog::RoleId;
//!
//! let brief = DepartmentBrief {
//!     name: "research".into(),
//!     mission: "Gather facts about llama.cpp.".into(),
//!     suggested_roles: vec![RoleId::new("researcher")],
//! };
//! assert_eq!(brief.name, "research");
//! assert_eq!(brief.suggested_roles.len(), 1);
//! ```

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::orchestrator::role_catalog::{RoleCatalog, RoleId};
use gglib_core::ports::{LlmCompletionPort, StructuredOutputError};

use crate::orchestrator::prompts::{CHIEF_OF_STAFF_SYSTEM_PROMPT, chief_of_staff_schema};
use crate::structured_output::get_structured;

// =============================================================================
// Constants
// =============================================================================

/// Maximum number of departments the Chief of Staff may return.
pub const MAX_DEPARTMENTS: usize = 5;

// =============================================================================
// DepartmentBrief
// =============================================================================

/// A single department brief returned by the Chief of Staff.
///
/// Passed to [`super::director::plan`] as the planning context for that
/// department's subgraph.
///
/// # Example
///
/// ```rust
/// use gglib_agent::orchestrator::chief_of_staff::DepartmentBrief;
/// use gglib_core::domain::orchestrator::role_catalog::RoleId;
///
/// let brief = DepartmentBrief {
///     name: "engineering".into(),
///     mission: "Define technical readiness and rollback plan.".into(),
///     suggested_roles: vec![RoleId::new("researcher"), RoleId::new("fact-checker")],
/// };
/// assert_eq!(brief.name, "engineering");
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepartmentBrief {
    /// Short kebab-case department identifier (lower-cased, deduplicated by planner).
    pub name: String,
    /// One-to-two-sentence mission statement passed to the department's Director.
    pub mission: String,
    /// Suggested specialist roles for this department's leaf nodes.
    pub suggested_roles: Vec<RoleId>,
}

// Internal wire type for JSON deserialization.
#[derive(Debug, Deserialize, Serialize)]
struct WireDepartment {
    name: String,
    mission: String,
    suggested_roles: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct WirePlan {
    departments: Vec<WireDepartment>,
}

// =============================================================================
// BriefError
// =============================================================================

/// Error returned by [`brief`].
#[derive(Debug, Error)]
pub enum BriefError {
    /// The structured-output call to the LLM failed.
    #[error("structured output error: {0}")]
    StructuredOutput(#[from] StructuredOutputError),
}

// =============================================================================
// Public API
// =============================================================================

/// Ask the Chief of Staff LLM to decompose `goal` into 1–5 department briefs.
///
/// Only role ids that exist in `catalog` are preserved in `suggested_roles`;
/// unrecognised ids are silently dropped.
///
/// Returns a non-empty [`Vec<DepartmentBrief>`] on success.  The vec is
/// guaranteed to have at most [`MAX_DEPARTMENTS`] entries, with unique
/// (lower-cased) names.
///
/// # Parameters
///
/// - `goal` — High-level goal to decompose into departments.
/// - `catalog` — Built-in role catalog used for validation of `suggested_roles`.
/// - `llm` — LLM completion port.
///
/// # Errors
///
/// Returns [`BriefError`] only if the structured-output call itself fails
/// (e.g. parse error or transport failure).  Validation issues in the LLM
/// output are silently corrected (truncation, dedup, unknown-role filtering).
///
/// # Example (doc-test — validates output contract, no LLM)
///
/// ```rust
/// use gglib_agent::orchestrator::chief_of_staff::{DepartmentBrief, MAX_DEPARTMENTS};
/// use gglib_core::domain::orchestrator::role_catalog::{RoleCatalog, RoleId};
///
/// // Simulate post-validation output the function would return.
/// let mut briefs = vec![
///     DepartmentBrief {
///         name: "research".into(),
///         mission: "Gather evidence.".into(),
///         suggested_roles: vec![RoleId::new("researcher")],
///     },
///     DepartmentBrief {
///         name: "writing".into(),
///         mission: "Produce the draft.".into(),
///         suggested_roles: vec![RoleId::new("writer"), RoleId::new("editor")],
///     },
/// ];
/// // Guarantee ≤ MAX_DEPARTMENTS (as done in `brief`).
/// briefs.truncate(MAX_DEPARTMENTS);
/// assert!(briefs.len() <= MAX_DEPARTMENTS);
/// assert_eq!(briefs[0].name, "research");
/// ```
pub async fn brief(
    goal: &str,
    catalog: &RoleCatalog,
    llm: Arc<dyn LlmCompletionPort>,
) -> Result<Vec<DepartmentBrief>, BriefError> {
    let role_catalog_text = render_role_catalog(catalog);
    let system = CHIEF_OF_STAFF_SYSTEM_PROMPT.replace("{role_catalog}", &role_catalog_text);

    let messages: Vec<AgentMessage> = vec![
        AgentMessage::System { content: system },
        AgentMessage::User {
            content: goal.to_owned(),
        },
    ];

    let schema = chief_of_staff_schema();

    let wire: WirePlan = get_structured(&llm, messages, schema, 1).await?;

    Ok(validate_briefs(wire.departments, catalog))
}

// =============================================================================
// Helpers
// =============================================================================

/// Validate and normalise raw wire departments into [`DepartmentBrief`]s.
///
/// - Trims to [`MAX_DEPARTMENTS`].
/// - Lower-cases and deduplicates names (first occurrence wins).
/// - Filters `suggested_roles` to only ids present in `catalog`.
fn validate_briefs(raw: Vec<WireDepartment>, catalog: &RoleCatalog) -> Vec<DepartmentBrief> {
    if raw.len() > MAX_DEPARTMENTS {
        tracing::warn!(
            count = raw.len(),
            max = MAX_DEPARTMENTS,
            "chief-of-staff: truncating oversized department list"
        );
    }

    let mut seen_names = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(raw.len().min(MAX_DEPARTMENTS));

    for dept in raw.into_iter().take(MAX_DEPARTMENTS) {
        let name = dept.name.trim().to_lowercase();
        if name.is_empty() {
            continue;
        }
        if !seen_names.insert(name.clone()) {
            tracing::debug!(name, "chief-of-staff: dropping duplicate department name");
            continue;
        }

        // Filter suggested_roles to catalog entries only.
        let suggested_roles: Vec<RoleId> = dept
            .suggested_roles
            .into_iter()
            .map(|s| RoleId::new(s.trim().to_lowercase()))
            .filter(|id| catalog.get(id).is_some())
            .collect();

        out.push(DepartmentBrief {
            name,
            mission: dept.mission,
            suggested_roles,
        });
    }

    out
}

/// Render the role catalog as `"- id: DisplayName — prompt_fragment_first_sentence"` lines.
pub fn render_role_catalog(catalog: &RoleCatalog) -> String {
    let mut lines: Vec<String> = catalog
        .iter()
        .map(|(id, spec)| {
            let first_sentence = spec
                .system_prompt_fragment
                .split(". ")
                .next()
                .unwrap_or(spec.system_prompt_fragment);
            format!(
                "- {}: {} — {}",
                id.as_str(),
                spec.display_name,
                first_sentence
            )
        })
        .collect();
    // Sort for determinism in tests.
    lines.sort();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_briefs_truncates_to_max() {
        let catalog = RoleCatalog::default();
        let raw: Vec<WireDepartment> = (0..8)
            .map(|i| WireDepartment {
                name: format!("dept-{i}"),
                mission: "some mission".into(),
                suggested_roles: vec![],
            })
            .collect();
        let out = validate_briefs(raw, &catalog);
        assert_eq!(out.len(), MAX_DEPARTMENTS);
    }

    #[test]
    fn validate_briefs_deduplicates_names() {
        let catalog = RoleCatalog::default();
        let raw = vec![
            WireDepartment {
                name: "research".into(),
                mission: "first".into(),
                suggested_roles: vec![],
            },
            WireDepartment {
                name: "RESEARCH".into(),
                mission: "duplicate".into(),
                suggested_roles: vec![],
            },
            WireDepartment {
                name: "writing".into(),
                mission: "second".into(),
                suggested_roles: vec![],
            },
        ];
        let out = validate_briefs(raw, &catalog);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].name, "research");
        assert_eq!(out[0].mission, "first");
    }

    #[test]
    fn validate_briefs_filters_unknown_roles() {
        let catalog = RoleCatalog::default();
        let raw = vec![WireDepartment {
            name: "analysis".into(),
            mission: "analyse stuff".into(),
            suggested_roles: vec!["researcher".into(), "made-up-role".into()],
        }];
        let out = validate_briefs(raw, &catalog);
        assert_eq!(out[0].suggested_roles.len(), 1);
        assert_eq!(out[0].suggested_roles[0].as_str(), "researcher");
    }

    #[test]
    fn validate_briefs_keeps_all_known_roles() {
        let catalog = RoleCatalog::default();
        let raw = vec![WireDepartment {
            name: "multi".into(),
            mission: "many roles".into(),
            suggested_roles: vec!["researcher".into(), "writer".into(), "editor".into()],
        }];
        let out = validate_briefs(raw, &catalog);
        assert_eq!(out[0].suggested_roles.len(), 3);
    }

    #[test]
    fn validate_briefs_drops_empty_names() {
        let catalog = RoleCatalog::default();
        let raw = vec![
            WireDepartment {
                name: "  ".into(),
                mission: "ghost dept".into(),
                suggested_roles: vec![],
            },
            WireDepartment {
                name: "real".into(),
                mission: "real dept".into(),
                suggested_roles: vec![],
            },
        ];
        let out = validate_briefs(raw, &catalog);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "real");
    }

    #[test]
    fn render_role_catalog_is_deterministic() {
        let catalog = RoleCatalog::default();
        let a = render_role_catalog(&catalog);
        let b = render_role_catalog(&catalog);
        assert_eq!(a, b);
        assert!(a.contains("researcher"));
        assert!(a.contains("synthesizer"));
    }
}
