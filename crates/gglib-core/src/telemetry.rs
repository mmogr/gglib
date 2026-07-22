//! Unified tracing initialization for gglib.
//!
//! Design:
//! - A single layered subscriber (console + daily rotating file) is installed once via [`OnceLock`].
//! - Calls to [`init_tracing`] are idempotent — subsequent calls return `Ok(())`.
//! - Log directory: `./logs/` in debug builds, `data_root()/logs` in release.
//! - Filter: `RUST_LOG` env var wins; otherwise `"debug"` if verbose, else `"warn"`.
//! - Console output goes through [`console_println`], which defaults to stderr
//!   but can be redirected via [`set_console_hook`] — see the "Console hook"
//!   section below.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};

use tracing_subscriber::{EnvFilter, Registry, layer::SubscriberExt, util::SubscriberInitExt};

#[allow(unused_imports)] // only used in release builds via cfg(not(debug_assertions))
use crate::paths::data_root;

static GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

// ─── Console hook ────────────────────────────────────────────────────────────
//
// The stdout `fmt` layer below writes through this hook instead of directly
// to a stream. Default (`None`) is a plain `eprintln!`. A CLI running an
// indicatif `MultiProgress` installs a hook that forwards through
// `MultiProgress::println` instead, so a log line emitted while download bars
// are live gets erased-printed-redrawn atomically rather than landing as raw
// bytes that corrupt the bars' redraw bookkeeping. See
// `gglib-download/src/cli_emitter.rs` for the installing side.

/// A sink for formatted console lines, e.g. one backed by
/// `MultiProgress::println`.
pub type ConsoleHook = Arc<dyn Fn(&str) + Send + Sync>;

/// Route for formatted log lines and other CLI console output. `None` means
/// "print straight to stderr".
static CONSOLE_HOOK: RwLock<Option<ConsoleHook>> = RwLock::new(None);

/// Install a hook that receives each formatted log line (and other console
/// output routed via [`console_println`]) instead of stderr.
pub fn set_console_hook(hook: ConsoleHook) {
    *CONSOLE_HOOK.write().unwrap() = Some(hook);
}

/// Remove a previously installed hook, reverting to plain stderr.
pub fn clear_console_hook() {
    *CONSOLE_HOOK.write().unwrap() = None;
}

/// Print one line through the installed console hook, or to stderr if none is
/// installed.
///
/// Used by the tracing `fmt` layer below, and by CLI code that prints
/// outside of `tracing` (subprocess passthrough, setup notices) so every
/// console write is subject to the same routing.
pub fn console_println(line: &str) {
    let hook = CONSOLE_HOOK.read().unwrap();
    if let Some(hook) = hook.as_ref() {
        hook(line);
    } else {
        eprintln!("{line}");
    }
}

/// `Write` target for the tracing `fmt` layer. Buffers one formatted record
/// and forwards it as a single line via [`console_println`] when dropped —
/// `fmt::layer()` creates a fresh writer per event, so `Drop` is exactly
/// "this record is complete."
#[derive(Default)]
struct ConsoleWriter(Vec<u8>);

impl std::io::Write for ConsoleWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for ConsoleWriter {
    fn drop(&mut self) {
        if self.0.is_empty() {
            return;
        }
        let text = String::from_utf8_lossy(&self.0);
        console_println(text.trim_end_matches('\n'));
    }
}

fn resolve_log_dir() -> PathBuf {
    #[cfg(debug_assertions)]
    let dir = PathBuf::from("./logs");

    #[cfg(not(debug_assertions))]
    let dir = data_root()
        .unwrap_or_else(|_| PathBuf::from("./logs"))
        .join("logs");

    std::fs::create_dir_all(&dir).ok();
    dir
}

fn build_env_filter(verbose: bool) -> EnvFilter {
    std::env::var("RUST_LOG").map_or_else(
        |_| {
            let level = if verbose { "debug" } else { "warn" };
            EnvFilter::try_new(level).unwrap_or_default()
        },
        |log_env| EnvFilter::try_new(log_env).unwrap_or_default(),
    )
}

/// Initialize the global tracing subscriber.
///
/// Safe to call multiple times; only the first call installs the subscriber.
pub fn init_tracing(verbose: bool) -> anyhow::Result<()> {
    // Idempotent: if already initialized, no-op
    if GUARD.get().is_some() {
        return Ok(());
    }

    let log_dir = resolve_log_dir();
    let file_appender = tracing_appender::rolling::daily(&log_dir, "gglib.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = build_env_filter(verbose);

    let subscriber = Registry::default()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_writer(ConsoleWriter::default),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_target(false),
        );

    subscriber
        .try_init()
        .map_err(|e| anyhow::anyhow!("failed to set global tracer: {e}"))?;

    // Ignore the Result since failure just means another thread set it concurrently
    let _ = GUARD.set(guard);

    Ok(())
}
