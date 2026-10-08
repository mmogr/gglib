//! Tests for weights the GGUF reader refuses: the row stored, the answer, and
//! the log line.

use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::projector_tests::{Registered, downloaded, register};

/// Keeps what is logged.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Registers a download in `dir` whose weights file holds `weights`. Answers
/// the registration, and every line logged at `WARN` while it was made.
async fn register_weights(dir: &Path, weights: &str) -> (Registered, Vec<String>) {
    let download = downloaded(dir, "projector");
    std::fs::write(&download.primary_path, weights).unwrap();

    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    // Other tests hit the registrar's callsites on other threads. A second
    // registered dispatcher makes a callsite one of them hit first consult
    // this capture too, instead of caching that nobody listens.
    let capture = tracing::Dispatch::new(subscriber);
    let _second = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    let _default = tracing::dispatcher::set_default(&capture);
    tracing::callsite::rebuild_interest_cache();

    let registered = register(&download).await;

    let log = String::from_utf8_lossy(&captured.0.lock().unwrap()).into_owned();
    let warnings = log
        .lines()
        .filter(|line| line.contains(" WARN "))
        .map(str::to_owned)
        .collect();
    (registered, warnings)
}

/// The reader may be stricter than llama.cpp, so a file it refuses is in the
/// library all the same, under its path and with none of a header's details.
/// The answer carries the reader's own sentence, and the log has it once.
#[tokio::test]
async fn weights_the_reader_refuses_are_registered_and_the_answer_and_the_log_say_why() {
    let dir = tempfile::tempdir().unwrap();

    let (registered, warnings) = register_weights(dir.path(), "truncated").await;

    let weights = dir.path().join("zeta.Q8_0.gguf");
    assert_eq!(registered.stored.file_path, weights);
    assert_eq!(registered.stored.architecture, None);
    assert_eq!(registered.stored.context_length, None);
    assert!(registered.stored.metadata.is_empty());
    assert_eq!(
        registered.answer.metadata_refusal.as_deref(),
        Some("Invalid GGUF format: no GGUF magic")
    );
    let [warning] = &warnings[..] else {
        panic!("one warning, not {warnings:?}");
    };
    assert!(warning.contains("zeta.Q8_0.gguf"), "{warning}");
    assert!(
        warning.contains("Invalid GGUF format: no GGUF magic"),
        "{warning}"
    );
}

#[tokio::test]
async fn weights_the_reader_accepts_report_nothing_and_log_no_warning() {
    let dir = tempfile::tempdir().unwrap();

    let (registered, warnings) = register_weights(dir.path(), "weights").await;

    assert_eq!(registered.answer.metadata_refusal, None);
    assert_eq!(warnings, Vec::<String>::new());
}
