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
//!   global settings — and measured models are excluded because re-checks
//!   are a deliberately deferred decision (PR 6's signal triggers), not a
//!   timer's.
//! - **Any real request preempts.** While a run is in flight the scheduler
//!   watches the admission queue; the moment anything is waiting, the run
//!   is cancelled and the GPU handed over. The next attempt starts from a
//!   fresh idle window.
//! - **One attempt per model per [`RETUNE_INTERVAL`].** A refusal
//!   (IncumbentStands, WithinDrift, …) is an answer, and answers do not
//!   expire in an afternoon; without this the scheduler would re-ask every
//!   idle window forever.

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
use gglib_core::domain::{DefaultsOrigin, Model};

use super::BenchmarkOps;

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

/// The least time between tune attempts on one model, however they ended.
/// A refusal is an answer; the next question should wait for new evidence
/// (a different build, new tasks) or PR 6's signal triggers.
const RETUNE_INTERVAL: chrono::Duration = chrono::Duration::days(7);

/// How many recent runs the target selector scans for the interval rule.
/// Generous against the interval: even a daemon tuning one model per idle
/// window cannot produce 200 tune runs in seven days.
const RUN_SCAN_LIMIT: i64 = 200;

/// Drive the scheduler until the daemon shuts down.
pub async fn run_loop(ops: Arc<BenchmarkOps>, shutdown: CancellationToken) {
    let mut idle_ticks: u32 = 0;
    loop {
        tokio::select! {
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(tick_interval()) => {}
        }
        match tick(&ops, &mut idle_ticks, &shutdown).await {
            Ok(()) => {}
            Err(e) => {
                // The loop must survive anything a tick can throw — a failed
                // settings read tonight must not cost the tuning the daemon
                // would have done tomorrow.
                warn!("auto-tune: tick failed: {e}");
                idle_ticks = 0;
            }
        }
    }
}

/// One observation of the world, and at most one run.
async fn tick(
    ops: &BenchmarkOps,
    idle_ticks: &mut u32,
    shutdown: &CancellationToken,
) -> anyhow::Result<()> {
    let deps = &ops.deps;

    let settings = deps.settings_repo.load().await?;
    if settings.auto_tune != Some(true) {
        *idle_ticks = 0;
        return Ok(());
    }

    // A run somebody started — through any surface — owns the GPU story.
    let runs = deps.bench_repo.list_runs(RUN_SCAN_LIMIT, 0).await?;
    if runs.iter().any(|r| r.status == BenchmarkRunStatus::Running) {
        debug!("auto-tune: a benchmark run is live — standing down");
        *idle_ticks = 0;
        return Ok(());
    }

    let snapshot = deps.runtime.admission_snapshot();
    if snapshot.inflight() > 0 || snapshot.waiting() > 0 {
        debug!(
            inflight = snapshot.inflight(),
            waiting = snapshot.waiting(),
            "auto-tune: the GPU is busy — idle window reset"
        );
        *idle_ticks = 0;
        return Ok(());
    }

    *idle_ticks += 1;
    let required = idle_ticks_required();
    debug!(
        idle_ticks = *idle_ticks,
        required, "auto-tune: idle tick banked"
    );
    if *idle_ticks < required {
        return Ok(());
    }

    let models = deps.model_repo.list().await?;
    let resident: Vec<i64> = snapshot
        .slots
        .iter()
        .map(|s| i64::from(s.model_id))
        .collect();
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
    *idle_ticks = 0;
    run_one(ops, target, shutdown).await
}

/// Run one gated tune, preempting on any queue activity.
async fn run_one(
    ops: &BenchmarkOps,
    model_id: i64,
    shutdown: &CancellationToken,
) -> anyhow::Result<()> {
    let config = TuneConfig {
        model_id,
        task_suite: TaskSuite::Default,
        // No swept dimensions: the run asks only whether the known candidate
        // recipes — GGUF author defaults, family presets — beat the
        // incumbent, judged by the calibration pair. Bounded, predictable,
        // and exactly the question an unattended tuner is entitled to ask.
        // Signal-driven dimension picks are PR 6's job.
        sweep: SweepSpec::default(),
        seed_from_gguf: true,
        seed_from_family_presets: true,
        weights: ScoreWeights::default(),
        // Nothing is pruned: the candidate set is a handful, and only a
        // full-suite candidate can win the gate.
        prune_fraction: 0.0,
        ctx_size: None,
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
    let recently_tuned = |id: i64| {
        runs.iter().any(|r| {
            r.run_type == BenchmarkRunType::Tune
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
}
