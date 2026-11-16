//! What a failed load says (#1959 C5): one sentence with its cause, in the
//! words every surface shows.

use crate::domain::RuntimeKind;
use crate::ports::{LoadCause, LoadFailure, ModelRuntimeError, RuntimeErrorEnvelope};
use crate::utils::process::bounded::Failure;

const GGUF_LINE: &str = "gguf_init_from_reader: invalid magic characters: 'this', expected 'GGUF'";

fn qwen(cause: LoadCause) -> LoadFailure {
    LoadFailure {
        model: "qwen".to_owned(),
        runtime: RuntimeKind::Llama,
        cause,
        fitted_context: None,
    }
}

#[test]
fn a_server_that_stopped_says_how_it_ended_and_what_it_printed() {
    let failure = qwen(LoadCause::Stopped {
        failure: Some(Failure::Failed { code: 1 }),
        said: Some(GGUF_LINE.to_owned()),
    });

    assert_eq!(
        failure.to_string(),
        format!(
            "llama-server stopped while loading qwen: it exited with status 1. It printed: {GGUF_LINE}"
        )
    );
}

/// A library the loader could not find is said in the engine check's
/// words, which already come from what the loader printed, so the line is
/// not given twice.
#[test]
fn a_missing_library_is_said_once_in_the_checks_words() {
    let failure = qwen(LoadCause::Stopped {
        failure: Some(Failure::MissingLibrary {
            name: Some("libvulkan.so.1".to_owned()),
        }),
        said: Some(
            "./llama-server: error while loading shared libraries: libvulkan.so.1: cannot open"
                .to_owned(),
        ),
    });

    assert_eq!(
        failure.to_string(),
        "llama-server stopped while loading qwen: the library libvulkan.so.1 it needs is missing"
    );
}

#[test]
fn a_server_that_exited_0_and_printed_nothing_says_only_that() {
    let failure = qwen(LoadCause::Stopped {
        failure: None,
        said: None,
    });

    assert_eq!(
        failure.to_string(),
        "llama-server stopped while loading qwen, before it was ready"
    );
}

#[test]
fn a_server_never_ready_says_how_long_it_had_and_what_it_printed() {
    let line = "load_tensors: loading model tensors, this can take a while...";
    let printed = qwen(LoadCause::NotReady {
        waited_secs: 120,
        said: Some(line.to_owned()),
    });
    let silent = qwen(LoadCause::NotReady {
        waited_secs: 120,
        said: None,
    });

    assert_eq!(
        printed.to_string(),
        format!(
            "llama-server was not ready within 120 s of starting to load qwen. It printed: {line}"
        )
    );
    assert_eq!(
        silent.to_string(),
        "llama-server was not ready within 120 s of starting to load qwen, and printed nothing"
    );
}

/// The refusal of an engine known not to work is the engine check's
/// sentence as it stands: the one the engine's status gives.
#[test]
fn an_engine_not_working_is_refused_in_the_checks_sentence() {
    let reason = "llama-server cannot start: a library it needs is missing.\n\nBinary: /x";
    let failure = qwen(LoadCause::EngineNotWorking {
        failure: Failure::MissingLibrary { name: None },
        reason: reason.to_owned(),
    });

    assert_eq!(failure.to_string(), reason);
}

/// A server that failed with a context gglib fitted says so, and how to
/// override it; an engine that never ran had no context to blame.
#[test]
fn a_fitted_context_is_named_for_a_server_that_ran() {
    let mut stopped = qwen(LoadCause::Stopped {
        failure: Some(Failure::Failed { code: 1 }),
        said: None,
    });
    stopped.fitted_context = Some(65_536);
    let mut refused = qwen(LoadCause::EngineNotWorking {
        failure: Failure::NotFound,
        reason: "llama-server not found at: /x".to_owned(),
    });
    refused.fitted_context = Some(65_536);

    let said = stopped.to_string();
    assert!(
        said.ends_with(&format!(
            " — context 65536 was fitted to this machine; set a global default to override it, \
             or GGLIB_DISABLE_CONTEXT_FIT=1 to fall back to {}",
            crate::settings::DEFAULT_CONTEXT_SIZE
        )),
        "{said}"
    );
    assert_eq!(refused.to_string(), "llama-server not found at: /x");
}

/// The failure is the error's whole message: no prefix of the variant's,
/// a 500 that retrying does not mend, and the same words over SSE.
#[test]
fn a_load_failure_is_its_own_message_on_every_wire() {
    let failure = qwen(LoadCause::Stopped {
        failure: Some(Failure::Failed { code: 1 }),
        said: Some(GGUF_LINE.to_owned()),
    });
    let err = ModelRuntimeError::LoadFailed(Box::new(failure.clone()));
    let envelope = RuntimeErrorEnvelope::from(&err);

    assert_eq!(err.to_string(), failure.to_string());
    assert_eq!(err.suggested_status_code(), 500);
    assert!(!err.is_retryable());
    assert_eq!(envelope.message, failure.to_string());
    assert_eq!(envelope.r#type, "server_error");
    assert_eq!(LoadFailure::CODE, "model_load_failed");
}
