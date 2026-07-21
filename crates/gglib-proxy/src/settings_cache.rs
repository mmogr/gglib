//! Short-lived snapshot of application settings.
//!
//! Every chat-completion request needs settings — global inference defaults,
//! and now the configured inference profiles. Reading them per request meant a
//! full `SELECT` over the `settings_kv` table on the hot path, twice on the
//! upstream-death retry path. Profiles are needed *earlier* in the handler than
//! the defaults were, so without a cache the cost would have doubled again.
//!
//! This wraps the repository in a snapshot that is reused for [`DEFAULT_TTL`]
//! before being refreshed, turning a per-request query into roughly one query
//! per TTL window. Callers hold the returned [`Arc`] for the life of the
//! request and borrow from it, so nothing is cloned per request either.
//!
//! # Why a TTL rather than invalidation on write
//!
//! The obvious alternative — clear the cache whenever settings are saved —
//! cannot work here. The CLI writes the same SQLite file from a **separate
//! process**, so an in-process invalidation hook would never observe
//! `gglib config profile set` while the proxy is running. A TTL bounds
//! staleness uniformly no matter which process did the writing, at the cost of
//! settings changes taking up to [`DEFAULT_TTL`] to take effect.
//!
//! # Failure behaviour
//!
//! A failed load never fails the request. The last good snapshot is served if
//! there is one, and [`Settings::default`] otherwise — matching the previous
//! `.ok().and_then(...)` behaviour at the call sites. A failure does not
//! refresh the expiry, so the next request retries rather than serving a stale
//! value for a whole TTL window; during a sustained outage that means one
//! attempt per request, which is what the code did before this cache existed.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gglib_core::Settings;
use gglib_core::ports::SettingsRepository;
use tokio::sync::RwLock;
use tracing::warn;

/// How long a snapshot is served before it is refreshed.
///
/// Short enough that a settings change from the GUI or CLI shows up quickly
/// enough to feel immediate, long enough that a burst of requests collapses to
/// a single query.
pub const DEFAULT_TTL: Duration = Duration::from_secs(5);

/// A settings snapshot refreshed at most once per TTL window.
pub struct SettingsCache {
    repo: Arc<dyn SettingsRepository>,
    /// The current snapshot and the instant it expires. `None` until the first
    /// successful load.
    snapshot: RwLock<Option<(Arc<Settings>, Instant)>>,
    ttl: Duration,
}

/// Hand-written because [`SettingsRepository`] is not `Debug`; the repository
/// is elided rather than dropping the impl, which `AppState` needs.
impl std::fmt::Debug for SettingsCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsCache")
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

impl SettingsCache {
    /// Wrap a repository with the default TTL.
    #[must_use]
    pub fn new(repo: Arc<dyn SettingsRepository>) -> Self {
        Self::with_ttl(repo, DEFAULT_TTL)
    }

    /// Wrap a repository with an explicit TTL.
    #[must_use]
    pub fn with_ttl(repo: Arc<dyn SettingsRepository>, ttl: Duration) -> Self {
        Self {
            repo,
            snapshot: RwLock::new(None),
            ttl,
        }
    }

    /// Get the current settings, refreshing if the snapshot has expired.
    ///
    /// Never fails: see the module docs for what happens when the repository
    /// errors.
    pub async fn get(&self) -> Arc<Settings> {
        // Fast path: a live snapshot, taken under a read lock so concurrent
        // requests do not serialise on each other.
        if let Some((settings, expires_at)) = self.snapshot.read().await.as_ref()
            && Instant::now() < *expires_at
        {
            return Arc::clone(settings);
        }

        // Refresh under the write lock. Holding it across the load is
        // deliberate: it makes the refresh single-flight, so a burst of
        // requests arriving on an expired snapshot issues one query rather
        // than one each.
        let mut guard = self.snapshot.write().await;

        // Another task may have refreshed while this one waited for the lock.
        if let Some((settings, expires_at)) = guard.as_ref()
            && Instant::now() < *expires_at
        {
            return Arc::clone(settings);
        }

        match self.repo.load().await {
            Ok(settings) => {
                let settings = Arc::new(settings);
                *guard = Some((Arc::clone(&settings), Instant::now() + self.ttl));
                settings
            }
            Err(e) => {
                warn!(error = %e, "failed to load settings; serving last known values");
                // Leave the expiry untouched so the next request retries.
                guard
                    .as_ref()
                    .map_or_else(|| Arc::new(Settings::default()), |(s, _)| Arc::clone(s))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use gglib_core::RepositoryError;

    /// Repository that counts loads and can be switched to failing.
    #[derive(Debug, Default)]
    struct CountingRepo {
        loads: AtomicUsize,
        context_size: AtomicUsize,
        fail: std::sync::atomic::AtomicBool,
    }

    impl CountingRepo {
        fn loads(&self) -> usize {
            self.loads.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl SettingsRepository for CountingRepo {
        async fn load(&self) -> Result<Settings, RepositoryError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            if self.fail.load(Ordering::SeqCst) {
                return Err(RepositoryError::Storage("boom".to_owned()));
            }
            Ok(Settings {
                default_context_size: Some(self.context_size.load(Ordering::SeqCst) as u64),
                ..Settings::default()
            })
        }

        async fn save(&self, _settings: &Settings) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    /// The point of the cache: repeated requests inside one window must not
    /// each hit the database.
    #[tokio::test]
    async fn repeated_reads_within_the_window_load_once() {
        let repo = Arc::new(CountingRepo::default());
        let cache = SettingsCache::with_ttl(Arc::clone(&repo) as _, Duration::from_secs(60));

        for _ in 0..10 {
            let _ = cache.get().await;
        }

        assert_eq!(repo.loads(), 1);
    }

    /// A write from another process is picked up once the window expires —
    /// the property that makes a TTL the right mechanism here.
    #[tokio::test]
    async fn a_write_is_observed_after_the_window_expires() {
        let repo = Arc::new(CountingRepo::default());
        let cache = SettingsCache::with_ttl(Arc::clone(&repo) as _, Duration::from_millis(20));

        assert_eq!(cache.get().await.default_context_size, Some(0));

        // Simulate an out-of-process edit.
        repo.context_size.store(4096, Ordering::SeqCst);
        assert_eq!(
            cache.get().await.default_context_size,
            Some(0),
            "still inside the window"
        );

        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(cache.get().await.default_context_size, Some(4096));
    }

    /// A repository failure must degrade to the last good snapshot rather than
    /// dropping settings — losing them mid-flight would silently change the
    /// sampling applied to a request.
    #[tokio::test]
    async fn a_failed_refresh_serves_the_last_good_snapshot() {
        let repo = Arc::new(CountingRepo::default());
        repo.context_size.store(4096, Ordering::SeqCst);
        let cache = SettingsCache::with_ttl(Arc::clone(&repo) as _, Duration::from_millis(20));

        assert_eq!(cache.get().await.default_context_size, Some(4096));

        repo.fail.store(true, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(30)).await;

        assert_eq!(cache.get().await.default_context_size, Some(4096));
    }

    /// Failing before any successful load has nothing to fall back on, so it
    /// yields defaults rather than panicking or erroring the request.
    #[tokio::test]
    async fn a_failure_with_no_prior_snapshot_yields_defaults() {
        let repo = Arc::new(CountingRepo::default());
        repo.fail.store(true, Ordering::SeqCst);
        let cache = SettingsCache::new(Arc::clone(&repo) as _);

        assert_eq!(*cache.get().await, Settings::default());
    }

    /// A burst arriving on an expired snapshot must collapse into one query,
    /// not one per caller.
    #[tokio::test]
    async fn concurrent_reads_on_an_expired_snapshot_are_single_flight() {
        let repo = Arc::new(CountingRepo::default());
        let cache = Arc::new(SettingsCache::with_ttl(
            Arc::clone(&repo) as _,
            Duration::from_secs(60),
        ));

        let handles: Vec<_> = (0..16)
            .map(|_| {
                let cache = Arc::clone(&cache);
                tokio::spawn(async move { cache.get().await })
            })
            .collect();
        for handle in handles {
            handle.await.expect("task completes");
        }

        assert_eq!(repo.loads(), 1);
    }
}
