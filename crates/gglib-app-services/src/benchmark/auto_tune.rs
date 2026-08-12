//! The idle-time auto-tune scheduler: the closed loop's system-pressed
//! button.
//!
//! Runs inside the daemon — the one process that owns llama-server — and
//! does nothing unless `Settings.auto_tune` is deliberately on. When the GPU
//! has been fully idle for a sustained window it picks one untuned model,
//! runs the bounded presets-versus-incumbent tune, and applies the winner
//! **only through the apply gate** (`tune::apply`). Every rule below exists
//! to make the autonomy boring:
//!
//! - **Opt-in.** `auto_tune` defaults to off; the loop re-reads it every
//!   tick, so switching it off takes effect within one tick and cancels
//!   nothing that is not running.
//! - **Idle means idle.** Zero in-flight requests *and* zero waiters,
//!   sustained for [`IDLE_TICKS_REQUIRED`] consecutive ticks — a burst that
//!   ends 9 minutes into the window resets it.
//! - **A warm model is never evicted.** If the resident model is itself
//!   untuned, it is tuned in place (no swap, no cache loss). If a *tuned*
//!   model is resident, the scheduler stands down entirely: evicting a warm
//!   model invalidates its KV cache and hands the next real request a cold
//!   prefill, which is a price an idle-time nicety may not charge. Only a
//!   fully cold GPU tunes from the catalog at large.
//! - **A person's work is never the target.** User-set defaults are
//!   excluded outright — the same principle that ranks `Measured` below
//!   global settings — and measured models are excluded from the idle
//!   queue because re-checks belong to the signal triggers: a production
//!   defect rate past its threshold (a loop-trip rate → a DRY sweep; a
//!   tool-call repair rate or a mid-stream failure rate → a temperature
//!   sweep) is the one thing that reopens a measured recipe, not a timer.
//! - **Any real request preempts.** While a run is in flight the scheduler
//!   watches the admission queue; the moment anything is waiting, the run
//!   is cancelled and the GPU handed over. The next attempt starts from a
//!   fresh idle window.
//! - **One attempt per model per [`RETUNE_INTERVAL`].** A refusal
//!   (IncumbentStands, WithinDrift, …) is an answer, and answers do not
//!   expire in an afternoon; without this the scheduler would re-ask every
//!   idle window forever.
//! - **Evidence survives restarts.** Every tick persists the per-model
//!   *unacted* defect windows (even while `auto_tune` is off, so enabling
//!   it later starts from real history); boot restores them decayed by
//!   wall-clock age ([`DEFECT_HALF_LIFE_DAYS`]) and discards them across a
//!   llama.cpp release change. A window is zeroed in the store the moment
//!   a signal spends it, so a crash mid-run cannot resurrect it.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use gglib_core::domain::benchmark::run::{BenchmarkRun, BenchmarkRunStatus};
use gglib_core::domain::benchmark::tune::config::{ScoreWeights, SweepSpec, TuneConfig};
use gglib_core::domain::benchmark::tune::task::TaskSuite;
use gglib_core::domain::benchmark::{BenchmarkEvent, BenchmarkRunType};
use gglib_core::domain::defects::{self, ModelDefectCounts, PersistedDefectWindow};
use gglib_core::domain::{DefaultsOrigin, Model};
use gglib_runtime::llama::effective_llama_release;

use super::{BenchmarkDeps, BenchmarkOps};

/// How often the scheduler wakes to look at the world, in seconds.
/// `GGLIB_AUTO_TUNE_TICK_SECS` overrides it — an operator knob, and what
/// makes the end-to-end path testable in seconds rather than tens of
/// minutes.
const TICK_SECS: u64 = 60;

/// Consecutive fully-idle ticks required before a run may start — 10 minutes
/// at the default tick. Long enough that a person pausing between requests
/// is not mistaken for an empty evening. `GGLIB_AUTO_TUNE_IDLE_TICKS`
/// overrides it.
const IDLE_TICKS_REQUIRED: u32 = 10;

/// The tick interval, after any environment override.
fn tick_interval() -> Duration {
    Duration::from_secs(env_override("GGLIB_AUTO_TUNE_TICK_SECS", TICK_SECS))
}

/// The idle-tick threshold, after any environment override.
fn idle_ticks_required() -> u32 {
    env_override("GGLIB_AUTO_TUNE_IDLE_TICKS", IDLE_TICKS_REQUIRED)
}

fn env_override<T: std::str::FromStr + Copy>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// How often the preemption watcher checks the queue while a run is live.
const PREEMPT_POLL: Duration = Duration::from_secs(2);

/// The least time between *answered* tune attempts on one model.
/// A refusal is an answer; the next question should wait for new evidence
/// (a different build, new tasks) or the signal triggers. An aborted or
/// crashed run is **not** an answer — a preempted idle tune, a daemon
/// stopped mid-run, a launch failure — and must not park the model for a
/// week (the first live idle tune was aborted by a daemon stop and parked
/// its model seven days for zero information). Only completed runs
/// consume the interval; the accepted trade is that a deterministically
/// crashing tune retries each idle window, bounded by the idle threshold
/// and visible in the activity feed.
const RETUNE_INTERVAL: chrono::Duration = chrono::Duration::days(7);

/// The fewest windowed requests before a defect rate means anything.
///
/// A rate over a handful of requests is a coin flip wearing a percentage;
/// fifty is enough that a 5% threshold needs three real events, not one
/// unlucky turn.
const MIN_SIGNAL_SAMPLE: u64 = 50;

/// The loop-guard trip rate that earns a model a signal-driven DRY sweep.
///
/// The ceiling experiment measured healthy traffic tripping at roughly 2%
/// per task under deliberately hot sampling; a sustained 5% of *production*
/// requests is qualitatively different traffic, and DRY is the dimension
/// built for exactly that failure shape.
const LOOP_TRIP_RATE_THRESHOLD: f64 = 0.05;

/// The tool-call repair attempt rate that earns a model a signal-driven
/// temperature sweep.
///
/// A repair attempt means the model emitted a tool call that failed schema
/// validation and had to be re-issued with `tool_choice: "required"` — the
/// defect being diagnosed is the malformed call, not whether the repair
/// then succeeded. The observed rate *undercounts*: attempts are flagged
/// on the streaming path only, and the per-model ledger write is skipped
/// when the request's snapshot has already left the metrics ring buffer
/// (`ContextMetricsStore::flag_tool_repair`), so busy traffic loses
/// events while `requests` counts everything. The bias is fail-safe — a
/// window that crosses this threshold understates the true rate, so the
/// signal fires late, never spuriously.
const REPAIR_RATE_THRESHOLD: f64 = 0.05;

/// The upstream mid-stream failure rate that earns a model a signal-driven
/// temperature sweep.
///
/// A stream error is the failure a person actually feels: the model server
/// killed the turn mid-generation (its native tool-call grammar rejecting
/// the model's own output, a crash, a severed stream) and the client's
/// request simply failed. It is also the counter that sees "dead" where
/// the loop and repair counters only see "sick" — a model too broken to
/// even attempt a tool call raises neither of those, but it raises this.
/// Same sample floor and threshold as its siblings; under real breakdown
/// the rate approaches 100%, so 5% fires almost immediately.
const STREAM_ERROR_RATE_THRESHOLD: f64 = 0.05;

/// How many recent runs the target selector scans for the interval rule.
/// Generous against the interval: even a daemon tuning one model per idle
/// window cannot produce 200 tune runs in seven days.
const RUN_SCAN_LIMIT: i64 = 200;

/// Half-life of persisted defect evidence, in days.
/// `GGLIB_DEFECT_HALF_LIFE_DAYS` overrides it (values ≤ 0 fall back here).
///
/// Decay is what makes persistence honest: yesterday's rate must not
/// answer today's question at full weight. Rates themselves are
/// decay-invariant (numerator and denominator scale together), so the
/// half-life really governs how long old traffic keeps counting toward
/// [`MIN_SIGNAL_SAMPLE`] — seven days mirrors [`RETUNE_INTERVAL`]: the
/// horizon on which this scheduler already treats answers as current.
const DEFECT_HALF_LIFE_DAYS: f64 = 7.0;

/// The half-life in seconds, after any environment override.
fn defect_half_life_secs() -> f64 {
    let days = env_override("GGLIB_DEFECT_HALF_LIFE_DAYS", DEFECT_HALF_LIFE_DAYS);
    let days = if days > 0.0 {
        days
    } else {
        DEFECT_HALF_LIFE_DAYS
    };
    days * 86_400.0
}

/// The scheduler's working memory, owned by [`run_loop`] for the daemon's
/// lifetime.
#[derive(Default)]
struct SchedulerState {
    /// Consecutive fully-idle ticks banked toward the threshold.
    idle_ticks: u32,
    /// Per-model baselines for the defect windows. The scheduler rates the
    /// delta since its own last look, so acting on a signal advances the
    /// baseline and the same events can never fire twice.
    baselines: std::collections::HashMap<String, ModelDefectCounts>,
    /// Per-model windows as last persisted. Flushing only what changed is
    /// also what preserves a stored row's `updated_at` across idle
    /// restarts, so decay keeps counting from the evidence's real age.
    last_flushed: std::collections::HashMap<String, ModelDefectCounts>,
}

/// Drive the scheduler until the daemon shuts down.
pub async fn run_loop(ops: Arc<BenchmarkOps>, shutdown: CancellationToken) {
    let mut state = SchedulerState::default();
    restore(&ops, &mut state).await;
    loop {
        tokio::select! {
            () = shutdown.cancelled() => {
                // One last flush of whatever the ticks have not written yet
                // — a single local SQLite write, well inside the daemon's
                // shutdown watchdog. The scheduler owns its state, so the
                // scheduler flushes it; shutdown needs no hook.
                flush_now(&ops, &mut state).await;
                return;
            }
            () = tokio::time::sleep(tick_interval()) => {}
        }
        match tick(&ops, &mut state, &shutdown).await {
            Ok(()) => {}
            Err(e) => {
                // The loop must survive anything a tick can throw — a failed
                // settings read tonight must not cost the tuning the daemon
                // would have done tomorrow.
                warn!("auto-tune: tick failed: {e}");
                state.idle_ticks = 0;
            }
        }
    }
}

/// Restore the persisted unacted windows into the live ledger, decayed by
/// their age and discarded across a llama.cpp release change.
///
/// Baselines stay empty on purpose: spent evidence was subtracted before it
/// was ever persisted, so the seeded counts *are* the resumed window and an
/// empty baseline windows all of them. Best-effort throughout — persistence
/// must never cost the scheduler its tuning.
async fn restore(ops: &BenchmarkOps, state: &mut SchedulerState) {
    let rows = match ops.deps.defect_windows.load_all().await {
        Ok(rows) => rows,
        Err(e) => {
            warn!("auto-tune: could not load persisted defect windows: {e}");
            return;
        }
    };
    if rows.is_empty() {
        return;
    }

    let plan = restore_plan(
        rows,
        Utc::now(),
        &effective_llama_release(),
        defect_half_life_secs(),
    );
    for (model, counts) in &plan.seed {
        ops.deps.defects.seed(model, *counts);
        // The store still holds the undecayed row with its original stamp;
        // remembering the decayed value as "last flushed" leaves that row
        // untouched until real traffic changes the window, so consecutive
        // idle restarts keep decaying from the original age.
        state.last_flushed.insert(model.clone(), *counts);
        info!(
            model = %model,
            requests = counts.requests,
            "auto-tune: restored a persisted defect window (decayed)"
        );
    }
    if !plan.discard.is_empty() {
        info!(
            discarded = plan.discard.len(),
            "auto-tune: discarded stale defect windows (foreign build or decayed to nothing)"
        );
        if let Err(e) = ops.deps.defect_windows.delete(&plan.discard).await {
            warn!("auto-tune: could not delete stale defect windows: {e}");
        }
    }
}

/// Persist whatever the current windows say, skipping unchanged rows.
async fn flush_now(ops: &BenchmarkOps, state: &mut SchedulerState) {
    let ledger = ops.deps.defects.snapshot();
    let windows = windowed(&ledger, &state.baselines);
    flush_changed(&ops.deps, &windows, &mut state.last_flushed).await;
}

/// Upsert the windows that differ from what was last persisted; on success,
/// remember them. Warn-only: a failed flush costs at most one tick of
/// evidence, never the tick itself.
async fn flush_changed(
    deps: &BenchmarkDeps,
    windows: &std::collections::HashMap<String, ModelDefectCounts>,
    last_flushed: &mut std::collections::HashMap<String, ModelDefectCounts>,
) {
    let rows = rows_to_flush(
        windows,
        last_flushed,
        Utc::now(),
        &effective_llama_release(),
    );
    if rows.is_empty() {
        return;
    }
    match deps.defect_windows.upsert_many(&rows).await {
        Ok(()) => {
            for row in rows {
                last_flushed.insert(row.model_name, row.counts);
            }
        }
        Err(e) => warn!("auto-tune: could not persist defect windows: {e}"),
    }
}

/// One observation of the world, and at most one run.
async fn tick(
    ops: &BenchmarkOps,
    state: &mut SchedulerState,
    shutdown: &CancellationToken,
) -> anyhow::Result<()> {
    let deps = &ops.deps;

    // Persist first, before anything can fail or stand the tick down:
    // evidence keeps accumulating (and surviving restarts) even while
    // `auto_tune` is off, so enabling the feature later starts from real
    // history rather than from zero.
    let ledger = deps.defects.snapshot();
    let windows = windowed(&ledger, &state.baselines);
    flush_changed(deps, &windows, &mut state.last_flushed).await;

    let settings = deps.settings_repo.load().await?;
    if settings.auto_tune != Some(true) {
        state.idle_ticks = 0;
        return Ok(());
    }

    // A run somebody started — through any surface — owns the GPU story.
    let runs = deps.bench_repo.list_runs(RUN_SCAN_LIMIT, 0).await?;
    if runs.iter().any(|r| r.status == BenchmarkRunStatus::Running) {
        debug!("auto-tune: a benchmark run is live — standing down");
        state.idle_ticks = 0;
        return Ok(());
    }

    let snapshot = deps.runtime.admission_snapshot();
    if snapshot.inflight() > 0 || snapshot.waiting() > 0 {
        debug!(
            inflight = snapshot.inflight(),
            waiting = snapshot.waiting(),
            "auto-tune: the GPU is busy — idle window reset"
        );
        state.idle_ticks = 0;
        return Ok(());
    }

    state.idle_ticks += 1;
    let required = idle_ticks_required();
    debug!(
        idle_ticks = state.idle_ticks,
        required, "auto-tune: idle tick banked"
    );
    if state.idle_ticks < required {
        return Ok(());
    }

    let models = deps.model_repo.list().await?;
    let resident: Vec<i64> = snapshot
        .slots
        .iter()
        .map(|s| i64::from(s.model_id))
        .collect();

    // Signals outrank the untuned queue: a model demonstrably failing in
    // production is a sharper reason to spend the GPU than one that has
    // simply never been measured. (The ledger snapshot and windows were
    // taken at the top of the tick, on the way through the flush.)
    if let Some((target, signal)) = select_signal_target(&models, &windows, &resident) {
        info!(
            model_id = target.id,
            model = %target.name,
            signal = %signal,
            "auto-tune: a production defect rate crossed its threshold — starting a targeted sweep"
        );
        // Advance the baseline before running, so the events that earned
        // this sweep can never earn a second one.
        let current = ledger.get(&target.name).copied().unwrap_or_default();
        let spent_model = target.name.clone();
        state.baselines.insert(spent_model.clone(), current);
        // Zero the spent window in the store *before* the (long) run: a
        // crash mid-run must not leave the loud pre-signal row behind to
        // re-seed and re-fire on the next boot.
        let spent = std::iter::once((spent_model, ModelDefectCounts::default())).collect();
        flush_changed(deps, &spent, &mut state.last_flushed).await;
        let id = target.id;
        state.idle_ticks = 0;
        return run_one(ops, id, signal.sweep(), signal.initiator(), shutdown).await;
    }

    let Some(target) = select_target(&models, &runs, &resident, Utc::now()) else {
        debug!("auto-tune: idle, but nothing eligible to tune");
        // Idle stays banked: eligibility can change (a new model lands, the
        // interval lapses) without the GPU having been touched in between.
        return Ok(());
    };

    info!(
        model_id = target,
        "auto-tune: GPU idle past the threshold — starting a gated tune"
    );
    state.idle_ticks = 0;
    run_one(ops, target, SweepSpec::default(), "idle", shutdown).await
}

/// The counts accumulated since the scheduler's last acted-on baseline.
fn windowed(
    current: &std::collections::HashMap<String, ModelDefectCounts>,
    baselines: &std::collections::HashMap<String, ModelDefectCounts>,
) -> std::collections::HashMap<String, ModelDefectCounts> {
    current
        .iter()
        .map(|(name, counts)| {
            let baseline = baselines.get(name).copied().unwrap_or_default();
            (name.clone(), defects::delta(*counts, baseline))
        })
        .collect()
}

/// The exponential decay a gap of `gap_secs` applies: `0.5^(gap/half_life)`.
/// A non-positive gap (clock skew, a future stamp) decays nothing — the
/// evidence is at worst current, never amplified.
fn decay_factor(gap_secs: f64, half_life_secs: f64) -> f64 {
    if gap_secs <= 0.0 {
        return 1.0;
    }
    0.5_f64.powf(gap_secs / half_life_secs)
}

/// Each count scaled and rounded independently. A window is a bundle of
/// counts, not a ratio — per-field rounding noise of ±0.5 events is
/// accepted, and documented here so nobody "fixes" it into a scheme that
/// preserves ratios by inventing fractional events.
fn decay_counts(counts: ModelDefectCounts, factor: f64) -> ModelDefectCounts {
    let scale = |c: u64| -> u64 {
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        {
            ((c as f64) * factor).round() as u64
        }
    };
    ModelDefectCounts {
        requests: scale(counts.requests),
        loop_guard_trips: scale(counts.loop_guard_trips),
        repairs_attempted: scale(counts.repairs_attempted),
        repairs_succeeded: scale(counts.repairs_succeeded),
        stream_errors: scale(counts.stream_errors),
    }
}

/// What boot does with the persisted rows: seed these, delete those.
struct RestorePlan {
    /// Rows to seed into the ledger, already decayed.
    seed: Vec<(String, ModelDefectCounts)>,
    /// Rows to delete: foreign-build evidence and windows decayed to
    /// nothing.
    discard: Vec<String>,
}

/// Sort persisted rows into seeds and discards.
///
/// A row from another llama.cpp release is discarded outright — another
/// build's rate must not answer this build's question at any weight. A
/// surviving row decays by its age; one whose decayed request count rounds
/// below a single request has nothing left to say and is deleted rather
/// than carried forever.
fn restore_plan(
    rows: Vec<PersistedDefectWindow>,
    now: DateTime<Utc>,
    llama_build: &str,
    half_life_secs: f64,
) -> RestorePlan {
    let mut plan = RestorePlan {
        seed: Vec::new(),
        discard: Vec::new(),
    };
    for row in rows {
        if row.llama_build != llama_build {
            plan.discard.push(row.model_name);
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let gap_secs = (now - row.updated_at).num_seconds() as f64;
        let decayed = decay_counts(row.counts, decay_factor(gap_secs, half_life_secs));
        if decayed.requests == 0 {
            plan.discard.push(row.model_name);
            continue;
        }
        plan.seed.push((row.model_name, decayed));
    }
    plan
}

/// The rows one flush writes: every window that differs from what was last
/// persisted, stamped with now and the current release. Skipping unchanged
/// rows is not just economy — it preserves a stored row's `updated_at`, so
/// evidence that sits untouched keeps decaying from its real age instead
/// of being re-dated by every tick.
fn rows_to_flush(
    windows: &std::collections::HashMap<String, ModelDefectCounts>,
    last_flushed: &std::collections::HashMap<String, ModelDefectCounts>,
    now: DateTime<Utc>,
    llama_build: &str,
) -> Vec<PersistedDefectWindow> {
    windows
        .iter()
        .filter(|(name, counts)| last_flushed.get(*name) != Some(counts))
        .map(|(name, counts)| PersistedDefectWindow {
            model_name: name.clone(),
            counts: *counts,
            updated_at: now,
            llama_build: llama_build.to_owned(),
        })
        .collect()
}

/// Run one gated tune, preempting on any queue activity.
async fn run_one(
    ops: &BenchmarkOps,
    model_id: i64,
    sweep: SweepSpec,
    initiator: &str,
    shutdown: &CancellationToken,
) -> anyhow::Result<()> {
    let config = TuneConfig {
        model_id,
        task_suite: TaskSuite::Default,
        // The untuned path sweeps nothing — it asks only whether the known
        // candidate recipes beat the incumbent. A signal-driven run sweeps
        // the one dimension its signal names, and nothing else: the sweep
        // is the treatment for a diagnosed failure shape, not a search.
        sweep,
        seed_from_gguf: true,
        seed_from_family_presets: true,
        weights: ScoreWeights::default(),
        // Nothing is pruned: the candidate set is a handful, and only a
        // full-suite candidate can win the gate.
        prune_fraction: 0.0,
        ctx_size: None,
        initiator: Some(initiator.to_owned()),
    };

    let cancel = CancellationToken::new();
    let (tx, mut rx) = mpsc::channel::<BenchmarkEvent>(256);

    let run_handle = {
        let ops = ops.clone_for_task();
        let cancel = cancel.clone();
        tokio::spawn(async move { ops.run_tune(config, tx, cancel).await })
    };

    // Consume events (capturing the run id) while watching for rivals. The
    // watcher cancels rather than pauses: a half-yielded GPU serves nobody,
    // and the next idle window restarts cleanly.
    let mut completed_run: Option<i64> = None;
    let mut preempted = false;
    loop {
        tokio::select! {
            event = rx.recv() => match event {
                Some(BenchmarkEvent::RunComplete { run_id }) => completed_run = Some(run_id),
                Some(BenchmarkEvent::RunFailed { error }) => {
                    info!("auto-tune: run failed: {error}");
                }
                Some(_) => {}
                None => break,
            },
            () = tokio::time::sleep(PREEMPT_POLL) => {
                if ops.deps.runtime.admission_snapshot().waiting() > 0 {
                    info!("auto-tune: a request is waiting — preempting the run");
                    preempted = true;
                    cancel.cancel();
                }
            }
            () = shutdown.cancelled() => {
                cancel.cancel();
            }
        }
    }
    run_handle.await??;

    if preempted {
        return Ok(());
    }
    let Some(run_id) = completed_run else {
        return Ok(());
    };

    let outcome = ops.apply_tune_run(run_id).await?;
    info!(
        run_id,
        model_id = outcome.model_id,
        applied = outcome.applied,
        "auto-tune: {}",
        summarize(&outcome.verdict)
    );
    Ok(())
}

/// One log line per verdict — the unattended twin of the CLI's rendering.
fn summarize(verdict: &gglib_core::domain::benchmark::tune::apply::ApplyVerdict) -> String {
    use gglib_core::domain::benchmark::tune::apply::ApplyVerdict as V;
    match verdict {
        V::Apply { margin, drift, .. } => {
            format!("applied as measured defaults (margin {margin:+.3} against drift {drift:.3})")
        }
        V::IncumbentStands { incumbent_mean } => {
            format!("incumbent stands at {incumbent_mean:.3} — nothing applied")
        }
        V::WithinDrift { margin, drift } => {
            format!("not applied: margin {margin:+.3} inside drift {drift:.3}")
        }
        V::PairedDisagrees { wins, losses } => {
            format!("not applied: pairs disagree ({wins}W-{losses}L)")
        }
        V::Uncalibrated => "not applied: run carried no calibration pair".to_owned(),
        V::Contaminated { unmeasured_runs } => {
            format!("not applied: {unmeasured_runs} run(s) never reached the model")
        }
    }
}

/// A production failure shape the counters can diagnose, and the one
/// dimension that treats it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    /// The loop/stagnation guard is rejecting this model's traffic —
    /// verbatim-sequence repetition, which is the failure DRY exists for.
    LoopGuard,
    /// The tool-call repair loop keeps re-issuing this model's calls —
    /// schema non-conformance from an unconstrained `tool_choice: "auto"`
    /// path, which is the failure shape a colder temperature treats. See
    /// [`REPAIR_RATE_THRESHOLD`] for why the measured rate understates
    /// the true one.
    RepairRate,
    /// This model's turns keep dying on upstream mid-stream failures —
    /// output so far outside the expected shape that the model server
    /// kills the stream rather than finish it. The catastrophic sibling
    /// of [`Self::RepairRate`], treated with the same dimension.
    StreamError,
}

impl SignalKind {
    /// The initiator slug this signal stamps on its runs, for the
    /// activity surfaces.
    #[must_use]
    pub const fn initiator(self) -> &'static str {
        match self {
            Self::LoopGuard => "signal:loop-guard",
            Self::RepairRate => "signal:repair-rate",
            Self::StreamError => "signal:stream-error",
        }
    }

    /// The sweep this signal prescribes. One dimension only: the sweep is
    /// the treatment for a diagnosed failure shape, not a search, and the
    /// incumbent pair on every run keeps "change nothing" on the table —
    /// DRY includes its own off switch (0.0), temperature's baseline is
    /// whatever the incumbent resolves today.
    ///
    /// A temperature candidate is a whole recipe, not a delta: naming
    /// `temperature` claims the temperature-coupled set under
    /// `resolve_layers`, so its companions resolve to llama.cpp's neutral
    /// defaults rather than the incumbent's — which is exactly how an
    /// applied temperature-only `Measured` recipe resolves in production.
    /// What the sweep measures is what an apply would ship. Swept values
    /// are never ceiling-clamped: candidates enter the eval as the
    /// trusted top layer, and a stored winner's model rung is
    /// ceiling-exempt (#748).
    #[must_use]
    pub fn sweep(self) -> SweepSpec {
        match self {
            Self::LoopGuard => SweepSpec {
                dry_multiplier: vec![0.0, 0.4, 0.8],
                ..SweepSpec::default()
            },
            Self::RepairRate => SweepSpec {
                temperature: vec![0.2, 0.5, 0.8],
                ..SweepSpec::default()
            },
            // The same diagnosis family as RepairRate — output too hot to
            // hold its required shape — one step further along, so the
            // same treatment applies.
            Self::StreamError => SweepSpec {
                temperature: vec![0.2, 0.5, 0.8],
                ..SweepSpec::default()
            },
        }
    }
}

impl std::fmt::Display for SignalKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LoopGuard => write!(f, "loop-guard trip rate"),
            Self::RepairRate => write!(f, "tool-call repair rate"),
            Self::StreamError => write!(f, "mid-stream failure rate"),
        }
    }
}

/// The strongest signal one model's window raises, with its severity —
/// the rate as a multiple of its threshold, so signals with different
/// thresholds compare on how far past alarm they are rather than on raw
/// percentages. A dead heat resolves by the array's order — loop guard,
/// then stream errors, then repairs: the sharper failure first, and never
/// by iteration accident (`max_by` keeps the *last* maximum, so the fold
/// below keeps the first instead). The caller has already enforced
/// [`MIN_SIGNAL_SAMPLE`], so the denominator is never zero.
fn worst_signal(window: &ModelDefectCounts) -> Option<(SignalKind, f64)> {
    #[allow(clippy::cast_precision_loss)]
    let rate = |count: u64, threshold: f64| (count as f64 / window.requests as f64) / threshold;
    let severities = [
        (
            SignalKind::LoopGuard,
            rate(window.loop_guard_trips, LOOP_TRIP_RATE_THRESHOLD),
        ),
        (
            SignalKind::StreamError,
            rate(window.stream_errors, STREAM_ERROR_RATE_THRESHOLD),
        ),
        (
            SignalKind::RepairRate,
            rate(window.repairs_attempted, REPAIR_RATE_THRESHOLD),
        ),
    ];
    severities
        .into_iter()
        .filter(|&(_, severity)| severity >= 1.0)
        .fold(None, |best, candidate| match best {
            Some((_, best_severity)) if best_severity >= candidate.1 => best,
            _ => Some(candidate),
        })
}

/// The model whose production defect rate has earned a targeted sweep, with
/// the signal that earned it.
///
/// Signals deliberately bypass two of the untuned path's rules and honour
/// the others. Bypassed: the untuned-only filter (a measured model failing
/// in production is *exactly* the re-check case) and the retune interval (a
/// defect rate is new evidence, not the old question re-asked — and the
/// baseline advance makes one burst of events good for at most one sweep).
/// Honoured: a person's defaults are still never touched, and the
/// warm-model rule still applies — though in practice a model with traffic
/// enough to signal *is* the resident one, and is tuned in place.
///
/// Each model contributes its own worst signal ([`worst_signal`]), and
/// models compete on severity — the rate as a multiple of its threshold —
/// so a 12% repair rate outranks a 6% trip rate not because the
/// percentage is bigger but because it is further past its alarm.
fn select_signal_target<'m>(
    models: &'m [Model],
    windows: &std::collections::HashMap<String, ModelDefectCounts>,
    resident_ids: &[i64],
) -> Option<(&'m Model, SignalKind)> {
    let mut candidates: Vec<(&Model, SignalKind, f64)> = Vec::new();
    for model in models {
        if matches!(model.defaults_origin, Some(DefaultsOrigin::User)) {
            continue;
        }
        // The warm-model rule, unchanged: never evict, tune in place.
        if !resident_ids.is_empty() && !resident_ids.contains(&model.id) {
            continue;
        }
        let Some(window) = windows.get(&model.name) else {
            continue;
        };
        if window.requests < MIN_SIGNAL_SAMPLE {
            continue;
        }
        if let Some((signal, severity)) = worst_signal(window) {
            candidates.push((model, signal, severity));
        }
    }
    // The worst severity first: one sweep per idle window, spent where the
    // evidence is furthest past its threshold.
    candidates
        .into_iter()
        .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(model, signal, _)| (model, signal))
}

/// Choose the model an idle GPU should spend itself on, or `None`.
///
/// Pure, so the policy is testable without a daemon: eligibility (no
/// measured or user-set defaults), the warm-model rule, the retune
/// interval, and oldest-import-first ordering all live here.
fn select_target(
    models: &[Model],
    runs: &[BenchmarkRun],
    resident_ids: &[i64],
    now: DateTime<Utc>,
) -> Option<i64> {
    // Only an answered question consumes the interval: completed runs
    // (their verdict stands for a week) and runs still in flight (their
    // answer is coming). A failed row — preempted, aborted by a daemon
    // stop, crashed — produced no answer and holds no claim.
    let recently_tuned = |id: i64| {
        runs.iter().any(|r| {
            r.run_type == BenchmarkRunType::Tune
                && matches!(
                    r.status,
                    BenchmarkRunStatus::Complete | BenchmarkRunStatus::Running
                )
                && r.model_ids.contains(&id)
                && now.signed_duration_since(r.created_at) < RETUNE_INTERVAL
        })
    };
    let untuned = |m: &&Model| {
        !matches!(
            m.defaults_origin,
            Some(DefaultsOrigin::User | DefaultsOrigin::Measured)
        ) && !recently_tuned(m.id)
    };

    // A resident model that needs tuning is tuned in place — no eviction.
    if let Some(resident) = models
        .iter()
        .filter(|m| resident_ids.contains(&m.id))
        .find(untuned)
    {
        return Some(resident.id);
    }
    // A resident model that does *not* need tuning is left warm: its KV
    // cache is worth more than an idle-time nicety.
    if !resident_ids.is_empty() {
        return None;
    }

    models
        .iter()
        .filter(untuned)
        .min_by_key(|m| m.added_at)
        .map(|m| m.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: i64, origin: Option<DefaultsOrigin>, added_days_ago: i64) -> Model {
        Model {
            id,
            name: format!("m{id}"),
            model_key: format!("key{id}"),
            file_path: std::path::PathBuf::from("/dev/null"),
            param_count_b: 4.0,
            architecture: None,
            quantization: None,
            context_length: None,
            expert_count: None,
            expert_used_count: None,
            expert_shared_count: None,
            metadata: std::collections::HashMap::new(),
            added_at: Utc::now() - chrono::Duration::days(added_days_ago),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: Vec::new(),
            capabilities: gglib_core::domain::ModelCapabilities::default(),
            inference_defaults: None,
            defaults_origin: origin,
            server_defaults: None,
            dialect_spec: None,
            benchmark_summary: None,
        }
    }

    fn tune_run(model_id: i64, days_ago: i64) -> BenchmarkRun {
        BenchmarkRun {
            id: 1,
            run_type: BenchmarkRunType::Tune,
            status: BenchmarkRunStatus::Complete,
            model_ids: vec![model_id],
            prompt_text: None,
            system_prompt: None,
            config_json: None,
            applied_json: None,
            error: None,
            created_at: Utc::now() - chrono::Duration::days(days_ago),
            completed_at: None,
        }
    }

    fn window(requests: u64, trips: u64) -> ModelDefectCounts {
        ModelDefectCounts {
            requests,
            loop_guard_trips: trips,
            ..ModelDefectCounts::default()
        }
    }

    fn repair_window(requests: u64, attempts: u64) -> ModelDefectCounts {
        ModelDefectCounts {
            requests,
            repairs_attempted: attempts,
            ..ModelDefectCounts::default()
        }
    }

    fn windows_for(
        entries: &[(&str, ModelDefectCounts)],
    ) -> std::collections::HashMap<String, ModelDefectCounts> {
        entries
            .iter()
            .map(|(name, counts)| ((*name).to_owned(), *counts))
            .collect()
    }

    /// The headline signal: a sustained trip rate over enough traffic earns
    /// the sweep — and a measured model is eligible, because production
    /// failure is exactly the re-check case.
    #[test]
    fn a_loud_loop_rate_earns_a_measured_model_a_dry_sweep() {
        let models = vec![model(1, Some(DefaultsOrigin::Measured), 10)];
        // model() names them m{id}; 100 requests, 8 trips = 8%.
        let windows = windows_for(&[("m1", window(100, 8))]);
        let (target, signal) = select_signal_target(&models, &windows, &[]).expect("signal fires");
        assert_eq!(target.id, 1);
        assert_eq!(signal, SignalKind::LoopGuard);
        assert_eq!(signal.sweep().dry_multiplier, vec![0.0, 0.4, 0.8]);
    }

    /// The repair-rate twin of the headline: a sustained repair attempt
    /// rate over enough traffic earns the temperature sweep — one
    /// dimension, no DRY riding along.
    #[test]
    fn a_loud_repair_rate_earns_a_temperature_sweep() {
        let models = vec![model(1, Some(DefaultsOrigin::Measured), 10)];
        let windows = windows_for(&[("m1", repair_window(100, 8))]); // 8%
        let (target, signal) = select_signal_target(&models, &windows, &[]).expect("signal fires");
        assert_eq!(target.id, 1);
        assert_eq!(signal, SignalKind::RepairRate);
        assert_eq!(signal.initiator(), "signal:repair-rate");
        assert_eq!(signal.sweep().temperature, vec![0.2, 0.5, 0.8]);
        assert!(signal.sweep().dry_multiplier.is_empty());
    }

    /// The sample floor guards the repair rate exactly as it guards the
    /// trip rate: 50% of ten requests is still a coin flip.
    #[test]
    fn a_thin_repair_sample_never_signals() {
        let models = vec![model(1, Some(DefaultsOrigin::AutoDetected), 10)];
        let windows = windows_for(&[("m1", repair_window(10, 5))]);
        assert!(select_signal_target(&models, &windows, &[]).is_none());
    }

    /// An occasional repair is the mechanism working, not a defect rate
    /// worth spending the GPU on.
    #[test]
    fn a_quiet_repair_rate_never_signals() {
        let models = vec![model(1, Some(DefaultsOrigin::AutoDetected), 10)];
        let windows = windows_for(&[("m1", repair_window(200, 4))]); // 2%
        assert!(select_signal_target(&models, &windows, &[]).is_none());
    }

    /// Across models and kinds, severity — the rate over its threshold —
    /// decides: a 12% repair rate (2.4× alarm) outranks a 6% trip rate
    /// (1.2× alarm).
    #[test]
    fn the_louder_signal_wins_across_kinds() {
        let models = vec![
            model(1, Some(DefaultsOrigin::AutoDetected), 10),
            model(2, Some(DefaultsOrigin::AutoDetected), 5),
        ];
        let windows = windows_for(&[
            ("m1", window(100, 6)),         // trip severity 1.2
            ("m2", repair_window(100, 12)), // repair severity 2.4
        ]);
        let hit = select_signal_target(&models, &windows, &[]);
        assert_eq!(
            hit.map(|(m, s)| (m.id, s)),
            Some((2, SignalKind::RepairRate))
        );
    }

    /// One model raising both signals gets one sweep, for its worst one:
    /// the sweep is a treatment, and the sharper diagnosis names it.
    #[test]
    fn one_models_worst_defect_names_the_sweep() {
        let models = vec![model(1, Some(DefaultsOrigin::AutoDetected), 10)];
        let both = ModelDefectCounts {
            requests: 100,
            loop_guard_trips: 6,   // severity 1.2
            repairs_attempted: 15, // severity 3.0
            ..ModelDefectCounts::default()
        };
        let windows = windows_for(&[("m1", both)]);
        let hit = select_signal_target(&models, &windows, &[]);
        assert_eq!(hit.map(|(_, s)| s), Some(SignalKind::RepairRate));
    }

    /// A dead heat prefers the loop guard — deterministically, not by
    /// iteration accident.
    #[test]
    fn a_dead_heat_prefers_the_loop_guard() {
        let both = ModelDefectCounts {
            requests: 100,
            loop_guard_trips: 5,  // severity 1.0
            repairs_attempted: 5, // severity 1.0
            ..ModelDefectCounts::default()
        };
        assert_eq!(
            worst_signal(&both).map(|(s, _)| s),
            Some(SignalKind::LoopGuard)
        );
    }

    /// The catastrophic counter: turns dying mid-stream earn the same
    /// temperature treatment as malformed calls — and a measured model is
    /// eligible, because production failure is exactly the re-check case.
    #[test]
    fn a_dying_stream_earns_a_temperature_sweep() {
        let models = vec![model(1, Some(DefaultsOrigin::Measured), 10)];
        let dying = ModelDefectCounts {
            requests: 100,
            stream_errors: 40, // severity 8.0 — a model that is simply broken
            ..ModelDefectCounts::default()
        };
        let windows = windows_for(&[("m1", dying)]);
        let (target, signal) = select_signal_target(&models, &windows, &[]).expect("signal fires");
        assert_eq!(target.id, 1);
        assert_eq!(signal, SignalKind::StreamError);
        assert_eq!(signal.initiator(), "signal:stream-error");
        assert_eq!(signal.sweep().temperature, vec![0.2, 0.5, 0.8]);
    }

    /// Between the two temperature-treated kinds, a dead heat prefers the
    /// stream error: a turn that died outranks a turn that was repaired.
    #[test]
    fn a_dead_heat_prefers_death_over_repair() {
        let both = ModelDefectCounts {
            requests: 100,
            repairs_attempted: 5, // severity 1.0
            stream_errors: 5,     // severity 1.0
            ..ModelDefectCounts::default()
        };
        assert_eq!(
            worst_signal(&both).map(|(s, _)| s),
            Some(SignalKind::StreamError)
        );
    }

    /// A rate over a handful of requests is a coin flip wearing a
    /// percentage: below the sample floor nothing fires, however loud.
    #[test]
    fn a_thin_sample_never_signals() {
        let models = vec![model(1, Some(DefaultsOrigin::AutoDetected), 10)];
        let windows = windows_for(&[("m1", window(10, 5))]); // 50%!
        assert!(select_signal_target(&models, &windows, &[]).is_none());
    }

    /// Below the rate threshold, healthy-ish traffic is left alone.
    #[test]
    fn a_quiet_rate_never_signals() {
        let models = vec![model(1, Some(DefaultsOrigin::AutoDetected), 10)];
        let windows = windows_for(&[("m1", window(200, 4))]); // 2%
        assert!(select_signal_target(&models, &windows, &[]).is_none());
    }

    /// The two rules signals do NOT bypass: a person's defaults, and the
    /// warm model.
    #[test]
    fn signals_honour_the_person_and_the_warm_model() {
        let user = vec![model(1, Some(DefaultsOrigin::User), 10)];
        let windows = windows_for(&[("m1", window(100, 20))]);
        assert!(select_signal_target(&user, &windows, &[]).is_none());

        // Model 1 signals, but model 2 is resident: no eviction, no run.
        let models = vec![
            model(1, Some(DefaultsOrigin::AutoDetected), 10),
            model(2, Some(DefaultsOrigin::Measured), 5),
        ];
        assert!(select_signal_target(&models, &windows, &[2]).is_none());
        // The signalling model resident itself: tuned in place.
        let hit = select_signal_target(&models, &windows, &[1]);
        assert_eq!(hit.map(|(m, _)| m.id), Some(1));
    }

    /// One sweep per window, spent where the evidence is loudest.
    #[test]
    fn the_worst_rate_wins_the_window() {
        let models = vec![
            model(1, Some(DefaultsOrigin::AutoDetected), 10),
            model(2, Some(DefaultsOrigin::AutoDetected), 5),
        ];
        let windows = windows_for(&[
            ("m1", window(100, 6)),  // 6%
            ("m2", window(100, 12)), // 12%
        ]);
        let hit = select_signal_target(&models, &windows, &[]);
        assert_eq!(hit.map(|(m, _)| m.id), Some(2));
    }

    #[test]
    fn a_persons_work_and_a_measurement_are_never_targets() {
        let models = vec![
            model(1, Some(DefaultsOrigin::User), 10),
            model(2, Some(DefaultsOrigin::Measured), 10),
        ];
        assert_eq!(select_target(&models, &[], &[], Utc::now()), None);
    }

    #[test]
    fn the_oldest_untuned_model_goes_first_on_a_cold_gpu() {
        let models = vec![
            model(1, Some(DefaultsOrigin::AutoDetected), 3),
            model(2, None, 30),
            model(3, Some(DefaultsOrigin::Published), 10),
        ];
        assert_eq!(select_target(&models, &[], &[], Utc::now()), Some(2));
    }

    #[test]
    fn a_resident_untuned_model_is_tuned_in_place() {
        let models = vec![
            model(1, Some(DefaultsOrigin::AutoDetected), 30),
            model(2, Some(DefaultsOrigin::AutoDetected), 3),
        ];
        // Model 2 is warm; it wins despite being newer — no eviction.
        assert_eq!(select_target(&models, &[], &[2], Utc::now()), Some(2));
    }

    #[test]
    fn a_resident_tuned_model_stands_the_scheduler_down() {
        let models = vec![
            model(1, Some(DefaultsOrigin::AutoDetected), 30),
            model(2, Some(DefaultsOrigin::Measured), 3),
        ];
        // Model 2 is warm and already measured: nothing runs, because
        // tuning model 1 would evict a warm model's cache.
        assert_eq!(select_target(&models, &[], &[2], Utc::now()), None);
    }

    #[test]
    fn a_recent_tune_run_parks_the_model_for_the_interval() {
        let models = vec![model(1, Some(DefaultsOrigin::AutoDetected), 30)];
        let runs = vec![tune_run(1, 2)];
        assert_eq!(select_target(&models, &runs, &[], Utc::now()), None);

        let stale = vec![tune_run(1, 8)];
        assert_eq!(select_target(&models, &stale, &[], Utc::now()), Some(1));
    }

    /// An aborted run answered nothing and holds no claim: the model stays
    /// eligible. (The first live idle tune was aborted by a daemon stop and
    /// wrongly parked its model for a week — this is that bug's headstone.)
    #[test]
    fn an_aborted_run_does_not_park_the_model() {
        let models = vec![model(1, Some(DefaultsOrigin::AutoDetected), 30)];
        let aborted = vec![BenchmarkRun {
            status: BenchmarkRunStatus::Failed,
            ..tune_run(1, 0)
        }];
        assert_eq!(select_target(&models, &aborted, &[], Utc::now()), Some(1));
    }

    /// A run still in flight is a question being answered — it parks the
    /// model exactly like a completed one, so two surfaces cannot race the
    /// same question.
    #[test]
    fn a_running_tune_parks_the_model() {
        let models = vec![model(1, Some(DefaultsOrigin::AutoDetected), 30)];
        let running = vec![BenchmarkRun {
            status: BenchmarkRunStatus::Running,
            ..tune_run(1, 0)
        }];
        assert_eq!(select_target(&models, &running, &[], Utc::now()), None);
    }

    // ── Persistence: decay, restore plans, flush rows ────────────────────

    /// Seven days in seconds — the default half-life, spelled out so the
    /// decay tests read as durations rather than magic floats.
    const HL: f64 = 7.0 * 86_400.0;

    fn full_window() -> ModelDefectCounts {
        ModelDefectCounts {
            requests: 100,
            loop_guard_trips: 10,
            repairs_attempted: 8,
            repairs_succeeded: 4,
            stream_errors: 6,
        }
    }

    fn row(
        model: &str,
        counts: ModelDefectCounts,
        now: DateTime<Utc>,
        age_secs: i64,
        build: &str,
    ) -> PersistedDefectWindow {
        PersistedDefectWindow {
            model_name: model.to_owned(),
            counts,
            updated_at: now - chrono::Duration::seconds(age_secs),
            llama_build: build.to_owned(),
        }
    }

    #[test]
    fn a_zero_gap_decays_nothing() {
        assert!((decay_factor(0.0, HL) - 1.0).abs() < f64::EPSILON);
        assert_eq!(decay_counts(full_window(), 1.0), full_window());
    }

    #[test]
    fn one_half_life_halves_the_window() {
        let halved = decay_counts(full_window(), decay_factor(HL, HL));
        assert_eq!(
            halved,
            ModelDefectCounts {
                requests: 50,
                loop_guard_trips: 5,
                repairs_attempted: 4,
                repairs_succeeded: 2,
                stream_errors: 3,
            }
        );
    }

    #[test]
    fn a_huge_gap_decays_the_window_to_nothing() {
        let sixty_days = 60.0 * 86_400.0;
        let gone = decay_counts(full_window(), decay_factor(sixty_days, HL));
        assert_eq!(gone.requests, 0);
    }

    /// Clock skew must never amplify evidence: a stamp from the future is
    /// treated as current, not compounded.
    #[test]
    fn a_future_timestamp_decays_nothing() {
        assert!((decay_factor(-3600.0, HL) - 1.0).abs() < f64::EPSILON);
    }

    /// Another build's rate must not answer this build's question at any
    /// weight — however fresh the row.
    #[test]
    fn a_foreign_build_row_is_discarded() {
        let now = Utc::now();
        let rows = vec![row("m1", full_window(), now, 0, "b0001")];
        let plan = restore_plan(rows, now, "b10327", HL);
        assert!(plan.seed.is_empty());
        assert_eq!(plan.discard, vec!["m1".to_owned()]);
    }

    /// A window with nothing left to say is deleted, not carried forever.
    #[test]
    fn a_row_decayed_below_one_request_is_discarded() {
        let now = Utc::now();
        let thin = ModelDefectCounts {
            requests: 1,
            ..ModelDefectCounts::default()
        };
        #[allow(clippy::cast_possible_truncation)]
        let two_half_lives = (2.0 * HL) as i64;
        let rows = vec![row("m1", thin, now, two_half_lives, "b10327")];
        let plan = restore_plan(rows, now, "b10327", HL);
        assert!(plan.seed.is_empty());
        assert_eq!(plan.discard, vec!["m1".to_owned()]);
    }

    #[test]
    fn a_restored_row_seeds_decayed_counts() {
        let now = Utc::now();
        #[allow(clippy::cast_possible_truncation)]
        let one_half_life = HL as i64;
        let rows = vec![row("m1", full_window(), now, one_half_life, "b10327")];
        let plan = restore_plan(rows, now, "b10327", HL);
        assert!(plan.discard.is_empty());
        assert_eq!(plan.seed.len(), 1);
        assert_eq!(plan.seed[0].0, "m1");
        assert_eq!(plan.seed[0].1.requests, 50);
        assert_eq!(plan.seed[0].1.loop_guard_trips, 5);
    }

    /// Skipping unchanged rows preserves a stored row's `updated_at`, so
    /// untouched evidence keeps decaying from its real age.
    #[test]
    fn an_unchanged_window_is_not_rewritten() {
        let windows = windows_for(&[("m1", full_window())]);
        let rows = rows_to_flush(&windows, &windows, Utc::now(), "b10327");
        assert!(rows.is_empty());
    }

    #[test]
    fn a_changed_window_is_flushed_with_the_current_build() {
        let now = Utc::now();
        let windows = windows_for(&[("m1", full_window())]);
        let rows = rows_to_flush(&windows, &std::collections::HashMap::new(), now, "b10327");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model_name, "m1");
        assert_eq!(rows[0].counts, full_window());
        assert_eq!(rows[0].updated_at, now);
        assert_eq!(rows[0].llama_build, "b10327");
    }

    /// The whole restore contract, end to end in miniature: a persisted
    /// loud window seeds a fresh ledger, an empty baseline windows all of
    /// it, and the signal selector fires on the restored evidence.
    #[test]
    fn a_restart_resumes_the_window_with_empty_baselines() {
        use gglib_core::domain::defects::ModelDefectLedger;

        let now = Utc::now();
        let loud = ModelDefectCounts {
            requests: 100,
            repairs_attempted: 12,
            ..ModelDefectCounts::default()
        };
        let plan = restore_plan(vec![row("m1", loud, now, 0, "b10327")], now, "b10327", HL);

        let ledger = ModelDefectLedger::new();
        for (model, counts) in &plan.seed {
            ledger.seed(model, *counts);
        }
        let windows = windowed(&ledger.snapshot(), &std::collections::HashMap::new());
        assert_eq!(windows["m1"], loud);

        let models = vec![model(1, Some(DefaultsOrigin::Measured), 10)];
        let hit = select_signal_target(&models, &windows, &[]);
        assert_eq!(
            hit.map(|(m, s)| (m.id, s)),
            Some((1, SignalKind::RepairRate))
        );
    }
}
