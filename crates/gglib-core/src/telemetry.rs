//! Unified tracing initialization for gglib.
//!
//! Design:
//! - A single layered subscriber (stdout + daily rotating file) is installed once via [`OnceLock`].
//! - Calls to [`init_tracing`] are idempotent — subsequent calls return `Ok(())`.
//! - Log directory: `./logs/` in debug builds, `data_root()/logs` in release.
//! - Filter: `RUST_LOG` env var wins; otherwise `"debug"` if verbose, else `"warn"`.

use std::path::PathBuf;
use std::sync::OnceLock;

use tracing_subscriber::{EnvFilter, Registry, layer::SubscriberExt, util::SubscriberInitExt};

#[allow(unused_imports)] // only used in release builds via cfg(not(debug_assertions))
use crate::paths::data_root;

static GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

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
                .with_writer(std::io::stdout),
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
