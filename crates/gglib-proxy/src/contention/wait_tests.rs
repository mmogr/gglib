//! Contention-wait tests.
//!
//! A scripted runtime double returns a canned sequence of outcomes, so the
//! interesting property — *which* errors are waited on and for how long — is
//! asserted without launching a model. Windows are milliseconds so the suite
//! stays fast; the backoff arithmetic itself is proven in `gglib_core::retry`.

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

// =============================================================================
// Waiting
// =============================================================================

#[tokio::test]
async fn contention_that_clears_inside_the_window_succeeds() {
    let runtime = ScriptedRuntime::with_script(vec![Err(contended()), Err(contended()), Ok(())]);

    let target =
        ensure_with_contention_wait(&runtime, "llama", None, 4096, Duration::from_millis(500))
            .await
            .expect("the slot freed before the window elapsed");

    assert_eq!(target.model_name, "llama");
}

#[tokio::test]
async fn contention_that_outlasts_the_window_surfaces_the_error() {
    let runtime = ScriptedRuntime::always_contended();

    let error =
        ensure_with_contention_wait(&runtime, "llama", None, 4096, Duration::from_millis(200))
            .await
            .expect_err("a permanently contended slot must still fail");

    assert!(
        matches!(error, ModelRuntimeError::ContentionTimeout(_)),
        "the original error type must survive so the caller still sends 503: {error:?}"
    );
}

#[tokio::test]
async fn the_window_bounds_how_long_the_wait_lasts() {
    let runtime = ScriptedRuntime::always_contended();
    let window = Duration::from_millis(200);

    let started = Instant::now();
    let _ = ensure_with_contention_wait(&runtime, "llama", None, 4096, window).await;

    assert!(
        started.elapsed() < window * 4,
        "waiting overran its window by too much: {:?}",
        started.elapsed()
    );
}

// =============================================================================
// Fail-fast
// =============================================================================

#[tokio::test]
async fn a_zero_window_restores_fail_fast() {
    // The behaviour this function replaced: no waiting, straight to 503.
    let runtime = ScriptedRuntime::always_contended();

    let started = Instant::now();
    let error = ensure_with_contention_wait(&runtime, "llama", None, 4096, Duration::ZERO)
        .await
        .expect_err("zero window must not wait");

    assert!(matches!(error, ModelRuntimeError::ContentionTimeout(_)));
    assert!(
        started.elapsed() < Duration::from_millis(50),
        "zero window slept anyway: {:?}",
        started.elapsed()
    );
}

// =============================================================================
// Scope — everything else passes through untouched
// =============================================================================

#[tokio::test]
async fn model_loading_is_not_waited_on() {
    // ModelLoading has its own longer retry loop in the caller. Absorbing it
    // here too would stack two budgets on the same failure.
    let runtime = ScriptedRuntime::with_script(vec![Err(ModelRuntimeError::ModelLoading)]);

    let started = Instant::now();
    let error = ensure_with_contention_wait(&runtime, "llama", None, 4096, Duration::from_secs(30))
        .await
        .expect_err("ModelLoading must pass straight through");

    assert!(matches!(error, ModelRuntimeError::ModelLoading));
    assert!(
        started.elapsed() < Duration::from_millis(50),
        "ModelLoading was waited on: {:?}",
        started.elapsed()
    );
}

#[tokio::test]
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
