//! Process-local HITL approval registry.
//!
//! [`CouncilApprovalRegistry`] is the concrete implementation of
//! [`CouncilApprovalRegistryPort`] used by all three adapter surfaces
//! (Axum, CLI, Tauri).  It is intentionally process-local: cross-process
//! approval coordination is out of scope for v1.
//!
//! # Usage
//!
//! ```rust
//! use std::sync::Arc;
//! use gglib_app_services::council_approvals::CouncilApprovalRegistry;
//! use gglib_core::ports::{ApprovalDecision, CouncilApprovalRegistryPort};
//! use tokio::sync::oneshot;
//!
//! # tokio_test::block_on(async {
//! let registry = Arc::new(CouncilApprovalRegistry::new());
//!
//! let (tx, rx) = oneshot::channel::<ApprovalDecision>();
//! registry.register("approval-1".into(), tx);
//! assert!(registry.is_pending("approval-1"));
//!
//! let resolved = registry.resolve("approval-1", ApprovalDecision::Approve);
//! assert!(resolved);
//! assert!(!registry.is_pending("approval-1"));
//!
//! let decision = rx.await.unwrap();
//! assert!(matches!(decision, ApprovalDecision::Approve));
//! # });
//! ```

use dashmap::DashMap;
use tokio::sync::oneshot;

use gglib_core::ports::{ApprovalDecision, CouncilApprovalRegistryPort};

/// Process-local registry mapping `approval_id` → pending oneshot sender.
///
/// Thread-safe via [`DashMap`]; all operations are lock-free sharded.
pub struct CouncilApprovalRegistry {
    pending: DashMap<String, oneshot::Sender<ApprovalDecision>>,
}

impl CouncilApprovalRegistry {
    /// Create a new, empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pending: DashMap::new(),
        }
    }
}

impl Default for CouncilApprovalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CouncilApprovalRegistryPort for CouncilApprovalRegistry {
    fn register(&self, approval_id: String, sender: oneshot::Sender<ApprovalDecision>) {
        self.pending.insert(approval_id, sender);
    }

    fn resolve(&self, approval_id: &str, decision: ApprovalDecision) -> bool {
        if let Some((_, sender)) = self.pending.remove(approval_id) {
            // If the receiver was dropped (executor timed out / cancelled),
            // the send will silently fail — that is the correct behaviour.
            let _ = sender.send(decision);
            true
        } else {
            false
        }
    }

    fn is_pending(&self, approval_id: &str) -> bool {
        self.pending.contains_key(approval_id)
    }
}
