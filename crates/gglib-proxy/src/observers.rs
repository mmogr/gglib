//! What a proxy run reports to that outlives the run.
//!
//! [`serve`](crate::serve) takes one of these rather than a parameter per
//! observer. Everything in it is owned a level up — by the supervisor, which
//! restarts a proxy without ending the process — so a stop and a start keep
//! counting into the same place. Both fields are observers only: nothing the
//! proxy decides reads them back.

use std::sync::Arc;

use gglib_core::domain::defects::ModelDefectLedger;
use gglib_core::ports::LoopGuardTripSink;

/// The observers a proxy run reports to. [`Default`] is a fresh ledger and no
/// log, which is what a test wants.
#[derive(Clone, Default)]
pub struct ProxyObservers {
    /// Per-model defect counters, per process, fed by the context-metrics
    /// store, which sees every signal with the model name attached.
    pub defects: Arc<ModelDefectLedger>,
    /// Where the loop guard records each decision and each scanned request,
    /// to outlive the process. `None` records nothing.
    pub loop_guard_trips: Option<Arc<dyn LoopGuardTripSink>>,
}
