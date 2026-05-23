//! Built-in role catalog for the orchestrator v2 hierarchical planner.
//!
//! A [`RoleId`] identifies a specialist role that a [`TaskNode`] can adopt.
//! The [`RoleCatalog`] maps role names to [`RoleSpec`] entries that carry the
//! system-prompt fragment, default tool allowlist, suggested sampling
//! temperature, and approval policy for that role.
//!
//! # Built-in roles
//!
//! | Role id | Purpose |
//! |---------|---------|
//! | `researcher` | Information gathering and source retrieval |
//! | `red-team` | Adversarial challenge and stress-testing of plans |
//! | `fact-checker` | Verification of claims against retrieved evidence |
//! | `writer` | First-draft prose generation |
//! | `editor` | Revision and polish of existing drafts |
//! | `critic` | Structured critique and gap identification |
//! | `synthesizer` | Final integration of multiple node outputs |
//!
//! YAML-overridable catalogs are deferred to a future phase.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// =============================================================================
// RoleId
// =============================================================================

/// Opaque identifier for a specialist role within a task-force subgraph.
///
/// Short, kebab-case strings are recommended (e.g. `"researcher"`, `"red-team"`).
///
/// # Example
///
/// ```rust
/// use gglib_core::domain::orchestrator::role_catalog::RoleId;
///
/// let id = RoleId::new("researcher");
/// assert_eq!(id.as_str(), "researcher");
/// assert_eq!(id.to_string(), "researcher");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoleId(pub String);

impl RoleId {
    /// Create a new [`RoleId`] from any string-like value.
    ///
    /// # Example
    ///
    /// ```rust
    /// use gglib_core::domain::orchestrator::role_catalog::RoleId;
    ///
    /// let id = RoleId::new("synthesizer");
    /// assert_eq!(id.as_str(), "synthesizer");
    /// ```
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Return the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RoleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// =============================================================================
// RoleSpec
// =============================================================================

/// Specification for a single specialist role.
///
/// Carried inside [`RoleCatalog`] and resolved at planning time by the
/// hierarchical director (Phase H).
#[derive(Debug, Clone)]
pub struct RoleSpec {
    /// Short, human-readable display name (title-case).
    pub display_name: &'static str,
    /// One-paragraph system-prompt fragment injected before the worker's goal.
    pub system_prompt_fragment: &'static str,
    /// Tool names this role is permitted to call by default.
    ///
    /// The director may widen or narrow this list per-node at plan time.
    pub default_tool_allowlist: &'static [&'static str],
    /// Suggested sampling temperature for this role's LLM calls (0.0–1.0).
    pub suggested_temperature: f32,
    /// Whether this role's output requires human approval before being passed
    /// downstream.
    pub requires_approval: bool,
}

// =============================================================================
// RoleCatalog
// =============================================================================

/// Immutable map of [`RoleId`] → [`RoleSpec`] for the built-in specialist roles.
///
/// Construct via [`RoleCatalog::default()`]; the seven built-in roles are
/// always present.  YAML-overridable catalogs are deferred to a future phase.
///
/// # Example
///
/// ```rust
/// use gglib_core::domain::orchestrator::role_catalog::{RoleCatalog, RoleId};
///
/// let catalog = RoleCatalog::default();
/// assert_eq!(catalog.len(), 7);
/// assert!(catalog.get(&RoleId::new("researcher")).is_some());
/// assert!(catalog.get(&RoleId::new("unknown-role")).is_none());
/// ```
pub struct RoleCatalog {
    roles: HashMap<RoleId, RoleSpec>,
}

impl Default for RoleCatalog {
    fn default() -> Self {
        let mut roles: HashMap<RoleId, RoleSpec> = HashMap::with_capacity(7);

        roles.insert(
            RoleId::new("researcher"),
            RoleSpec {
                display_name: "Researcher",
                system_prompt_fragment: "You are a specialist researcher. Your job is to gather \
                    accurate, relevant information from the sources available to you. Prefer \
                    primary sources. Cite your evidence clearly. Do not synthesise or \
                    editorialize — return facts and summaries of what you found.",
                default_tool_allowlist: &["web_search", "read_file"],
                suggested_temperature: 0.3,
                requires_approval: false,
            },
        );

        roles.insert(
            RoleId::new("red-team"),
            RoleSpec {
                display_name: "Red Team",
                system_prompt_fragment: "You are an adversarial critic. Your job is to find \
                    weaknesses, edge cases, and failure modes in the plan or content presented \
                    to you. Be specific and constructive. Do not simply disagree — provide \
                    concrete counter-arguments and evidence where possible.",
                default_tool_allowlist: &[],
                suggested_temperature: 0.7,
                requires_approval: false,
            },
        );

        roles.insert(
            RoleId::new("fact-checker"),
            RoleSpec {
                display_name: "Fact Checker",
                system_prompt_fragment: "You are a rigorous fact-checker. Your job is to verify \
                    every claim in the content presented to you. For each claim, determine whether \
                    it is supported, unsupported, or contradicted by available evidence. Return a \
                    structured list of verdicts with your evidence.",
                default_tool_allowlist: &["web_search"],
                suggested_temperature: 0.2,
                requires_approval: false,
            },
        );

        roles.insert(
            RoleId::new("writer"),
            RoleSpec {
                display_name: "Writer",
                system_prompt_fragment: "You are a skilled writer. Your job is to produce a \
                    first draft of high-quality prose based on the research and outlines provided \
                    to you. Write clearly and engagingly. Do not pad with filler. Follow any \
                    specified format, tone, or length constraints precisely.",
                default_tool_allowlist: &[],
                suggested_temperature: 0.7,
                requires_approval: false,
            },
        );

        roles.insert(
            RoleId::new("editor"),
            RoleSpec {
                display_name: "Editor",
                system_prompt_fragment: "You are a professional editor. Your job is to revise \
                    and polish the draft provided to you. Improve clarity, flow, grammar, and \
                    conciseness without changing the author's voice or intent. Return the revised \
                    full text.",
                default_tool_allowlist: &[],
                suggested_temperature: 0.4,
                requires_approval: false,
            },
        );

        roles.insert(
            RoleId::new("critic"),
            RoleSpec {
                display_name: "Critic",
                system_prompt_fragment: "You are a structured critic. Your job is to identify \
                    gaps, logical inconsistencies, and areas for improvement in the content \
                    provided. Return a numbered list of specific, actionable critiques. Do not \
                    rewrite the content — only identify what needs changing and why.",
                default_tool_allowlist: &[],
                suggested_temperature: 0.5,
                requires_approval: false,
            },
        );

        roles.insert(
            RoleId::new("synthesizer"),
            RoleSpec {
                display_name: "Synthesizer",
                system_prompt_fragment: "You are a synthesis specialist. Your job is to \
                    integrate the outputs from multiple preceding workers into a single coherent, \
                    well-structured response. Eliminate redundancy. Resolve contradictions by \
                    noting them explicitly. Return a unified, polished result.",
                default_tool_allowlist: &[],
                suggested_temperature: 0.5,
                requires_approval: false,
            },
        );

        Self { roles }
    }
}

impl RoleCatalog {
    /// Look up a role by id.
    ///
    /// Returns `None` if no role with the given id exists in the catalog.
    ///
    /// # Example
    ///
    /// ```rust
    /// use gglib_core::domain::orchestrator::role_catalog::{RoleCatalog, RoleId};
    ///
    /// let catalog = RoleCatalog::default();
    /// let spec = catalog.get(&RoleId::new("writer")).unwrap();
    /// assert_eq!(spec.display_name, "Writer");
    /// assert!(catalog.get(&RoleId::new("nonexistent")).is_none());
    /// ```
    pub fn get(&self, id: &RoleId) -> Option<&RoleSpec> {
        self.roles.get(id)
    }

    /// Return the number of roles in the catalog.
    ///
    /// # Example
    ///
    /// ```rust
    /// use gglib_core::domain::orchestrator::role_catalog::RoleCatalog;
    ///
    /// let catalog = RoleCatalog::default();
    /// assert_eq!(catalog.len(), 7);
    /// ```
    pub fn len(&self) -> usize {
        self.roles.len()
    }

    /// Return `true` if the catalog contains no roles.
    pub fn is_empty(&self) -> bool {
        self.roles.is_empty()
    }

    /// Return an iterator over `(RoleId, RoleSpec)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&RoleId, &RoleSpec)> {
        self.roles.iter()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_catalog_has_seven_roles() {
        let catalog = RoleCatalog::default();
        assert_eq!(catalog.len(), 7);
    }

    #[test]
    fn all_builtin_role_ids_resolve() {
        let catalog = RoleCatalog::default();
        for name in [
            "researcher",
            "red-team",
            "fact-checker",
            "writer",
            "editor",
            "critic",
            "synthesizer",
        ] {
            assert!(
                catalog.get(&RoleId::new(name)).is_some(),
                "built-in role '{name}' missing from catalog"
            );
        }
    }

    #[test]
    fn unknown_role_returns_none() {
        let catalog = RoleCatalog::default();
        assert!(catalog.get(&RoleId::new("unknown")).is_none());
    }

    #[test]
    fn researcher_spec_has_web_search_in_allowlist() {
        let catalog = RoleCatalog::default();
        let spec = catalog.get(&RoleId::new("researcher")).unwrap();
        assert!(spec.default_tool_allowlist.contains(&"web_search"));
    }

    #[test]
    fn role_id_display_matches_inner_string() {
        let id = RoleId::new("fact-checker");
        assert_eq!(id.to_string(), "fact-checker");
        assert_eq!(id.as_str(), "fact-checker");
    }

    #[test]
    fn is_empty_returns_false_for_default_catalog() {
        let catalog = RoleCatalog::default();
        assert!(!catalog.is_empty());
    }

    #[test]
    fn iter_yields_seven_entries() {
        let catalog = RoleCatalog::default();
        assert_eq!(catalog.iter().count(), 7);
    }
}
