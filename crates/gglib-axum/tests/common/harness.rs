//! Bootstrap harness for the integration tests in this crate.
//!
//! Every test here needs the same two properties, and wrote them out by hand:
//!
//! 1. **Isolation.** [`bootstrap`] without a `db_path` resolves through
//!    `gglib_core::paths::database_path()`, which in a debug build is the
//!    developer's own checkout. No suite had this. Tests were reading rows
//!    they did not create and leaving rows behind for the next run.
//! 2. **Honesty.** A bootstrap failure has to fail the test. Every suite but
//!    `daemon_route_contract` skipped instead — `Err(_) => return` in four of
//!    them, `bootstrap(..).ok()?` behind a let-else in `daemon_access` —
//!    which turns a contract test into a no-op that reports success.
//!
//! Routing every suite through [`test_state`] leaves one place to get both
//! right. Nothing enforces it, though: `gglib_axum::bootstrap` is `pub`, and
//! no check reads these files for *how* they bootstrap, so a new test can
//! still open the developer's database by writing the four lines out again.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

use axum::Router;
use gglib_axum::{AxumContext, DaemonAccess, ServerConfig, bootstrap, create_router};
use gglib_core::CorsConfig;

use super::ports::TEST_BASE_PORT;

/// Scratch directory for this test process.
///
/// Under Cargo's own scratch directory so `cargo clean` collects it, and
/// keyed by both the executable's file name and the process id.
///
/// The pid is what keeps two runs apart *in space*. An earlier version keyed
/// on the binary alone, which two concurrent `cargo test` runs of the same
/// binary defeat exactly: both wipe the shared directory, both restart the
/// counter at zero, and both open `gglib-0.db` — `SQLITE_BUSY: database is
/// locked`.
///
/// It does not keep them apart *in time*: pids recycle (macOS wraps around
/// 99998), so a later run can be handed a directory a dead run left behind
/// and read rows it did not create — the very thing this module exists to
/// stop. Emptying our own directory closes that, and is safe in a way the
/// old shared wipe was not: if this pid is live it is us, and if it was
/// recycled the previous owner is gone.
///
/// Nothing else collects these, so [`sweep_stale`] takes the siblings whose
/// runs are long over.
///
/// (Cargo runs this crate's test binaries one at a time; it is the tests
/// *within* a binary that share a process. The binary name is in the path
/// for legibility, not for exclusion.)
static SCRATCH_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let exe = std::env::current_exe().expect("locate the test executable");
    let mut name = exe
        .file_name()
        .expect("test executable has a file name")
        .to_os_string();
    name.push(format!("-{}", std::process::id()));

    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let dir = root.join(name);

    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create isolated data dir");
    // `dir` by argument, not through `SCRATCH_DIR`: we are inside its
    // initialiser, and touching it here would deadlock on itself.
    sweep_stale(&root, &dir);
    dir
});

/// How long a scratch directory is assumed to belong to a live run.
///
/// These suites finish in seconds; an hour is far outside any run's lifetime,
/// so a directory older than this cannot belong to a process still using it.
const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Drop scratch directories left by runs that are certainly finished.
///
/// Without this they accumulate forever — a full suite leaves roughly 10 MiB
/// across six directories, and `cargo clean -p gglib-axum` does not reclaim
/// them, so only a whole-target clean ever did. Best-effort throughout: a
/// directory that resists removal is a directory the next run will try again.
fn sweep_stale(root: &std::path::Path, ours: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .and_then(|modified| modified.elapsed().map_err(std::io::Error::other))
            .is_ok_and(|age| age > STALE_AFTER);

        if stale && entry.path() != ours {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// A database of its own for each context.
///
/// Per test rather than per binary: tests in one binary run concurrently, so
/// a shared file both deadlocks on SQLite's write lock and lets one test see
/// another's rows.
fn isolated_db_path() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    SCRATCH_DIR.join(format!("gglib-{}.db", NEXT.fetch_add(1, Ordering::Relaxed)))
}

/// Config for a context that binds nothing and launches nothing.
fn test_config(cors: CorsConfig) -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".into(),
        port: 0,
        base_port: TEST_BASE_PORT,
        llama_server_path: "/nonexistent/llama-server".into(),
        max_concurrent_agent_loops: 1,
        static_dir: None,
        cors,
        db_path: Some(isolated_db_path()),
    }
}

/// Bootstrap a context against a database of its own.
///
/// # Panics
///
/// If bootstrap fails. That is the point, and it is #842's convention: a
/// route-contract test that passes silently is the failure mode it exists to
/// prevent, since #834 shipped a deleted route through a fully green suite.
pub(crate) async fn test_state(cors: CorsConfig) -> Arc<AxumContext> {
    let ctx = bootstrap(test_config(cors))
        .await
        .expect("bootstrap an isolated test context");

    Arc::new(ctx)
}

/// The access policy most tests here run under: loopback, no token.
pub(crate) fn test_access() -> Arc<DaemonAccess> {
    Arc::new(DaemonAccess::loopback())
}

/// [`test_state`] plus the router over it, for tests that assert on both.
pub(crate) async fn test_state_and_app(cors: CorsConfig) -> (Arc<AxumContext>, Router) {
    let state = test_state(cors.clone()).await;
    let router = create_router(Arc::clone(&state), &cors, test_access());
    (state, router)
}

/// [`test_state_and_app`] for the tests that never touch the context.
#[allow(dead_code)] // each test binary uses a different subset
pub(crate) async fn test_app(cors: CorsConfig) -> Router {
    test_state_and_app(cors).await.1
}

/// [`test_app`] under an access policy other than plain loopback.
#[allow(dead_code)] // each test binary uses a different subset
pub(crate) async fn test_app_with_access(cors: CorsConfig, access: DaemonAccess) -> Router {
    let state = test_state(cors.clone()).await;
    create_router(state, &cors, Arc::new(access))
}
