//! Where an agent loop in this process reports what its guard decided.
//!
//! One accessor, in a file of its own rather than beside [`ProxyOps`]'s
//! others in `proxy.rs`, because that file is at exactly its entry in
//! `scripts/rust-complexity-baseline.txt` and a budget there can be
//! approached but never retreated from. `proxy_port.rs` split off the same
//! way and for the same reason.

use std::sync::Arc;

use gglib_core::ports::AgentGuardSink;

use crate::ProxyOps;

impl ProxyOps {
    /// The ledger an agent loop in this process counts its guard decisions
    /// into, shared with the proxy dashboard that reads them.
    ///
    /// The companion of [`ProxyOps::agent_metrics`], and available for the
    /// same reason: both live on the supervisor for the process's lifetime,
    /// whether or not a proxy is running. GUI chat composes its loop with
    /// this, so a loop or stagnation trip on the agent path reaches a counter
    /// (#1091) instead of ending the run and leaving no trace.
    ///
    /// The count is the per-process ledger's, which resets when the process
    /// does. The loop guard's *log*, which outlives the process, still records
    /// the proxy's pre-dispatch scan alone.
    #[must_use]
    pub fn agent_guard_sink(&self) -> Arc<dyn AgentGuardSink> {
        self.supervisor.agent_guard_sink()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The accessor passes the supervisor's ledger through rather than
    /// standing one up of its own. Pointer identity, because the port is
    /// write-only by design — it has no reader to compare populations with,
    /// and the supervisor's own tests cover what recording into it does.
    #[tokio::test]
    async fn the_sink_is_the_supervisors_own_ledger() {
        let (_core, proxy) = crate::test_support::test_core_and_proxy().await;

        assert!(
            Arc::ptr_eq(
                &proxy.agent_guard_sink(),
                &proxy.supervisor.agent_guard_sink()
            ),
            "a ledger of its own would count agent traffic where nothing reads it"
        );
    }
}
