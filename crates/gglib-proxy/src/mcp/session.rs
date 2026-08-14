//! MCP session management for the Streamable HTTP transport.
//!
//! Tracks active sessions identified by `Mcp-Session-Id` header values.
//! Each session is created during `initialize` and removed on
//! `DELETE /mcp` or expiry.
//!
//! Thread-safe: all access goes through `Arc<RwLock<...>>`.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

/// State for a single MCP session.
#[derive(Debug)]
pub(crate) struct SessionState {
    /// Whether the client has sent `notifications/initialized`.
    pub initialized: bool,
}

/// Thread-safe in-memory session store.
///
/// Sessions are keyed by a cryptographically-random UUID v4 string
/// sent to the client via the `Mcp-Session-Id` response header.
#[derive(Debug, Clone)]
pub(crate) struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, SessionState>>>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    /// Create an empty session manager.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new session and return its ID.
    pub(crate) async fn create_session(&self) -> String {
        let id = Uuid::new_v4().to_string();
        let state = SessionState { initialized: false };
        self.sessions.write().await.insert(id.clone(), state);
        id
    }

    /// Check whether a session ID exists.
    pub(crate) async fn validate_session(&self, id: &str) -> bool {
        self.sessions.read().await.contains_key(id)
    }

    /// Mark a session as having completed initialization.
    ///
    /// Returns `false` if the session does not exist.
    pub(crate) async fn mark_initialized(&self, id: &str) -> bool {
        if let Some(state) = self.sessions.write().await.get_mut(id) {
            state.initialized = true;
            true
        } else {
            false
        }
    }

    /// Remove a session (e.g. on `DELETE /mcp`).
    ///
    /// Returns `true` if the session existed and was removed.
    pub(crate) async fn remove_session(&self, id: &str) -> bool {
        self.sessions.write().await.remove(id).is_some()
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_and_validate() {
        let mgr = SessionManager::new();
        let id = mgr.create_session().await;

        assert!(mgr.validate_session(&id).await);
        assert!(!mgr.validate_session("nonexistent").await);
    }

    #[tokio::test]
    async fn mark_initialized() {
        let mgr = SessionManager::new();
        let id = mgr.create_session().await;

        assert!(mgr.mark_initialized(&id).await);
        assert!(!mgr.mark_initialized("nonexistent").await);
    }

    #[tokio::test]
    async fn remove_session() {
        let mgr = SessionManager::new();
        let id = mgr.create_session().await;

        assert!(mgr.remove_session(&id).await);
        assert!(!mgr.validate_session(&id).await);
        assert!(!mgr.remove_session(&id).await);
    }
}
