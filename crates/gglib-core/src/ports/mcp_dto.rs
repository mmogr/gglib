//! MCP DTOs for cross-boundary communication (Tauri, Axum, TypeScript).
//!
//! These types are designed to be serde-stable and avoid internal implementation
//! details like `PathBuf` crossing the wire.

use serde::{Deserialize, Serialize};

/// Result of executable path resolution for a server.
///
/// This is the contract between backend (Rust) and frontend (TypeScript/Axum clients).
/// All fields are String-based to ensure clean JSON serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResolutionStatus {
    /// Whether resolution succeeded.
    pub success: bool,

    /// The resolved absolute path (if successful).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<String>,

    /// All attempts made during resolution (for diagnostics).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attempts: Vec<ResolutionAttempt>,

    /// Non-fatal warnings.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<String>,

    /// Error message (if resolution failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,

    /// Suggested command to run to find the executable (e.g., "command -v npx").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<String>,
}

/// A single resolution attempt for diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResolutionAttempt {
    /// The candidate path that was tried.
    pub candidate: String,

    /// The outcome of checking this candidate (simple string for cross-language compat).
    pub outcome: String,
}

impl ResolutionStatus {
    /// Get a user-friendly error message with suggestions.
    pub fn error_message_with_suggestions(&self) -> String {
        let base_msg = self
            .error_message
            .clone()
            .unwrap_or_else(|| "Resolution failed".to_string());

        if self.attempts.is_empty() {
            return base_msg;
        }

        let attempts_list: Vec<String> = self
            .attempts
            .iter()
            .map(|a| format!("  ✗ {}: {}", a.candidate, a.outcome))
            .collect();

        let mut msg = format!("{}\n\nTried:\n{}", base_msg, attempts_list.join("\n"));

        if let Some(fix) = &self.suggested_fix {
            msg.push_str("\n\nSuggested fix: ");
            msg.push_str(fix);
        }

        // Add install hint if all attempts are NotFound
        let all_not_found = self
            .attempts
            .iter()
            .all(|a| a.outcome.contains("not found"));
        if all_not_found {
            msg.push_str("\n\n• Install Node.js if not installed");
        }

        msg
    }
}
