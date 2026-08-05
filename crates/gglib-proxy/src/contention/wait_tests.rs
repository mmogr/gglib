//! Contention-wait tests.
//!
//! A scripted runtime double returns a canned sequence of outcomes, so the
//! interesting property — *which* errors are waited on and for how long — is
//! asserted without launching a model.
//!
//! Nothing here sleeps and nothing here rolls dice. Both impurities are
//! supplied by the test:
//!
//! * **Randomness** — [`fixed`] pins the jitter draw, so every delay below is
//!   an exact number rather than a range. The backoff arithmetic producing
//!   those numbers is proven separately in `gglib_core::retry`.
//! * **Time** — `#[tokio::test(start_paused = true)]` runs the whole window on
//!   a virtual clock, which auto-advances whenever the runtime goes idle. The
//!   elapsed assertions are therefore equalities, not margins, and a wait that
//!   would take half a second of wall clock takes none.
//!
//! `dashboard.rs` documents a case where `start_paused` was rejected as flaky:
//! `advance` fires a due timer but does not guarantee the woken *other* task is
//! polled before the test's own future. That is a cross-task hazard and does
//! not apply here — `ensure_with` is awaited directly in the test body, so
//! there is only ever one task with work to do.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use gglib_core::ports::ModelRuntimePort;

use super::*;

/// Returns a scripted outcome per call; the last one repeats.
struct ScriptedRuntime {
    script: Mutex<Vec<Result<(), ModelRuntimeError>>>,
    calls: AtomicUsize,
}

impl ScriptedRuntime {
    fn with_script(script: Vec<Result<(), ModelRuntimeError>>) -> Arc<dyn ModelRuntimePort> {
        Arc::new(Self {
            script: Mutex::new(script),
            calls: AtomicUsize::new(0),
        })
    }

    /// Always contended, so the window is what ends the wait.
    fn always_contended() -> Arc<dyn ModelRuntimePort> {
        Self::with_script(vec![Err(contended())])
    }
}

impl std::fmt::Debug for ScriptedRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScriptedRuntime")
    }
}

#[async_trait]
impl ModelRuntimePort for ScriptedRuntime {
    async fn ensure_model_running(
        &self,
        model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: u64,
    ) -> Result<RunningTarget, ModelRuntimeError> {
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        let script = self.script.lock().expect("script mutex");
        let outcome = script
            .get(index)
            .or_else(|| script.last())
            .expect("script must not be empty");

        match outcome {
            Ok(()) => Ok(RunningTarget::local(
                1,
                1,
                model_name.to_owned(),
                4096,
                false,
            )),
            Err(e) => Err(clone_error(e)),
        }
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }
}

/// `ModelRuntimeError` is not `Clone`; rebuild the variants the script uses.
fn clone_error(err: &ModelRuntimeError) -> ModelRuntimeError {
    match err {
        ModelRuntimeError::ContentionTimeout(m) => ModelRuntimeError::ContentionTimeout(m.clone()),
        ModelRuntimeError::ModelLoading => ModelRuntimeError::ModelLoading,
        other => ModelRuntimeError::Internal(other.to_string()),
    }
}

fn contended() -> ModelRuntimeError {
    ModelRuntimeError::ContentionTimeout("slot held".to_owned())
}

/// A jitter source that always draws `unit`.
///
/// The counterpart to `retry/env_tests.rs`'s `fixture` — the impurity becomes
/// an argument, so the delays below are arithmetic rather than a sample.
/// `polling_policy` starts at 250 ms and doubles, capped at 2 s, and full
/// jitter scales each window by the draw:
///
/// | retry | window | `fixed(0.5)` | `fixed(0.9)` |
/// |-------|--------|--------------|--------------|
/// | 1st   | 250 ms | 125 ms       | 225 ms       |
/// | 2nd   | 500 ms | 250 ms       | 450 ms       |
///
/// Never `0.0` against a permanently contended runtime: every delay would be
/// zero, `elapsed` would never advance on the virtual clock, and the wait has
/// no attempt ceiling to stop it.
fn fixed(unit: f64) -> impl Fn() -> f64 {
    move || unit
}

// =============================================================================
// Waiting
// =============================================================================

/// Two retries fitting inside the window: 125 ms + 250 ms = 375 ms of a 500 ms
/// budget, then the script clears.
#[tokio::test(start_paused = true)]
async fn contention_that_clears_inside_the_window_succeeds() {
    let runtime = ScriptedRuntime::with_script(vec![Err(contended()), Err(contended()), Ok(())]);

    let started = Instant::now();
    let target = ensure_with(
        &runtime,
        "llama",
        None,
        4096,
        Duration::from_millis(500),
        fixed(0.5),
    )
    .await
    .expect("the slot freed before the window elapsed");

    assert_eq!(target.model_name, "llama");
    assert_eq!(
        started.elapsed(),
        Duration::from_millis(375),
        "the wait should have cost exactly the two backoffs"
    );
}

/// The other side of the same boundary, and the reason this pair exists.
///
/// The delays are drawn, so an unlucky draw makes the wait give up *before* the
/// slot would have freed — 225 ms + 450 ms overruns the same 500 ms window that
/// 375 ms fits inside. That behaviour was real and unasserted, and with a live
/// jitter source it surfaced as the sibling test above failing about one run in
/// four rather than as a test of its own.
#[tokio::test(start_paused = true)]
async fn contention_gives_up_when_the_backoff_would_overrun_the_window() {
    let runtime = ScriptedRuntime::with_script(vec![Err(contended()), Err(contended()), Ok(())]);

    let error = ensure_with(
        &runtime,
        "llama",
        None,
        4096,
        Duration::from_millis(500),
        fixed(0.9),
    )
    .await
    .expect_err("a backoff that would overrun the window must not be taken");

    assert!(
        matches!(error, ModelRuntimeError::ContentionTimeout(_)),
        "giving up early still owes the caller a 503: {error:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn contention_that_outlasts_the_window_surfaces_the_error() {
    let runtime = ScriptedRuntime::always_contended();

    let error = ensure_with(
        &runtime,
        "llama",
        None,
        4096,
        Duration::from_millis(200),
        fixed(0.5),
    )
    .await
    .expect_err("a permanently contended slot must still fail");

    assert!(
        matches!(error, ModelRuntimeError::ContentionTimeout(_)),
        "the original error type must survive so the caller still sends 503: {error:?}"
    );
}

/// One 125 ms backoff fits the 200 ms window; the next would be 250 ms, which
/// does not, so the wait stops there rather than overrunning what it promised.
#[tokio::test(start_paused = true)]
async fn the_window_bounds_how_long_the_wait_lasts() {
    let runtime = ScriptedRuntime::always_contended();
    let window = Duration::from_millis(200);

    let started = Instant::now();
    let _ = ensure_with(&runtime, "llama", None, 4096, window, fixed(0.5)).await;

    assert_eq!(
        started.elapsed(),
        Duration::from_millis(125),
        "the wait must stop inside its window, not merely near it"
    );
}

// =============================================================================
// Fail-fast
// =============================================================================

#[tokio::test(start_paused = true)]
async fn a_zero_window_restores_fail_fast() {
    // The behaviour this function replaced: no waiting, straight to 503.
    let runtime = ScriptedRuntime::always_contended();

    let started = Instant::now();
    let error = ensure_with_contention_wait(&runtime, "llama", None, 4096, Duration::ZERO)
        .await
        .expect_err("zero window must not wait");

    assert!(matches!(error, ModelRuntimeError::ContentionTimeout(_)));
    assert_eq!(
        started.elapsed(),
        Duration::ZERO,
        "zero window slept anyway"
    );
}

// =============================================================================
// Scope — everything else passes through untouched
// =============================================================================

#[tokio::test(start_paused = true)]
async fn model_loading_is_not_waited_on() {
    // ModelLoading has its own longer retry loop in the caller. Absorbing it
    // here too would stack two budgets on the same failure.
    let runtime = ScriptedRuntime::with_script(vec![Err(ModelRuntimeError::ModelLoading)]);

    let started = Instant::now();
    let error = ensure_with_contention_wait(&runtime, "llama", None, 4096, Duration::from_secs(30))
        .await
        .expect_err("ModelLoading must pass straight through");

    assert!(matches!(error, ModelRuntimeError::ModelLoading));
    assert_eq!(
        started.elapsed(),
        Duration::ZERO,
        "ModelLoading was waited on"
    );
}

#[tokio::test(start_paused = true)]
async fn other_errors_pass_through_immediately() {
    let runtime =
        ScriptedRuntime::with_script(vec![Err(ModelRuntimeError::Internal("boom".to_owned()))]);

    let error = ensure_with_contention_wait(&runtime, "llama", None, 4096, Duration::from_secs(30))
        .await
        .expect_err("a terminal error must not be waited on");

    assert!(matches!(error, ModelRuntimeError::Internal(_)));
}

// =============================================================================
// Configuration
// =============================================================================

#[test]
fn the_default_window_is_used_when_unset() {
    // Asserted on the constant rather than by mutating the process environment,
    // which would race the other tests in this binary.
    assert_eq!(DEFAULT_WAIT, Duration::from_secs(30));
    assert!(
        !DEFAULT_WAIT.is_zero(),
        "a zero default would silently restore fail-fast for everyone"
    );
}

#[test]
fn the_env_var_name_is_stable() {
    // Documented in the module README and PR 4's operator notes.
    assert_eq!(WAIT_ENV_VAR, "GGLIB_CONTENTION_WAIT_SECS");
}
