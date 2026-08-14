//! Background poller for llama.cpp's `GET /slots` endpoint.
//!
//! Kept deliberately separate from [`crate::slots`] (which is pure
//! fetch-and-parse): this module owns the *stateful* parts — the polling
//! interval, exponential backoff on failure, the "disabled" latch, and the
//! last-known-result cache — so `slots.rs` itself stays a small, easily
//! unit-tested leaf.
//!
//! ## Resilience
//!
//! [`fetch_slots`] never panics, and neither does this module: every branch
//! of the poll loop matches on a [`SlotsPollResult`] variant and either
//! updates the cache or adjusts the sleep duration. A struggling or
//! unreachable llama-server can only ever slow the poller down (via
//! [`next_backoff`], capped at [`MAX_POLL_BACKOFF`]) — it can never crash it
//! or block request-handling tasks, since it runs as its own `tokio::spawn`
//! task entirely isolated from the Axum handlers.
//!
//! ## Lifecycle
//!
//! [`spawn_slots_poller`] races its sleep against the shared shutdown
//! [`CancellationToken`] on every iteration, so it returns promptly when the
//! proxy server shuts down instead of sleeping out a long backoff first.
//! `serve()` awaits the returned `JoinHandle` after `axum::serve` completes,
//! so the task is always joined rather than left detached.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::Client;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use gglib_core::domain::ModelSamplingDefaults;
use gglib_core::ports::ModelRuntimePort;

use crate::connections::ActiveConnectionsRegistry;
use crate::props::{BaselineReport, BaselineState, PropsResult, fetch_props};
use crate::sampling_audit::{SamplingAuditStore, SlotParams, compare_poll};
use crate::slots::{SlotsPollResult, fetch_slots};

/// Polling cadence while llama-server is responding normally.
const BASE_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Ceiling for exponential backoff after consecutive failed polls.
const MAX_POLL_BACKOFF: Duration = Duration::from_secs(30);

// =============================================================================
// SlotsCache
// =============================================================================

/// Holds the most recent [`SlotsPollResult`], shared between the poller
/// task and (in a future phase) the dashboard HTTP handlers.
///
/// Uses `std::sync::Mutex`, following the same synchronous-critical-section
/// convention as [`crate::metrics::ContextMetricsStore`] and
/// [`crate::connections::ActiveConnectionsRegistry`]: `get`/`set` are a
/// single clone/assign with no `.await` inside, so the lock can never be
/// held across an await point.
pub struct SlotsCache {
    latest: Mutex<SlotsPollResult>,
}

impl Default for SlotsCache {
    fn default() -> Self {
        Self {
            latest: Mutex::new(SlotsPollResult::Unreachable(
                "no /slots poll has completed yet".to_string(),
            )),
        }
    }
}

impl SlotsCache {
    /// Create a cache with an initial "not polled yet" placeholder state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The most recently observed poll result.
    #[must_use]
    pub fn get(&self) -> SlotsPollResult {
        self.latest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Overwrite the cached result. Called from the poller task; also used
    /// directly by `dashboard`'s unit tests to seed a known state without
    /// spinning up the poller.
    pub(crate) fn set(&self, result: SlotsPollResult) {
        *self.latest.lock().unwrap_or_else(|e| e.into_inner()) = result;
    }
}

// =============================================================================
// Sampling readback (pure parts)
// =============================================================================

/// The `params` of every slot that was actively processing.
///
/// Only a busy slot carries them — llama.cpp omits the object entirely on an
/// idle one rather than reporting the last request's values, so this needs no
/// staleness guard. Filtering on `is_processing` as well is belt-and-braces
/// against a build that reports both.
fn busy_slot_params(result: &SlotsPollResult) -> Vec<SlotParams> {
    match result {
        SlotsPollResult::Available(slots) => slots
            .iter()
            .filter(|s| s.is_processing)
            .filter_map(|s| s.params.clone())
            .collect(),
        SlotsPollResult::Disabled | SlotsPollResult::Unreachable(_) => Vec::new(),
    }
}

/// Why the readback cannot see, given a poll result — or `None` when it can.
///
/// A reachable server with slots is *not* proof of sight, so this deliberately
/// does not clear the latch; only an actual comparison does (see
/// [`SamplingAuditStore::record_poll`]). What it does is name the two states
/// where sight is structurally impossible, so the dashboard shows a reason
/// rather than a zero.
fn blindness(result: &SlotsPollResult) -> Option<String> {
    match result {
        SlotsPollResult::Disabled => Some(
            "llama-server was launched with --no-slots, so no request's sampling can be \
             read back"
                .to_string(),
        ),
        SlotsPollResult::Unreachable(msg) => Some(format!("/slots is unreadable: {msg}")),
        SlotsPollResult::Available(_) => None,
    }
}

/// Run one readback against a completed poll.
///
/// Split from the task body so the wiring — which intents, which slots, what
/// gets counted — is testable without a server, a timer, or a spawned task.
fn audit_one_poll(
    result: &SlotsPollResult,
    model_name: &str,
    connections: &ActiveConnectionsRegistry,
    audit: &SamplingAuditStore,
) {
    if let Some(reason) = blindness(result) {
        audit.mark_blind(reason);
        return;
    }

    let observed = busy_slot_params(result);
    if observed.is_empty() {
        return;
    }

    let intents = connections.in_flight_sampling(model_name);
    let outcome = compare_poll(&intents, &observed);

    for d in &outcome.found {
        // `warn!` rather than `error!`: nothing is broken for the user, and
        // the request in question has already been served. This is a signal
        // for whoever is holding the evidence, per ADR 0001's rule that a Tier
        // C organ reports and never acts.
        warn!(
            field = d.field,
            sent = d.sent,
            observed = d.observed,
            provenance = %d.provenance,
            model = model_name,
            "sampling readback: llama-server reports a value gglib did not send"
        );
    }
    audit.record_poll(&outcome);
}

// =============================================================================
// Backoff arithmetic (pure, unit-tested)
// =============================================================================

/// Compute the next backoff delay after a failed poll: double the current
/// delay, capped at [`MAX_POLL_BACKOFF`].
#[must_use]
fn next_backoff(current: Duration) -> Duration {
    current.saturating_mul(2).min(MAX_POLL_BACKOFF)
}

/// Decide the delay before the next poll attempt given the outcome of the
/// most recent one, or `None` to signal that polling should stop entirely
/// (the [`SlotsPollResult::Disabled`] case).
///
/// Pure and synchronous so the state-machine logic is unit-testable
/// without spinning up a real timer or task.
#[must_use]
fn next_delay(result: &SlotsPollResult, current_backoff: Duration) -> Option<Duration> {
    match result {
        SlotsPollResult::Available(_) => Some(BASE_POLL_INTERVAL),
        SlotsPollResult::Unreachable(_) => Some(next_backoff(current_backoff)),
        SlotsPollResult::Disabled => None,
    }
}

// =============================================================================
// Poller task
// =============================================================================

/// Read `/props` for a newly-seen model and fold the baseline check into the
/// audit store.
///
/// Once per launch, not per poll: `default_generation_settings` cannot change
/// while a server runs. See [`crate::props`] for what the check can and cannot
/// conclude.
///
/// # Returns whether a reading was actually stored
///
/// The caller latches on this rather than on having made the attempt. A
/// `/props` read fails most often because llama-server has not finished
/// starting, which is precisely when the first poll after a model change
/// lands — so latching on the attempt lost the baseline for the whole of that
/// model's run, and the surface said "not read yet" forever.
async fn read_baseline(
    client: &Client,
    base_url: &str,
    model_name: &str,
    model: Option<&ModelSamplingDefaults>,
    audit: &SamplingAuditStore,
) -> bool {
    // Set before the read is attempted, and from the same value the baseline
    // check uses: an unreadable `/props` says nothing about what the model
    // publishes, which gglib knows from the GGUF either way.
    audit.set_model_sampling(model_name, model.copied());

    match fetch_props(client, base_url).await {
        PropsResult::Available(params) => {
            let report = BaselineReport::from_params(&params, model);
            for field in report.drifted() {
                warn!(
                    field = field.field,
                    verdict = ?field.verdict,
                    "sampling baseline: this build's default has moved since it was measured; \
                     ADR 0003's deferral is re-opened for this parameter"
                );
            }
            // Not a warning: a model shipping its own recipe is llama.cpp
            // working as designed, and gglib deferring to it is ADR 0003's
            // decision reaching one layer further than it was written for.
            // Worth one line per launch because it is the reason the baseline
            // check cannot speak for those fields.
            let supplied = report.model_supplied();
            if !supplied.is_empty() {
                info!(
                    fields = %supplied
                        .iter()
                        .map(|f| f.field)
                        .collect::<Vec<_>>()
                        .join(", "),
                    "sampling baseline: this model's own GGUF supplies these defaults, so \
                     the build's own values are not observable for them"
                );
            }
            debug!(
                coverage = ?report.coverage,
                "read llama-server sampling defaults from /props"
            );
            audit.set_baseline(BaselineState::Read { report });
            true
        }
        PropsResult::Unavailable(reason) => {
            debug!("proxy dashboard: /props unreadable ({reason}); no sampling baseline");
            audit.set_baseline(BaselineState::Unreadable { reason });
            false
        }
    }
}

/// Decides when `/props` is due to be re-read.
///
/// Pulled out of the task body for the same reason [`audit_one_poll`] is: the
/// interesting behaviour is a state transition across model swaps and failed
/// reads, and it should be testable without a server, a timer or a spawned
/// task.
///
/// The rule is "re-read whenever the running model is not the one we last read
/// *successfully* for". Latching on the attempt instead is what lost the
/// baseline for a whole run, and latching only on success without clearing
/// first has its own hole — see [`Self::due`].
#[derive(Default)]
struct BaselineLatch {
    read_for: Option<String>,
}

impl BaselineLatch {
    /// Whether `/props` should be read for `model_name` now.
    ///
    /// Clears the latch as a side effect when it names a different model, so a
    /// failed read cannot leave the *previous* model's name recorded. Without
    /// that, swapping A → B where B's `/props` fails, then back to B → A,
    /// would find A still latched and skip a read that never succeeded.
    fn due(&mut self, model_name: &str) -> bool {
        if self.read_for.as_deref() == Some(model_name) {
            return false;
        }
        self.read_for = None;
        true
    }

    /// Record that a read for `model_name` stored a baseline.
    fn succeeded(&mut self, model_name: &str) {
        self.read_for = Some(model_name.to_owned());
    }
}

/// Spawn the background `/slots` poller as its own Tokio task.
///
/// Polls at [`BASE_POLL_INTERVAL`] while llama-server is reachable, with
/// exponential backoff (capped at [`MAX_POLL_BACKOFF`], reset to base on
/// the next success) while it is not. If `runtime_port.current_model()`
/// reports no model running, the HTTP call is skipped entirely for that
/// tick. If a `501`/`--no-slots` response is ever observed, the task logs
/// it once and returns for good — no further polling for the remainder of
/// this server run. In every case the task returns promptly once `cancel`
/// is triggered, rather than sleeping out a pending backoff first.
///
/// # It also drives the sampling readback
///
/// The poll this task already makes carries the one field that answers "did
/// what gglib resolved reach llama-server" ([`crate::sampling_audit`]), so the
/// readback rides along rather than opening a second connection at a second
/// cadence. `connections` supplies the intents to compare against — the set of
/// requests in flight, which is exactly the set that could be occupying the
/// slots this poll observed.
///
/// A model change also triggers a one-off `/props` read for the baseline
/// check. Keyed on the model name rather than on `just_started`, because this
/// task has no launch hook and a name change is the only swap signal it sees —
/// and on a *successful* read rather than on the attempt, so a server that was
/// still starting when the first poll landed gets read on the next tick
/// instead of going without a baseline for the rest of its run. See
/// [`BaselineLatch`].
pub(crate) fn spawn_slots_poller(
    runtime_port: Arc<dyn ModelRuntimePort>,
    client: Client,
    cache: Arc<SlotsCache>,
    connections: Arc<ActiveConnectionsRegistry>,
    audit: Arc<SamplingAuditStore>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut backoff = BASE_POLL_INTERVAL;
        let mut baseline = BaselineLatch::default();

        loop {
            let sleep_for = match runtime_port.current_model().await {
                None => BASE_POLL_INTERVAL,
                Some(target) => {
                    if baseline.due(&target.model_name)
                        && read_baseline(
                            &client,
                            &target.base_url,
                            &target.model_name,
                            target.model_sampling.as_ref(),
                            &audit,
                        )
                        .await
                    {
                        baseline.succeeded(&target.model_name);
                    }

                    let result = fetch_slots(&client, &target.base_url).await;
                    if let SlotsPollResult::Unreachable(ref msg) = result {
                        warn!(
                            "proxy dashboard: /slots poll failed ({msg}); backing off to {backoff:?}"
                        );
                    }
                    audit_one_poll(&result, &target.model_name, &connections, &audit);
                    let delay = next_delay(&result, backoff);
                    cache.set(result);
                    match delay {
                        Some(delay) => {
                            backoff = delay;
                            delay
                        }
                        None => {
                            info!(
                                "proxy dashboard: /slots endpoint is disabled upstream (--no-slots); poller stopping"
                            );
                            return;
                        }
                    }
                }
            };

            tokio::select! {
                () = cancel.cancelled() => {
                    debug!("proxy dashboard: /slots poller shutting down");
                    return;
                }
                () = tokio::time::sleep(sleep_for) => {}
            }
        }
    })
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use gglib_core::ports::{ModelRuntimeError, RunningTarget};

    #[test]
    fn next_backoff_doubles() {
        assert_eq!(next_backoff(Duration::from_secs(1)), Duration::from_secs(2));
        assert_eq!(next_backoff(Duration::from_secs(2)), Duration::from_secs(4));
    }

    #[test]
    fn next_backoff_caps_at_ceiling() {
        assert_eq!(
            next_backoff(Duration::from_secs(20)),
            Duration::from_secs(30)
        );
        assert_eq!(
            next_backoff(Duration::from_secs(30)),
            Duration::from_secs(30)
        );
        assert_eq!(
            next_backoff(Duration::from_secs(100)),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn next_delay_resets_to_base_on_success() {
        let result = SlotsPollResult::Available(vec![]);
        assert_eq!(
            next_delay(&result, Duration::from_secs(16)),
            Some(BASE_POLL_INTERVAL)
        );
    }

    #[test]
    fn next_delay_backs_off_on_unreachable() {
        let result = SlotsPollResult::Unreachable("connection refused".to_string());
        assert_eq!(
            next_delay(&result, Duration::from_secs(4)),
            Some(Duration::from_secs(8))
        );
    }

    #[test]
    fn next_delay_signals_stop_on_disabled() {
        assert_eq!(
            next_delay(&SlotsPollResult::Disabled, Duration::from_secs(1)),
            None
        );
    }

    // ── The /props baseline latch ─────────────────────────────────────────

    /// The ordinary path: read once per model, not once per second.
    #[test]
    fn a_baseline_is_read_once_per_model_and_not_again() {
        let mut latch = BaselineLatch::default();

        assert!(latch.due("m"));
        latch.succeeded("m");
        assert!(
            !latch.due("m"),
            "a stored baseline is not re-read every tick"
        );
    }

    /// **The defect.** `/props` fails most often because llama-server has not
    /// finished starting, which is exactly when the first poll after a model
    /// change lands. Latching on the attempt meant one such failure left the
    /// model with no baseline for the whole of its run.
    #[test]
    fn a_failed_read_is_retried_on_the_next_tick() {
        let mut latch = BaselineLatch::default();

        assert!(latch.due("m"));
        // read_baseline returned false, so `succeeded` is never called.
        assert!(latch.due("m"), "a failed read must not latch");
        assert!(latch.due("m"), "and must keep being retried");

        latch.succeeded("m");
        assert!(!latch.due("m"));
    }

    #[test]
    fn a_model_swap_forces_a_fresh_read() {
        let mut latch = BaselineLatch::default();
        latch.succeeded("model-a");

        assert!(latch.due("model-b"), "a different model needs its own read");
    }

    /// The hole that "latch only on success" leaves if the latch is not also
    /// cleared on the way in: B's failed read would leave A's name recorded,
    /// and swapping back to A would skip a read whose result had since been
    /// cleared by that failure.
    #[test]
    fn swapping_away_and_back_after_a_failure_still_reads() {
        let mut latch = BaselineLatch::default();
        latch.succeeded("model-a");

        assert!(latch.due("model-b"));
        // model-b's read fails: no `succeeded` call.
        assert!(
            latch.due("model-a"),
            "model-a's baseline was cleared by model-b's attempt, so it must be re-read"
        );
    }

    #[test]
    fn cache_defaults_to_unreachable_placeholder() {
        let cache = SlotsCache::new();
        assert!(matches!(cache.get(), SlotsPollResult::Unreachable(_)));
    }

    #[test]
    fn cache_get_reflects_latest_set() {
        let cache = SlotsCache::new();
        cache.set(SlotsPollResult::Disabled);
        assert_eq!(cache.get(), SlotsPollResult::Disabled);
    }

    // ── Sampling readback wiring ──────────────────────────────────────────

    use crate::sampling_audit::AuditState;
    use gglib_core::domain::{FieldSources, InferenceConfig, ParamSource};
    use gglib_core::request_pipeline::{FloorClass, SamplingDecision};

    /// Built through the real parser rather than by hand, so these tests
    /// exercise the `/slots` shape the poller actually receives.
    fn slots(json: &str) -> SlotsPollResult {
        SlotsPollResult::Available(serde_json::from_str(json).expect("fixture parses"))
    }

    const BUSY_AT_TEMP_0_7: &str = r#"[
        {"id": 0, "is_processing": true, "params": {"temperature": 0.7, "top_k": 40}}
    ]"#;

    const IDLE: &str = r#"[{"id": 0, "is_processing": false}]"#;

    fn intent(temperature: f32) -> SamplingDecision {
        SamplingDecision {
            resolved: InferenceConfig {
                temperature: Some(temperature),
                ..Default::default()
            },
            sources: FieldSources {
                temperature: ParamSource::Floor,
                top_p: ParamSource::Unset,
                top_k: ParamSource::Unset,
                max_tokens: ParamSource::Unset,
                repeat_penalty: ParamSource::Unset,
                presence_penalty: ParamSource::Unset,
                min_p: ParamSource::Unset,
                dynatemp_range: ParamSource::Unset,
                dynatemp_exponent: ParamSource::Unset,
                top_n_sigma: ParamSource::Unset,
                dry_multiplier: ParamSource::Unset,
                dry_base: ParamSource::Unset,
                dry_allowed_length: ParamSource::Unset,
                dry_penalty_last_n: ParamSource::Unset,
                frequency_penalty: ParamSource::Unset,
            },
            layer_names: ["cli", "client", "profile", "model", "global", "auto"],
            floor: FloorClass::Default,
            agentic_turn: false,
            agentic_ceiling_applied: None,
            client_fields_rejected: Vec::new(),
            client_fields_discarded: Vec::new(),
            applied: true,
        }
    }

    /// One in-flight request, one busy slot, values agreeing.
    #[test]
    fn a_busy_slot_is_compared_against_the_intent_in_flight() {
        let connections = Arc::new(ActiveConnectionsRegistry::new());
        let audit = SamplingAuditStore::new();
        let guard = connections.register("m", true, None);
        guard.record_sampling(intent(0.7));

        audit_one_poll(&slots(BUSY_AT_TEMP_0_7), "m", &connections, &audit);

        assert_eq!(
            audit.state(),
            AuditState::Comparing {
                comparisons: 1,
                divergences: 0
            }
        );
    }

    #[test]
    fn a_divergence_reaches_the_store_with_its_provenance() {
        let connections = Arc::new(ActiveConnectionsRegistry::new());
        let audit = SamplingAuditStore::new();
        let guard = connections.register("m", true, None);
        guard.record_sampling(intent(0.2)); // the slot reports 0.7

        audit_one_poll(&slots(BUSY_AT_TEMP_0_7), "m", &connections, &audit);

        assert_eq!(
            audit.state(),
            AuditState::Comparing {
                comparisons: 1,
                divergences: 1
            }
        );
        let snap = audit.snapshot();
        assert_eq!(snap.recent_divergences.len(), 1);
        assert_eq!(snap.recent_divergences[0].field, "temperature");
        assert_eq!(snap.recent_divergences[0].provenance, "floor");
    }

    /// An idle slot carries no `params`, so a quiet server must leave the
    /// organ at `NotYetObserved` rather than manufacturing a comparison.
    #[test]
    fn an_idle_slot_contributes_no_observation() {
        let connections = Arc::new(ActiveConnectionsRegistry::new());
        let audit = SamplingAuditStore::new();
        let guard = connections.register("m", true, None);
        guard.record_sampling(intent(0.7));

        audit_one_poll(&slots(IDLE), "m", &connections, &audit);

        assert_eq!(audit.state(), AuditState::NotYetObserved);
    }

    /// A slot for a model nobody has a request in flight for cannot be
    /// attributed, and absence of intent is not ambiguity.
    #[test]
    fn a_slot_with_no_matching_intent_compares_nothing() {
        let connections = Arc::new(ActiveConnectionsRegistry::new());
        let audit = SamplingAuditStore::new();
        let guard = connections.register("other-model", true, None);
        guard.record_sampling(intent(0.7));

        audit_one_poll(&slots(BUSY_AT_TEMP_0_7), "m", &connections, &audit);

        assert_eq!(audit.state(), AuditState::NotYetObserved);
        assert_eq!(audit.snapshot().skipped_ambiguous, 0);
    }

    /// The distinction the whole liveness contract exists for: `--no-slots`
    /// must report *why* nothing is being compared, not zero divergences.
    #[test]
    fn a_disabled_slots_endpoint_blinds_the_readback() {
        let connections = Arc::new(ActiveConnectionsRegistry::new());
        let audit = SamplingAuditStore::new();

        audit_one_poll(&SlotsPollResult::Disabled, "m", &connections, &audit);

        match audit.state() {
            AuditState::Blind { reason } => assert!(reason.contains("--no-slots"), "{reason}"),
            other => panic!("expected Blind, got {other:?}"),
        }
    }

    #[test]
    fn an_unreachable_upstream_blinds_the_readback() {
        let connections = Arc::new(ActiveConnectionsRegistry::new());
        let audit = SamplingAuditStore::new();

        audit_one_poll(
            &SlotsPollResult::Unreachable("connection refused".into()),
            "m",
            &connections,
            &audit,
        );

        match audit.state() {
            AuditState::Blind { reason } => {
                assert!(reason.contains("connection refused"), "{reason}");
            }
            other => panic!("expected Blind, got {other:?}"),
        }
    }

    /// Recovery has to be demonstrated, not assumed: the latch clears when a
    /// comparison actually happens, so a server that comes back but is never
    /// caught mid-turn keeps reporting blind.
    #[test]
    fn only_a_real_comparison_clears_the_blind_latch() {
        let connections = Arc::new(ActiveConnectionsRegistry::new());
        let audit = SamplingAuditStore::new();
        let guard = connections.register("m", true, None);
        guard.record_sampling(intent(0.7));

        audit_one_poll(
            &SlotsPollResult::Unreachable("gone".into()),
            "m",
            &connections,
            &audit,
        );
        assert!(matches!(audit.state(), AuditState::Blind { .. }));

        // Reachable again, but every slot idle — nothing was compared.
        audit_one_poll(&slots(IDLE), "m", &connections, &audit);
        assert!(
            matches!(audit.state(), AuditState::Blind { .. }),
            "a reachable server that was never caught mid-turn has proved nothing"
        );

        audit_one_poll(&slots(BUSY_AT_TEMP_0_7), "m", &connections, &audit);
        assert!(audit.state().is_observing());
    }

    /// The finished-request property, end to end through the poller path.
    #[test]
    fn a_completed_request_stops_being_compared_against() {
        let connections = Arc::new(ActiveConnectionsRegistry::new());
        let audit = SamplingAuditStore::new();
        let guard = connections.register("m", true, None);
        guard.record_sampling(intent(0.7));
        drop(guard);

        audit_one_poll(&slots(BUSY_AT_TEMP_0_7), "m", &connections, &audit);

        assert_eq!(audit.state(), AuditState::NotYetObserved);
    }

    #[test]
    fn busy_slot_params_ignores_idle_slots_and_missing_params() {
        let mixed = slots(
            r#"[
                {"id": 0, "is_processing": true, "params": {"temperature": 0.7}},
                {"id": 1, "is_processing": false, "params": {"temperature": 0.9}},
                {"id": 2, "is_processing": true}
            ]"#,
        );
        let params = busy_slot_params(&mixed);
        assert_eq!(params.len(), 1, "{params:?}");
        assert_eq!(params[0].temperature, Some(0.7));
    }

    /// A `ModelRuntimePort` that always reports no model running, so the
    /// poller never makes an HTTP call and there is nothing to mock.
    #[derive(Debug)]
    struct NoModelRunning;

    #[async_trait]
    impl ModelRuntimePort for NoModelRunning {
        async fn admit(
            &self,
            model: &str,
            _num_ctx: Option<u64>,
            _default_ctx: u64,
            _overrides: gglib_core::ports::LaunchOverrides,
        ) -> Result<gglib_core::ports::Admission, ModelRuntimeError> {
            Err(ModelRuntimeError::ModelNotFound(model.to_string()))
        }
        async fn current_model(&self) -> Option<RunningTarget> {
            None
        }
        async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn poller_shuts_down_promptly_on_cancellation() {
        let cancel = CancellationToken::new();
        let cache = Arc::new(SlotsCache::new());
        let handle = spawn_slots_poller(
            Arc::new(NoModelRunning),
            Client::new(),
            cache,
            Arc::new(ActiveConnectionsRegistry::new()),
            Arc::new(SamplingAuditStore::new()),
            cancel.clone(),
        );

        cancel.cancel();

        // The poller must return promptly rather than leaking or hanging;
        // give it a generous but bounded window well under the base poll
        // interval so a regression (e.g. missing cancellation check) fails
        // fast instead of hanging the test suite.
        tokio::time::timeout(Duration::from_millis(500), handle)
            .await
            .expect("poller task did not shut down promptly after cancellation")
            .expect("poller task panicked");
    }
}
