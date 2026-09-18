//! The loop guard's batched writer: the [`LoopGuardTripSink`] the proxy
//! records into, and the task that turns what it records into rows.
//!
//! Recording happens on the request path, so nothing here waits for the
//! database there. A decision goes onto a bounded queue with `try_send`; a
//! scanned request bumps an in-memory count for its day, model and mode. A
//! background task drains both into one transaction — when a batch fills, on a
//! timer, and once more when it is asked to stop — and prunes what the log no
//! longer keeps in the same transaction.
//!
//! What cannot be kept is counted, not waited for: a full queue drops the
//! decision, a stopped writer drops a decision or a scan, and a flush the
//! database refuses loses its batch. [`LoopGuardTripWriter::lost`] says how
//! many; a warning, carrying the reason, says so at most once a minute, and
//! stopping says the total once more. A failed flush never ends the task.
//!
//! The losses are not symmetric, and the asymmetry leans the wrong way for a
//! criterion read from zeros: a dropped decision's scan was already counted,
//! so that day reads as fewer trips over the same denominator. That is why the
//! queue holds 1024 decisions between flushes, and why every loss is counted
//! and said: at most once a minute as it happens, and in total at a graceful
//! stop. A loss after the last warning that ends in a forced exit is not said.
//!
//! The writer is owned by whoever owns the database — the daemon's bootstrap —
//! rather than by a proxy run, so it outlives a proxy that is stopped and
//! started again, and its last flush is part of the daemon's graceful
//! teardown. A forced exit — either watchdog, a crash, a kill — skips that
//! flush and loses what the writer held: at most one timer period of scans,
//! and whatever decisions were queued.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sqlx::SqlitePool;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Instant, interval_at};
use tracing::warn;

use gglib_core::LoopGuardMode;
use gglib_core::domain::loop_guard_log::{
    GGLIB_VERSION, LOOP_GUARD_LOG_RETENTION_DAYS, LoopGuardTripEvent, bounded_model_name, epoch_day,
};
use gglib_core::ports::LoopGuardTripSink;

use crate::repositories::sqlite_loop_guard_trip_log::{ScanKey, prune, write_batch};

/// How the writer queues, flushes and prunes. [`Default`] is what the daemon
/// runs; the fields exist so a test can make a queue small, a timer fast or a
/// cap reachable.
#[derive(Debug, Clone, Copy)]
pub struct TripWriterLimits {
    /// Decisions the queue holds before it drops one.
    pub capacity: usize,
    /// Decisions that make a flush happen without waiting for the timer.
    pub batch: usize,
    /// How often whatever is queued or counted is flushed.
    pub period: Duration,
    /// How many days the log keeps.
    pub retention_days: u32,
    /// How many trip rows the log keeps before whole old days are dropped.
    pub row_cap: u32,
}

impl Default for TripWriterLimits {
    fn default() -> Self {
        Self {
            capacity: 1024,
            batch: 256,
            period: Duration::from_secs(5),
            retention_days: LOOP_GUARD_LOG_RETENTION_DAYS,
            row_cap: 50_000,
        }
    }
}

/// What the recording side and the task both touch.
#[derive(Default)]
struct Shared {
    scans: Mutex<HashMap<ScanKey, u64>>,
    lost: AtomicU64,
    last_warned_secs: AtomicU64,
}

impl Shared {
    /// Count `n` records that will never be written, and say so at most once a
    /// minute.
    fn lose(&self, n: u64, why: &str) {
        let total = self.lost.fetch_add(n, Ordering::Relaxed) + n;
        let now = now_secs();
        let last = self.last_warned_secs.load(Ordering::Relaxed);
        if now.saturating_sub(last) >= 60
            && self
                .last_warned_secs
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            warn!(
                lost = total,
                reason = why,
                "loop guard log: records dropped (said at most once a minute)"
            );
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// The loop guard's batched writer. See the module docs.
pub struct LoopGuardTripWriter {
    trips: mpsc::Sender<LoopGuardTripEvent>,
    shared: Arc<Shared>,
    stop: Mutex<Option<oneshot::Sender<()>>>,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl LoopGuardTripWriter {
    /// Start a writer over `pool` with the daemon's limits. Must be called
    /// from inside a Tokio runtime.
    pub fn spawn(pool: SqlitePool) -> Arc<Self> {
        Self::spawn_with(pool, TripWriterLimits::default())
    }

    /// Start a writer over `pool` with `limits`. A capacity or batch of zero
    /// is taken as one: a batch of zero would read as every sender gone.
    pub fn spawn_with(pool: SqlitePool, limits: TripWriterLimits) -> Arc<Self> {
        let limits = TripWriterLimits {
            capacity: limits.capacity.max(1),
            batch: limits.batch.max(1),
            ..limits
        };
        let (trips, queue) = mpsc::channel(limits.capacity);
        let shared = Arc::new(Shared::default());
        let (stop, stopped) = oneshot::channel();
        let task = tokio::spawn(run(pool, queue, Arc::clone(&shared), stopped, limits));
        Arc::new(Self {
            trips,
            shared,
            stop: Mutex::new(Some(stop)),
            task: Mutex::new(Some(task)),
        })
    }

    /// Records lost since the writer started: decisions dropped on a full or
    /// stopped queue, scans recorded after the stop, and decisions and scans
    /// in a flush the database refused.
    pub fn lost(&self) -> u64 {
        self.shared.lost.load(Ordering::Relaxed)
    }

    /// Stop: drain the queue and the day counts, write them, and return once
    /// they are written. A second call returns at once — before the first has
    /// finished, if the two race; the daemon has one caller.
    pub async fn shutdown(&self) {
        if let Some(stop) = lock(&self.stop).take() {
            // An `Err` means the task has already ended; there is nothing to
            // tell it.
            let _ = stop.send(());
        }
        let task = lock(&self.task).take();
        let Some(task) = task else {
            return;
        };
        if let Err(e) = task.await {
            warn!("loop guard log: the writer task ended abnormally: {e}");
        }
        // Said once more on the way out, whatever the warnings above were
        // rate-limited to: the total is what a reader of the log needs.
        let lost = self.lost();
        if lost > 0 {
            warn!(lost, "loop guard log: records lost while this writer ran");
        }
    }
}

impl LoopGuardTripSink for LoopGuardTripWriter {
    fn record_trip(&self, event: LoopGuardTripEvent) {
        match self.trips.try_send(event) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => self.shared.lose(1, "the queue is full"),
            Err(TrySendError::Closed(_)) => self.shared.lose(1, "the writer has stopped"),
        }
    }

    fn record_scan(&self, model_name: &str, mode: LoopGuardMode, at_secs: u64) {
        let key = (epoch_day(at_secs), bounded_model_name(model_name), mode);
        let mut scans = lock(&self.shared.scans);
        // Once the task has stopped, nothing will flush the counts again: a
        // scan kept here would be lost without being counted as lost. Checked
        // under the lock the stop's last flush takes the counts with, and the
        // stop closes the queue before it takes them, so a scan either makes
        // that flush or is counted as lost — never neither.
        if self.trips.is_closed() {
            drop(scans);
            self.shared.lose(1, "the writer has stopped");
            return;
        }
        *scans.entry(key).or_insert(0) += 1;
    }
}

/// The task: prune once, then flush on a full batch, on the timer, and once
/// more on the way out.
async fn run(
    pool: SqlitePool,
    mut queue: mpsc::Receiver<LoopGuardTripEvent>,
    shared: Arc<Shared>,
    mut stopped: oneshot::Receiver<()>,
    limits: TripWriterLimits,
) {
    let mut batch = Vec::with_capacity(limits.batch);
    if let Err(e) = write(&pool, &batch, &HashMap::new(), limits).await {
        warn!("loop guard log: the pruning at start failed: {e}");
    }
    // `interval_at`, not `interval`: an interval's first tick is immediate, and
    // a flush racing the first records is how a test of the stop path passes
    // for the wrong reason.
    let mut timer = interval_at(Instant::now() + limits.period, limits.period);
    loop {
        tokio::select! {
            // Stop first. Not for correctness — the stop arm drains the queue
            // itself, so nothing queued is lost either way — but so that a stop
            // is acted on the moment it is asked for.
            biased;
            _ = &mut stopped => {
                queue.close();
                while let Ok(event) = queue.try_recv() {
                    batch.push(event);
                }
                flush(&pool, &shared, &mut batch, limits).await;
                return;
            }
            received = queue.recv_many(&mut batch, limits.batch) => {
                if received == 0 {
                    // Every sender is gone: the writer was dropped unstopped.
                    flush(&pool, &shared, &mut batch, limits).await;
                    return;
                }
                if batch.len() >= limits.batch {
                    flush(&pool, &shared, &mut batch, limits).await;
                }
            }
            _ = timer.tick() => flush(&pool, &shared, &mut batch, limits).await,
        }
    }
}

/// Write what is queued and counted, if anything is. A refusal loses the batch
/// and is counted; the task goes on.
async fn flush(
    pool: &SqlitePool,
    shared: &Shared,
    batch: &mut Vec<LoopGuardTripEvent>,
    limits: TripWriterLimits,
) {
    let scans = std::mem::take(&mut *lock(&shared.scans));
    if batch.is_empty() && scans.is_empty() {
        return;
    }
    if let Err(e) = write(pool, batch, &scans, limits).await {
        let trips = u64::try_from(batch.len()).unwrap_or(u64::MAX);
        shared.lose(
            trips + scans.values().sum::<u64>(),
            &format!("a flush failed and its batch is lost: {e}"),
        );
    }
    batch.clear();
}

/// One transaction: the rows, then the pruning.
async fn write(
    pool: &SqlitePool,
    trips: &[LoopGuardTripEvent],
    scans: &HashMap<ScanKey, u64>,
    limits: TripWriterLimits,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    write_batch(&mut tx, GGLIB_VERSION, trips, scans).await?;
    prune(&mut tx, now_secs(), limits.retention_days, limits.row_cap).await?;
    tx.commit().await
}

#[cfg(test)]
#[path = "loop_guard_trip_writer_tests.rs"]
mod tests;
