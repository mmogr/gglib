//! The agentic eval's proxy arm: a real `gglib-proxy`, started in-process just
//! before that arm and stopped just after it.
//!
//! Every other arm posts to llama-server directly; this one measures
//! tool-call repair and the loop guard, the two things it is chosen for,
//! through the proxy that applies them (#1047). It sends every turn through
//! `gglib_proxy::serve`, bound to a free loopback port, in front of the model
//! the eval already holds.
//!
//! What the proxy is given, and why:
//!
//! - **A runtime port pinned to the held target** ([`PinnedTarget`]). The
//!   eval's own admission lease covers every arm. The proxy must neither
//!   launch a second model (it admits per request, and a context that differs
//!   from the held one would recycle it) nor stop this one (it does on a dead
//!   upstream, and its watchdog does on a stall). So this port answers every
//!   admission with the held target, refuses any other model, and refuses to
//!   stop anything.
//! - **Fixed settings** ([`FixedSettings`]): repair on, the loop guard in its
//!   default `note` mode, client sampling trusted so each run's seed survives,
//!   and no global sampling defaults. A reading then means the same on any
//!   machine. The report records the first two ([`ProxyArmSettings`]).
//! - **Its own defect ledger and no trip log.** No other arm sends it a
//!   request, so the ledger's counts are this arm's alone. Benchmark traffic
//!   never reaches the daemon's loop-guard log, which ADR 0011's kill
//!   criterion reads.
//! - **An MCP service with no servers** ([`NoMcpServers`]). The proxy serves
//!   `/mcp`, and this one has no API key; given the daemon's servers, any local
//!   process could reach their tools through it while the arm ran.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use gglib_core::ProxyAccessConfig;
use gglib_core::domain::benchmark::agentic::{EvalArm, ProxyArmSettings, ProxyTaskRuns};
use gglib_core::domain::benchmark::tune::result::TuneTaskResult;
use gglib_core::domain::benchmark::tune::task::TuneTask;
use gglib_core::domain::defect_counts::ModelDefectCounts;
use gglib_core::domain::defects::ModelDefectLedger;
use gglib_core::ports::{ModelCatalogPort, RunningTarget};
use gglib_mcp::McpService;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::{CancellationToken, DropGuard};
use tracing::warn;

#[path = "proxy_arm_ports.rs"]
mod proxy_arm_ports;
use proxy_arm_ports::{FixedSettings, NoMcpServers, PinnedTarget};

/// How long [`ProxyArm::finish`] waits for the proxy to wind down.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(5);

/// A running in-process proxy in front of the eval's held model.
///
/// Dropping it cancels the proxy, so every early return in the eval stops
/// it without a call of its own.
pub(super) struct ProxyArm {
    base_url: String,
    defects: Arc<ModelDefectLedger>,
    settings: ProxyArmSettings,
    stop: DropGuard,
    task: JoinHandle<()>,
}

impl ProxyArm {
    /// Bind a loopback port and start the proxy on it, in front of `target`.
    pub(super) async fn start(
        target: RunningTarget,
        catalog: Arc<dyn ModelCatalogPort>,
    ) -> Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("failed to bind a port for the proxy arm")?;
        let addr = listener.local_addr()?;
        let defects = Arc::new(ModelDefectLedger::new());
        let fixed = FixedSettings::for_eval();
        let settings = ProxyArmSettings {
            tool_call_repair: !gglib_core::debug_switches::enabled(
                gglib_proxy::repair::DISABLE_REPAIR_ENV,
            ),
            loop_guard_mode: fixed.loop_guard_mode(),
        };
        let observers = gglib_proxy::ProxyObservers {
            defects: Arc::clone(&defects),
            loop_guard_trips: None,
        };
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let default_ctx = Some(target.effective_ctx);
        let task = tokio::spawn(async move {
            let served = gglib_proxy::serve(
                listener,
                default_ctx,
                // Only the context fit reads it, and the pinned target
                // never fits anything.
                true,
                Arc::new(PinnedTarget { target }),
                catalog,
                Arc::new(McpService::new(Arc::new(NoMcpServers))),
                token,
                None,
                Arc::new(fixed),
                None,
                None,
                // No prompt cache, as in the eval's own runtime: one would
                // perturb what the other arms measure without it.
                false,
                None,
                gglib_proxy::slot_eviction::DiskBudget::Auto,
                Arc::new(gglib_core::cache_metrics::CacheMetricsStore::new()),
                observers,
                &ProxyAccessConfig::default(),
            )
            .await;
            if let Err(e) = served {
                warn!("agentic eval: the proxy arm's proxy stopped with an error: {e:#}");
            }
        });
        Ok(Self {
            base_url: format!("http://{addr}"),
            defects,
            settings,
            stop: cancel.drop_guard(),
            task,
        })
    }

    /// Where the arm's requests go.
    pub(super) fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Stop the proxy and return what it counted, with the settings it ran
    /// under.
    pub(super) async fn finish(self) -> (ModelDefectCounts, ProxyArmSettings) {
        let Self {
            defects,
            settings,
            stop,
            task,
            ..
        } = self;
        drop(stop);
        if tokio::time::timeout(SHUTDOWN_WAIT, task).await.is_err() {
            warn!("agentic eval: the proxy arm's proxy did not stop within {SHUTDOWN_WAIT:?}");
        }
        (only_model(defects.snapshot()), settings)
    }
}

/// The one model's counts in a ledger that has only ever seen one model.
///
/// Keyed by whatever name the proxy resolved, which need not be the catalog
/// name the eval used. A second key would mean traffic this arm did not send,
/// so it is said out loud rather than summed into the arm's numbers.
fn only_model(snapshot: HashMap<String, ModelDefectCounts>) -> ModelDefectCounts {
    let mut entries = snapshot.into_iter();
    let Some((_, counts)) = entries.next() else {
        return ModelDefectCounts::default();
    };
    let others: Vec<String> = entries.map(|(name, _)| name).collect();
    if !others.is_empty() {
        warn!(
            "agentic eval: the proxy arm's ledger counted more than one model ({}); \
             reporting one of them",
            others.join(", ")
        );
    }
    counts
}

/// Whether an arm opens a task that demands a call with `"required"`.
///
/// Every arm but the proxy pair. Under `"auto"` the proxy judges every call
/// whose schema it can judge.
/// Under `"required"` it judges only a turn gglib's own grammar constrained,
/// which it installs for a dialect model, and forwards the rest unjudged.
pub(super) const fn opens_with_required(arm: EvalArm) -> bool {
    !matches!(arm, EvalArm::RawAuto | EvalArm::Proxy)
}

/// Where an arm's requests go: the proxy for the proxy arm, llama-server for
/// every other.
pub(super) fn arm_base_url<'a>(
    arm: EvalArm,
    upstream: &'a str,
    proxy: Option<&'a ProxyArm>,
) -> &'a str {
    match (arm, proxy) {
        (EvalArm::Proxy, Some(proxy)) => proxy.base_url(),
        _ => upstream,
    }
}

/// The proxy pair's per-task drill-down, in suite order.
pub(super) fn proxy_task_runs(
    tasks: &[TuneTask],
    raw_auto: Vec<Vec<TuneTaskResult>>,
    proxy: Vec<Vec<TuneTaskResult>>,
) -> Vec<ProxyTaskRuns> {
    tasks
        .iter()
        .zip(raw_auto.into_iter().zip(proxy))
        .map(|(task, (raw_auto, proxy))| ProxyTaskRuns {
            task_id: task.id.clone(),
            raw_auto,
            proxy,
        })
        .collect()
}

#[cfg(test)]
#[path = "proxy_arm_tests.rs"]
mod proxy_arm_tests;
