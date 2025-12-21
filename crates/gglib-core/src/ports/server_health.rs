//! Server health status types for monitoring.
//!
//! These types define the health states that a server process can be in,
//! used for continuous monitoring after initial startup.

use serde::{Deserialize, Serialize};

/// Health status of a running server process.
///
/// Used by monitoring systems to track server state and emit lifecycle events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ServerHealthStatus {
    /// Server is responding to health checks and process is alive.
    Healthy,

    /// Server is running but experiencing issues.
    ///
    /// Example: HTTP health endpoint returns non-200 status.
    Degraded {
        /// Human-readable reason for degraded state.
        reason: String,
    },

    /// Server process is alive but HTTP endpoint is unreachable.
    ///
    /// Example: Connection timeout or refused.
    Unreachable {
        /// Last error message from health check attempt.
        #[serde(rename = "lastError")]
        last_error: String,
    },

    /// Server process has died unexpectedly.
    ///
    /// Detected via PID check (process no longer exists).
    ProcessDied,
}

impl ServerHealthStatus {
    /// Check if the status represents a healthy state.
    #[must_use]
    pub const fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy)
    }

    /// Check if the status represents a failed/critical state.
    #[must_use]
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::ProcessDied | Self::Unreachable { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_classification() {
        assert!(ServerHealthStatus::Healthy.is_healthy());
        assert!(!ServerHealthStatus::Healthy.is_failed());

        let degraded = ServerHealthStatus::Degraded {
            reason: "slow response".to_string(),
        };
        assert!(!degraded.is_healthy());
        assert!(!degraded.is_failed());

        let unreachable = ServerHealthStatus::Unreachable {
            last_error: "connection refused".to_string(),
        };
        assert!(!unreachable.is_healthy());
        assert!(unreachable.is_failed());

        assert!(!ServerHealthStatus::ProcessDied.is_healthy());
        assert!(ServerHealthStatus::ProcessDied.is_failed());
    }

    #[test]
    fn test_serialization() {
        let status = ServerHealthStatus::Degraded {
            reason: "high latency".to_string(),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"status\":\"degraded\""));
        assert!(json.contains("\"reason\":\"high latency\""));
    }
}
