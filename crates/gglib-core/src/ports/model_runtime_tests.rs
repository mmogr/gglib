//! The lease, the defaults a minimal runtime inherits, and the error envelope.

use super::*;
use crate::settings::DEFAULT_CONTEXT_SIZE;

/// Implements only the three required methods, so the defaulted ones are
/// exercised exactly as an untouched test double would get them.
#[derive(Debug)]
struct MinimalRuntime;

#[async_trait]
impl ModelRuntimePort for MinimalRuntime {
    async fn admit(
        &self,
        model_name: &str,
        num_ctx: Option<u64>,
        default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        Ok(Admission::detached(RunningTarget::local(
            5500,
            1,
            model_name.to_string(),
            num_ctx.or(default_ctx).unwrap_or(DEFAULT_CONTEXT_SIZE),
            false,
        )))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    /// Refused, so a test can tell its answer from a default's own.
    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Err(ModelRuntimeError::Internal("stop refused".to_owned()))
    }

    async fn stop_model(&self, _model_id: u32) -> Result<bool, ModelRuntimeError> {
        Ok(false)
    }
}

/// A runtime with no resident set to account for must still hand back a
/// usable lease, or every test double would need a scheduler.
#[tokio::test]
async fn a_minimal_runtime_admits_with_a_detached_lease() {
    let admission = MinimalRuntime
        .admit("m", Some(8192), Some(4096), LaunchOverrides::default())
        .await
        .expect("minimal runtime admits");

    assert_eq!(admission.target.model_name, "m");
    assert_eq!(admission.target.effective_ctx, 8192);
    assert_eq!(admission.lease.slot(), 0);
    // Dropping it must be a no-op rather than a panic.
    drop(admission);
}

#[test]
fn admission_snapshot_defaults_to_empty() {
    let snapshot = MinimalRuntime.admission_snapshot();
    assert!(snapshot.slots.is_empty());
    assert!(snapshot.queued.is_empty());
    assert_eq!(snapshot.total_swaps, 0);
}

/// The whole point of the lease: exactly one release per acquisition, on
/// every exit path including an unwinding panic.
#[test]
fn a_lease_releases_its_slot_exactly_once_on_drop() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Default)]
    struct Counter(AtomicUsize);

    impl AdmissionRelease for Counter {
        fn release(&self, slot: usize) {
            assert_eq!(slot, 1, "the lease must release the slot it was given");
            self.0.fetch_add(1, Ordering::SeqCst);
        }

        fn progress(&self, _slot: usize) {}
    }

    let counter = Arc::new(Counter::default());
    {
        let lease = AdmissionLease::new(Arc::clone(&counter) as Arc<dyn AdmissionRelease>, 1);
        assert_eq!(lease.slot(), 1);
        assert_eq!(counter.0.load(Ordering::SeqCst), 0, "not yet released");
    }
    assert_eq!(counter.0.load(Ordering::SeqCst), 1);

    // A panic unwinds through Drop just the same — this workspace sets no
    // `panic = "abort"` profile, so a panicking handler cannot leak a slot.
    let counter2 = Arc::new(Counter::default());
    let held = Arc::clone(&counter2);
    let result = std::panic::catch_unwind(move || {
        let _lease = AdmissionLease::new(held as Arc<dyn AdmissionRelease>, 1);
        panic!("handler blew up mid-request");
    });
    assert!(result.is_err());
    assert_eq!(counter2.0.load(Ordering::SeqCst), 1, "released on unwind");
}

/// A detached lease has nothing to release, so dropping it must not reach
/// for an owner that is not there.
#[test]
fn a_detached_lease_drops_cleanly() {
    let lease = AdmissionLease::detached();
    assert_eq!(lease.slot(), 0);
    drop(lease);
}

/// Unpinned is the safe default: a runtime that says nothing about
/// pinning must not cause callers to narrow what they offer.
#[test]
fn pinned_defaults_to_unpinned() {
    assert!(MinimalRuntime.pinned().is_none());
}

/// A runtime with no resident set holds nothing.
#[test]
fn hold_defaults_to_none() {
    assert!(MinimalRuntime.hold(5500, 1).is_none());
}

/// A runtime with no holds recycles by stopping, and passes on the stop's
/// answer: the benchmark's pinned target refuses its stop so the watchdog
/// re-arms.
#[tokio::test]
async fn recycle_current_defaults_to_stop_current() {
    let refused = MinimalRuntime.recycle_current().await.unwrap_err();
    assert!(
        matches!(&refused, ModelRuntimeError::Internal(m) if m == "stop refused"),
        "{refused}"
    );
}

#[tokio::test]
async fn list_running_defaults_to_empty() {
    assert!(MinimalRuntime.list_running().await.is_empty());
}

/// "No opinion" has to be the default, or merging one in would silently
/// override the runtime's own template.
#[test]
fn launch_overrides_default_is_empty() {
    let overrides = LaunchOverrides::default();
    assert!(overrides.cache_ram.is_none());
    assert!(overrides.options.context_size.is_none());
    assert!(overrides.options.mlock.is_none());
}

fn pinned_mismatch() -> ModelRuntimeError {
    ModelRuntimeError::PinnedModelMismatch {
        expected: "qwen2.5".to_string(),
        requested: "llama-3-8b".to_string(),
    }
}

/// 404 rather than 403: from the caller's point of view the model it asked
/// for does not exist on this endpoint.
#[test]
fn pinned_mismatch_is_not_found() {
    assert_eq!(pinned_mismatch().suggested_status_code(), 404);
}

/// Retrying the identical request can never succeed — the pin is fixed for
/// the process lifetime — so clients must not back off and retry.
#[test]
fn pinned_mismatch_is_not_retryable() {
    assert!(!pinned_mismatch().is_retryable());
}

/// Both model names belong in the message; without them the caller cannot
/// tell what this endpoint actually serves.
#[test]
fn pinned_mismatch_names_both_models() {
    let rendered = pinned_mismatch().to_string();
    assert!(rendered.contains("qwen2.5"), "{rendered}");
    assert!(rendered.contains("llama-3-8b"), "{rendered}");
}

/// A retryable error's envelope must carry `retryable: true` and the
/// `service_unavailable` type, matching the HTTP layer's 503 mapping.
#[test]
fn envelope_for_admission_timeout_is_retryable_service_unavailable() {
    let err = ModelRuntimeError::AdmissionTimeout("waited too long".to_string());
    let envelope = RuntimeErrorEnvelope::from(&err);
    assert_eq!(envelope.r#type, "service_unavailable");
    assert!(envelope.retryable);
    assert_eq!(envelope.message, err.to_string());
}

/// A non-retryable error's envelope must say so, matching the HTTP
/// layer's non-503 mapping.
#[test]
fn envelope_for_pinned_mismatch_is_not_retryable_invalid_request() {
    let envelope = RuntimeErrorEnvelope::from(&pinned_mismatch());
    assert_eq!(envelope.r#type, "invalid_request_error");
    assert!(!envelope.retryable);
}

/// The wire-vocabulary predicate must agree with `is_retryable()` for every
/// variant.
///
/// An HTTP client only ever sees the `type` discriminant — it has no
/// `ModelRuntimeError` to ask. This is what stops the two from drifting
/// into disagreeing about which failures are worth retrying, and it is
/// exhaustive so a new variant cannot quietly skip the check.
#[test]
fn retryable_predicate_agrees_with_the_error_itself() {
    let all = [
        ModelRuntimeError::ModelLoading,
        ModelRuntimeError::AdmissionTimeout("contended".to_string()),
        ModelRuntimeError::ModelNotFound("m".to_string()),
        ModelRuntimeError::ModelFileNotFound("f".to_string()),
        pinned_mismatch(),
        ModelRuntimeError::SpawnFailed("boom".to_string()),
        ModelRuntimeError::HealthCheckFailed("unhealthy".to_string()),
        ModelRuntimeError::Internal("internal".to_string()),
    ];

    for err in all {
        let envelope = RuntimeErrorEnvelope::from(&err);
        assert_eq!(
            is_retryable_error_type(&envelope.r#type),
            err.is_retryable(),
            "wire type {:?} disagrees with is_retryable() for {err:?}",
            envelope.r#type
        );
    }
}

/// A 503 is the only status the retryable discriminant maps to, so the
/// HTTP-status fallback used by clients that receive a non-gglib error
/// body stays consistent with the discriminant path.
#[test]
fn retryable_discriminant_lines_up_with_status_503() {
    let retryable = ModelRuntimeError::AdmissionTimeout("c".to_string());
    assert!(is_retryable_error_type(error_type::SERVICE_UNAVAILABLE));
    assert_eq!(retryable.suggested_status_code(), 503);
    assert!(!is_retryable_error_type(error_type::SERVER_ERROR));
    assert!(!is_retryable_error_type(error_type::INVALID_REQUEST));
}

/// An image model asked to chat is the caller's mistake, a 400 with the
/// invalid-request type, and retrying it can never succeed.
#[test]
fn an_image_model_cannot_chat_is_an_invalid_request() {
    let refused = ModelRuntimeError::ImageModelCannotChat("flux".to_owned());

    assert_eq!(refused.suggested_status_code(), 400);
    assert!(!refused.is_retryable());
    let envelope = RuntimeErrorEnvelope::from(&refused);
    assert_eq!(envelope.r#type, error_type::INVALID_REQUEST);
    assert!(envelope.message.contains("'flux'"), "{}", envelope.message);
}

/// No runtime to draw with is a 503 the caller cannot retry its way out of:
/// the server-error type, and words that name the install command.
#[test]
fn an_image_runtime_not_installed_is_a_503_that_names_the_command() {
    let refused = ModelRuntimeError::ImageRuntimeNotInstalled;

    assert_eq!(refused.suggested_status_code(), 503);
    assert!(!refused.is_retryable());
    let envelope = RuntimeErrorEnvelope::from(&refused);
    assert_eq!(envelope.r#type, error_type::SERVER_ERROR);
    assert!(!envelope.retryable);
    assert!(
        envelope.message.contains("`gglib config sd install`"),
        "{}",
        envelope.message
    );
    assert!(
        envelope.message.contains("sd-server"),
        "{}",
        envelope.message
    );
}

/// A model missing a file is the caller's to fix: a 400, and every missing
/// role named with the flag that links it.
#[test]
fn an_incomplete_image_model_is_a_400_that_names_every_missing_role() {
    let refused = ModelRuntimeError::ImageModelIncomplete {
        model: "flux1-schnell".to_owned(),
        missing: vec![
            ComponentRole::Vae,
            ComponentRole::ClipL,
            ComponentRole::T5xxl,
        ],
    };

    assert_eq!(refused.suggested_status_code(), 400);
    assert!(!refused.is_retryable());
    let envelope = RuntimeErrorEnvelope::from(&refused);
    assert_eq!(envelope.r#type, error_type::INVALID_REQUEST);
    assert_eq!(
        envelope.message,
        "Image model 'flux1-schnell' has no VAE, CLIP-L or T5-XXL linked, so it cannot draw. \
         Link each with `gglib model update \"flux1-schnell\" --component vae=<path> \
         --component clip_l=<path> --component t5xxl=<path>`."
    );

    let one = ModelRuntimeError::ImageModelIncomplete {
        model: "sdxl".to_owned(),
        missing: vec![ComponentRole::Vae],
    };
    assert_eq!(
        one.to_string(),
        "Image model 'sdxl' has no VAE linked, so it cannot draw. Link it with \
         `gglib model update \"sdxl\" --component vae=<path>`."
    );
}

/// No room beside a held model is a retryable 503, and the words name the
/// held model and both sizes, or say the free memory could not be read.
#[test]
fn an_image_model_that_does_not_fit_is_a_retryable_503_naming_the_held_model_and_bytes() {
    const GIB: u64 = 1024 * 1024 * 1024;
    let refused = ModelRuntimeError::ImageModelDoesNotFit {
        model: "flux1-schnell".to_owned(),
        held_model: "qwen3-27b".to_owned(),
        needed_bytes: Some(28 * GIB),
        free_bytes: Some(9 * GIB),
    };

    assert_eq!(refused.suggested_status_code(), 503);
    assert!(refused.is_retryable());
    let envelope = RuntimeErrorEnvelope::from(&refused);
    assert_eq!(envelope.r#type, error_type::SERVICE_UNAVAILABLE);
    assert!(envelope.retryable);
    assert_eq!(
        envelope.message,
        "Image model 'flux1-schnell' needs 28.00 GiB; 9.00 GiB is free beside 'qwen3-27b', \
         which a chat is using. Use a smaller family, stop the chat model, or draw from a chat \
         on the paired machine."
    );

    let unknown = ModelRuntimeError::ImageModelDoesNotFit {
        model: "flux1-schnell".to_owned(),
        held_model: "qwen3-27b".to_owned(),
        needed_bytes: None,
        free_bytes: None,
    };
    assert_eq!(
        unknown.to_string(),
        "Image model 'flux1-schnell' needs more memory than is free; how much is free beside \
         'qwen3-27b' cannot be read, which a chat is using. Use a smaller family, stop the chat \
         model, or draw from a chat on the paired machine."
    );
}
