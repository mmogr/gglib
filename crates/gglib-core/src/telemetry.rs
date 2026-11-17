//! Unified tracing initialization for gglib.
//!
//! Design:
//! - A single layered subscriber (console + daily rotating file) is installed once via [`OnceLock`].
//! - Calls to [`init_tracing`] are idempotent — subsequent calls return `Ok(())`.
//! - Log directory: `data_root()/logs` in every build — which honours
//!   `GGLIB_DATA_DIR`, so an isolated data dir isolates its logs too.
//!   `./logs/` only as a last resort when no data root resolves.
//! - Filter, one per layer: `RUST_LOG` wins for both; otherwise `"debug"` for
//!   both if verbose; otherwise the console shows `"warn"` and the file keeps
//!   `"warn,gglib=info"`, so the file says what gglib did (what it installed,
//!   downloaded, started and stopped) and a terminal stays quiet.
//! - Console output goes through [`console_println`], which defaults to stderr
//!   but can be redirected via [`set_console_hook`] — see the "Console hook"
//!   section below.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};

use tracing_subscriber::{
    EnvFilter, Layer, Registry, layer::SubscriberExt, util::SubscriberInitExt,
};

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
// `gglib-cli/src/console.rs` for the installing side.

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

/// Remove the installed hook, reverting to plain stderr.
///
/// No production caller: the CLI installs a hook for the life of the process
/// and exits without clearing it. Kept, and gated, because `CONSOLE_HOOK` is
/// process-global — a test that installs one has to be able to put it back,
/// or it leaks into every test that runs after it in the same binary.
#[cfg(test)]
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
    // The data root decides in every build, so a test daemon pointed at an
    // isolated `GGLIB_DATA_DIR` never interleaves its lines into the real
    // installation's log file. `data_root()` prefers `GGLIB_DATA_DIR`, then
    // the user's gglib folder; if it fails, the fallback root is the working
    // directory itself.
    let dir = data_root()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("logs");

    std::fs::create_dir_all(&dir).ok();
    dir
}

/// What the console shows unless told otherwise: warnings and errors.
const CONSOLE_DEFAULT: &str = "warn";

/// What the log file keeps unless told otherwise: warnings and errors from
/// everything, and gglib's own `info` lines, which say what it did (an
/// engine installed, a download ended, a server started with what
/// arguments, became ready, was stopped). A directive names the start of a
/// target, so `gglib` is every crate of the workspace. Other crates' `info`
/// stays out: it is theirs to explain, and it would bury gglib's.
const FILE_DEFAULT: &str = "warn,gglib=info";

/// The directives the console and the log file are filtered by, in that
/// order: `RUST_LOG` for both when it is set, else `debug` for both when
/// `verbose`, else each one's default.
fn directives(verbose: bool, rust_log: Option<&str>) -> (String, String) {
    match (rust_log, verbose) {
        (Some(set), _) => (set.to_owned(), set.to_owned()),
        (None, true) => ("debug".to_owned(), "debug".to_owned()),
        (None, false) => (CONSOLE_DEFAULT.to_owned(), FILE_DEFAULT.to_owned()),
    }
}

/// The filter `directives` names; one that does not parse filters as
/// `EnvFilter`'s default does.
fn filter(directives: &str) -> EnvFilter {
    EnvFilter::try_new(directives).unwrap_or_default()
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

    let (console, file) = directives(verbose, std::env::var("RUST_LOG").ok().as_deref());

    let subscriber = Registry::default()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_writer(ConsoleWriter::default)
                .with_filter(filter(&console)),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_target(false)
                .with_filter(filter(&file)),
        );

    subscriber
        .try_init()
        .map_err(|e| anyhow::anyhow!("failed to set global tracer: {e}"))?;

    // Ignore the Result since failure just means another thread set it concurrently
    let _ = GUARD.set(guard);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `console_println` must forward to an installed hook rather than
    /// writing to stderr directly — this is the mechanism that lets a live
    /// `MultiProgress` intercept log lines and redraw around them instead of
    /// having them corrupt its bookkeeping. `CONSOLE_HOOK` is the only piece
    /// of process-global state this module touches and no other test in
    /// this crate installs a hook, so a single test covering both install
    /// and teardown is safe without cross-test races.
    #[test]
    fn console_println_routes_through_an_installed_hook() {
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let captured_clone = Arc::clone(&captured);
        set_console_hook(Arc::new(move |line: &str| {
            captured_clone.lock().unwrap().push(line.to_string());
        }));

        console_println("hello from the hook");
        clear_console_hook();

        assert_eq!(captured.lock().unwrap().as_slice(), ["hello from the hook"]);

        // After clearing, console_println must not reach the cleared hook.
        console_println("after clear");
        assert_eq!(captured.lock().unwrap().len(), 1);
    }

    #[test]
    fn rust_log_wins_for_both_then_verbose_then_each_default() {
        let pair = |console: &str, file: &str| (console.to_owned(), file.to_owned());
        assert_eq!(directives(false, None), pair("warn", "warn,gglib=info"));
        assert_eq!(directives(true, None), pair("debug", "debug"));
        assert_eq!(directives(false, Some("trace")), pair("trace", "trace"));
        assert_eq!(directives(true, Some("error")), pair("error", "error"));
    }

    /// A writer that keeps what a layer wrote.
    #[derive(Clone, Default)]
    struct Kept(Arc<Mutex<Vec<u8>>>);

    impl Kept {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl std::io::Write for Kept {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// With nothing set, the file keeps what gglib did and the console does
    /// not show it; neither takes another crate's `info`, and both take a
    /// warning from anywhere. A session that installed both engines,
    /// downloaded a model and answered two chats once left three lines.
    #[test]
    fn the_file_keeps_gglibs_info_and_the_console_does_not() {
        /// A layer writing to `to` what `lets` lets through.
        fn layer<S>(to: Kept, lets: EnvFilter) -> impl Layer<S> + Send + Sync + 'static
        where
            S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
        {
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(move || to.clone())
                .with_filter(lets)
        }

        let (console, file) = directives(false, None);
        let (shown, kept) = (Kept::default(), Kept::default());
        let subscriber = Registry::default()
            .with(layer(shown.clone(), filter(&console)))
            .with(layer(kept.clone(), filter(&file)));

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "gglib_runtime::process::health", "llama-server is ready on port 9000");
            tracing::info!(target: "gglib.download", "Queue run STARTED");
            tracing::debug!(target: "gglib_runtime::process", "a detail");
            tracing::info!(target: "hyper::client", "another crate's news");
            tracing::warn!(target: "hyper::client", "another crate's warning");
        });

        let kept = kept.text();
        assert!(
            kept.contains("llama-server is ready on port 9000"),
            "{kept}"
        );
        assert!(kept.contains("Queue run STARTED"), "{kept}");
        assert!(kept.contains("another crate's warning"), "{kept}");
        assert!(!kept.contains("a detail"), "{kept}");
        assert!(!kept.contains("another crate's news"), "{kept}");

        let shown = shown.text();
        assert!(shown.contains("another crate's warning"), "{shown}");
        assert!(!shown.contains("is ready on port"), "{shown}");
        assert!(!shown.contains("Queue run STARTED"), "{shown}");
    }
}
