#![doc = include_str!("README.md")]
mod ending;
mod enqueue;
mod group_completion;
mod meter;
mod paths;
mod publish;
mod running;
mod shard_group_tracker;
mod worker;

#[cfg(test)]
mod duplicate_guard_tests;
#[cfg(test)]
mod group_registration_tests;
#[cfg(test)]
mod projector_group_tests;
#[cfg(test)]
mod runner_tests;
#[cfg(test)]
mod test_support;

use crate::queue::ShardGroupId;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use indexmap::IndexMap;

use tokio::sync::{Mutex, Notify, RwLock, watch};
use tokio_util::sync::CancellationToken;

use gglib_core::download::{
    DownloadError, DownloadEvent, DownloadId, DownloadOutcome, DownloadPhase, QueueSnapshot,
    ShardInfo, download_title,
};
use gglib_core::events::AppEvent;
use gglib_core::ports::{
    AppEventEmitter, DownloadManagerConfig, DownloadManagerPort, HfClientPort, ModelRegistrarPort,
    QuantizationResolver, ResolvedFile,
};

use crate::executor::known_size;
use crate::quant_selector::QuantizationSelector;
use crate::queue::{DownloadQueue, QueuedItem};
use crate::resolver::HfQuantizationResolver;

pub(crate) use meter::GroupMeter;
use shard_group_tracker::{GroupMetadata, ShardGroupTracker};

pub(crate) use paths::DownloadDestination;
pub(crate) use worker::{CompletedJob, DownloadJob, ProgressUpdate, WorkerDeps};

/// How often a download's meter samples the file being fetched and a
/// snapshot is published.
///
/// Speed and ETA smoothing live in the meter, not here; this is purely
/// the display cadence. The GUI does not re-throttle on top of it.
pub(crate) const PROGRESS_TICK: Duration = Duration::from_millis(250);

/// Wrap a download event for the application-wide emitter.
///
/// Downloads reach every surface as `AppEvent::Download`; this is the whole
/// of the translation, and the only place in the crate that performs it.
const fn app_event(event: DownloadEvent) -> AppEvent {
    AppEvent::Download { event }
}

/// Lease ID for tracking active downloads.
///
/// Prevents stale finalize commits when a download is cancelled
/// or replaced while running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LeaseId(u64);

/// State for an active download.
struct ActiveJob {
    /// Unique lease for this execution.
    lease: LeaseId,
    /// Cancellation token.
    cancel: CancellationToken,
    /// The file being fetched, as it came off the queue.
    item: QueuedItem,
    /// What the download is doing: fetching this file, or, once its last
    /// file is in, being finalized and registered.
    phase: DownloadPhase,
}

// =============================================================================
// Queue Run State (for completion tracking)
// =============================================================================

use gglib_core::download::CompletionKind;

/// Aggregated completion data for a single artifact (across all attempts).
#[derive(Debug, Clone)]
struct CompletionAggregate {
    /// Display name from the first attempt.
    display_name: String,
    /// Download IDs from all attempts.
    download_ids: Vec<DownloadId>,
    /// Total number of attempts (includes retries).
    total_attempts: u32,
    /// Number of successful completions.
    success_count: u32,
    /// Number of failed attempts.
    failure_count: u32,
    /// Number of cancellations.
    cancelled_count: u32,
    /// Result of the last attempt.
    last_result: CompletionKind,
    /// Timestamp of last attempt (milliseconds since epoch).
    last_attempt_ms: u64,
}

impl CompletionAggregate {
    /// Create a new aggregate for the first attempt.
    fn new(
        display_name: String,
        download_id: DownloadId,
        kind: CompletionKind,
        timestamp_ms: u64,
    ) -> Self {
        let (success_count, failure_count, cancelled_count) = match kind {
            CompletionKind::Downloaded | CompletionKind::AlreadyPresent => (1, 0, 0),
            CompletionKind::Failed => (0, 1, 0),
            CompletionKind::Cancelled => (0, 0, 1),
        };

        Self {
            display_name,
            download_ids: vec![download_id],
            total_attempts: 1,
            success_count,
            failure_count,
            cancelled_count,
            last_result: kind,
            last_attempt_ms: timestamp_ms,
        }
    }

    /// Record an additional attempt.
    fn record_attempt(
        &mut self,
        download_id: &DownloadId,
        kind: CompletionKind,
        timestamp_ms: u64,
    ) {
        self.download_ids.push(download_id.clone());
        self.total_attempts += 1;
        self.last_attempt_ms = timestamp_ms;
        self.last_result = kind;

        match kind {
            CompletionKind::Downloaded | CompletionKind::AlreadyPresent => {
                self.success_count += 1;
            }
            CompletionKind::Failed => self.failure_count += 1,
            CompletionKind::Cancelled => self.cancelled_count += 1,
        }
    }
}

/// State for tracking a queue run (from busy→drained transition).
#[derive(Debug)]
struct QueueRunState {
    /// Unique identifier for this run.
    run_id: uuid::Uuid,
    /// Start time (milliseconds since epoch).
    started_at_ms: u64,
    /// Aggregated completions keyed by `CompletionKey` (insertion order preserved).
    completions: IndexMap<gglib_core::download::CompletionKey, CompletionAggregate>,
}

impl QueueRunState {
    /// Create a new queue run.
    fn new() -> Self {
        use std::time::SystemTime;

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();

        Self {
            run_id: uuid::Uuid::new_v4(),
            started_at_ms: now.as_millis().try_into().unwrap_or(0),
            completions: IndexMap::new(),
        }
    }

    /// Record a completion (creates or updates aggregate).
    fn record_completion(
        &mut self,
        key: &gglib_core::download::CompletionKey,
        download_id: &DownloadId,
        display_name: &str,
        kind: CompletionKind,
        completed_at_ms: u64,
    ) {
        self.completions
            .entry(key.clone())
            .and_modify(|agg| agg.record_attempt(download_id, kind, completed_at_ms))
            .or_insert_with(|| {
                CompletionAggregate::new(
                    display_name.to_string(),
                    download_id.clone(),
                    kind,
                    completed_at_ms,
                )
            });
    }
}

/// Dependencies for creating a download manager.
///
/// This struct bundles all the ports and configuration needed
/// to construct a `DownloadManagerImpl`.
pub struct DownloadManagerDeps {
    /// Port for registering completed downloads as models.
    pub model_registrar: Arc<dyn ModelRegistrarPort>,
    /// Port for `HuggingFace` API access.
    pub hf_client: Arc<dyn HfClientPort>,
    /// Sink for the application events downloads produce.
    pub event_emitter: Arc<dyn AppEventEmitter>,
    /// Configuration for the download manager.
    pub config: DownloadManagerConfig,
}

/// Build a download manager from its dependencies.
///
/// Returns an implementation of `DownloadManagerPort` that can be
/// stored as `Arc<dyn DownloadManagerPort>` in adapters.
pub fn build_download_manager(deps: DownloadManagerDeps) -> DownloadManagerImpl {
    DownloadManagerImpl::new(
        deps.model_registrar,
        deps.hf_client,
        deps.event_emitter,
        deps.config,
    )
}

/// Concrete implementation of the download manager.
///
/// Produced by [`build_download_manager`] and consumed as
/// `Arc<dyn DownloadManagerPort>`; it is not nameable outside this crate.
///
/// Lock order: `publish` → `queue` → `active` → `shard_tracker` → `meters`.
/// A task takes them in that order and never the other way; `meters` is a
/// std mutex, never held across an await. `current_run` is taken under
/// `publish` when a run starts or is summed up, and under `queue` when a
/// download's ending is recorded in it; nothing else is taken while it is
/// held.
pub struct DownloadManagerImpl {
    /// Model registrar for completed downloads.
    model_registrar: Arc<dyn ModelRegistrarPort>,
    /// Event emitter for download events.
    event_emitter: Arc<dyn AppEventEmitter>,
    /// `HuggingFace` client for fetching model metadata (e.g. tags at registration time).
    hf_client: Arc<dyn HfClientPort>,
    /// File resolver, shared with [`QuantizationSelector`].
    resolver: Arc<HfQuantizationResolver>,
    /// Quantization selector for choosing best quantization.
    selector: QuantizationSelector,
    /// Queue state (protected by `RwLock` for async access).
    queue: RwLock<DownloadQueue>,
    /// Configuration.
    config: DownloadManagerConfig,
    /// The snapshot revision, and the lock every snapshot is built and sent
    /// under. See `publish.rs`.
    publish: Mutex<u64>,
    /// Active downloads (keyed by download ID).
    active: Mutex<HashMap<DownloadId, ActiveJob>>,
    /// Shard group tracker for coordinating multi-shard downloads.
    shard_tracker: Mutex<ShardGroupTracker>,
    /// Counter for generating lease IDs.
    lease_counter: AtomicU64,
    /// Notifier for waking the runner when work is available.
    queue_notify: Notify,
    /// Whether the runner has been started (never reset for long-lived runner).
    runner_started: AtomicBool,
    /// Current queue run state (None when drained).
    current_run: Mutex<Option<QueueRunState>>,
    /// Previous drain state for transition detection.
    prev_is_drained: Mutex<bool>,
    /// File entries with OIDs for each download (keyed by download ID).
    file_entries_map: Mutex<HashMap<String, Vec<ResolvedFile>>>,
    /// One meter per download that has started, kept from file to file so its
    /// bytes and speed run on, until the download ends.
    meters: std::sync::Mutex<HashMap<DownloadId, GroupMeter>>,
}

impl DownloadManagerImpl {
    /// Create a new download manager.
    fn new(
        model_registrar: Arc<dyn ModelRegistrarPort>,
        hf_client: Arc<dyn HfClientPort>,
        event_emitter: Arc<dyn AppEventEmitter>,
        config: DownloadManagerConfig,
    ) -> Self {
        let resolver = Arc::new(HfQuantizationResolver::new(Arc::clone(&hf_client)));
        let selector =
            QuantizationSelector::new(Arc::clone(&resolver) as Arc<dyn QuantizationResolver>);

        Self {
            model_registrar,
            event_emitter,
            hf_client,
            resolver,
            selector,
            queue: RwLock::new(DownloadQueue::new(config.max_queue_size)),
            config,
            publish: Mutex::new(0),
            active: Mutex::new(HashMap::new()),
            shard_tracker: Mutex::new(ShardGroupTracker::new()),
            lease_counter: AtomicU64::new(0),
            queue_notify: Notify::new(),
            runner_started: AtomicBool::new(false),
            current_run: Mutex::new(None),
            prev_is_drained: Mutex::new(true), // Start in drained state
            file_entries_map: Mutex::new(HashMap::new()),
            meters: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Record a completion in the current queue run (if active).
    async fn record_completion_in_run(&self, item: &QueuedItem, kind: CompletionKind) {
        use std::time::SystemTime;

        let timestamp_ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(0);

        // The summary names the download as its row did
        let display_name = download_title(&item.id);

        if let Some(run) = self.current_run.lock().await.as_mut() {
            tracing::debug!(
                target: "gglib.download",
                key = %item.completion_key,
                kind = ?kind,
                "Recording completion in run"
            );
            run.record_completion(
                &item.completion_key,
                &item.id,
                &display_name,
                kind,
                timestamp_ms,
            );
        } else {
            tracing::warn!(
                target: "gglib.download",
                key = %item.completion_key,
                "Completion occurred but no active run to record to"
            );
        }
    }

    /// Get access to the model registrar for direct registration.
    pub fn model_registrar(&self) -> &Arc<dyn ModelRegistrarPort> {
        &self.model_registrar
    }

    /// Ensure the runner is started.
    ///
    /// This method is idempotent: calling it multiple times has no effect
    /// after the first call. The runner runs for the lifetime of the manager.
    pub fn ensure_runner(self: &Arc<Self>) {
        if self
            .runner_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            tokio::spawn(Arc::clone(self).run_loop());
        }
    }

    /// The main runner loop.
    ///
    /// This runs for the lifetime of the manager, waiting on `queue_notify`
    /// when there's no work and draining the queue when there is.
    async fn run_loop(self: Arc<Self>) {
        loop {
            // Try to get the next job
            if let Some((lease, item, cancel, progress_tx)) = self.next_job().await {
                // The meter task samples this file into its download's meter
                // and publishes a snapshot on every tick.
                let finished = CancellationToken::new();
                let meter_task = tokio::spawn(Arc::clone(&self).run_meter(
                    item.id.clone(),
                    progress_tx.subscribe(),
                    cancel.clone(),
                    finished.clone(),
                ));

                // Create worker deps and job
                let deps = WorkerDeps {
                    config: self.config.clone(),
                };

                let files = Self::extract_files(&item);
                let destination =
                    DownloadDestination::plan(&self.config.models_directory, &item.id, files);

                // Save primary file path before destination is moved into the job.
                let primary_file_path = destination.primary_path();

                // Remove corrupt cached files before hf_hub_download sees them.
                Self::remove_corrupt_cached_file(&item, primary_file_path.as_ref());

                let job = DownloadJob {
                    id: item.id.clone(),
                    destination,
                    revision: item.revision.clone(),
                    cancel: cancel.clone(),
                    progress_tx,
                    expected_size: known_size(item.shard_info.as_ref().and_then(|s| s.file_size)),
                };

                // Run the worker
                let result = worker::run_job(job, &deps).await;

                // Tell the meter task the worker is done, then actually join
                // it, so the meter has the file's final count before the
                // file is finalized.
                finished.cancel();
                let _ = meter_task.await;

                // Finalize the job with item context for shard tracking
                self.finalize_job(&item, lease, result).await;

                // Notify to keep draining if more work
                self.queue_notify.notify_one();
            } else {
                // No work, wait for notification
                self.queue_notify.notified().await;
            }
        }
    }

    /// Emit one download event through the application-wide emitter.
    fn emit(&self, event: DownloadEvent) {
        self.event_emitter.emit(app_event(event));
    }

    /// Validate a cached GGUF file and delete it if corrupt.
    ///
    /// A previous interrupted download may have left a truncated file that
    /// `hf_hub_download` would silently accept as "cached".
    fn remove_corrupt_cached_file(
        item: &QueuedItem,
        primary_file_path: Option<&std::path::PathBuf>,
    ) {
        let Some(path) = primary_file_path else {
            return;
        };
        if !path.exists() {
            return;
        }

        let expected_size = known_size(item.shard_info.as_ref().and_then(|s| s.file_size));
        if expected_size.is_none() {
            tracing::warn!(
                id = %item.id,
                path = %path.display(),
                "No expected file size from HF metadata — \
                 size validation will be skipped"
            );
        }
        if let Err(reason) = validate_cached_gguf(path, expected_size) {
            tracing::warn!(
                id = %item.id,
                path = %path.display(),
                reason,
                "Cached file is corrupt — deleting for re-download"
            );
            let _ = std::fs::remove_file(path);
        }
    }

    /// Get the next job from the queue.
    ///
    /// Returns `None` if the queue is empty.
    ///
    /// The queue guard is held from the dequeue until the file is in
    /// `active`, so a reader holding the queue never finds the file in
    /// neither place. The download's meter is made under it too, so a row
    /// that is running always has one. The guard is dropped before the
    /// snapshot is published, which reads the queue itself.
    /// Lock order: queue → active → meters.
    async fn next_job(
        &self,
    ) -> Option<(
        LeaseId,
        QueuedItem,
        CancellationToken,
        watch::Sender<ProgressUpdate>,
    )> {
        // Acquire queue lock first, then active lock
        let mut queue = self.queue.write().await;
        let item = queue.dequeue()?;

        // Mint a new lease
        let lease = LeaseId(self.lease_counter.fetch_add(1, Ordering::Relaxed));

        // Create cancellation token and progress channel
        let cancel = CancellationToken::new();
        let (progress_tx, _) = watch::channel(ProgressUpdate::default());

        // Insert into active map
        {
            let mut active = self.active.lock().await;
            active.insert(
                item.id.clone(),
                ActiveJob {
                    lease,
                    cancel: cancel.clone(),
                    item: item.clone(),
                    phase: DownloadPhase::Downloading,
                },
            );
        }
        // The first file of a download starts its meter; a later file finds
        // the one its earlier files fed.
        self.meters().entry(item.id.clone()).or_insert_with(|| {
            GroupMeter::for_group(item.shard_info.as_ref(), std::time::Instant::now())
        });
        drop(queue);

        // Publish the queue with the file now active
        self.publish().await;

        Some((lease, item, cancel, progress_tx))
    }

    /// Finalize a job after it completes or fails.
    ///
    /// Verifies the lease first (to prevent double-finalization), then runs
    /// `handle_job_result` (which includes model registration) while the item
    /// is **still present** in the active map. Only after that completes is the
    /// item removed from `active` and the snapshot published.
    ///
    /// This ordering is critical: the CLI interactive monitor exits once
    /// nothing is running or waiting. Removing the item before registration
    /// completes would allow the CLI to exit mid-insert, dropping the tokio
    /// runtime and silently losing the DB row.
    ///
    /// Cancel wins when it is in time. The token is read once, as the worker
    /// returns: cancelled by then, the download ends cancelled whatever the
    /// worker answered, and a landed file is neither counted nor registered.
    /// A later cancel still ends a download with files to come (`settle`),
    /// but not one this file has ended, its last file being registered or a
    /// file that failed: that one ends completed or failed all the same.
    ///
    /// The file then leaves `active`, and the download ends when this file
    /// ended it (`ending.rs`).
    ///
    /// Locks: registration takes `active`, the tracker and the publish mutex
    /// one at a time. The ending takes queue → active → tracker → meters,
    /// and the snapshot published after it takes them all in order.
    async fn finalize_job(
        &self,
        item: &QueuedItem,
        lease: LeaseId,
        result: Result<CompletedJob, DownloadError>,
    ) {
        // Step 1 — verify lease (a stale or duplicate finalize); stay active.
        let Some(cancelled) = self.cancelled_under(&item.id, lease).await else {
            tracing::debug!(id = %item.id, "Ignoring stale finalize (lease mismatch)");
            return;
        };

        // Step 2 — register while still active: no monitor races past it.
        let ended = if cancelled {
            Some(DownloadOutcome::Cancelled)
        } else {
            self.handle_job_result(item, result).await
        };

        // Step 3 — now safe to remove from active map and notify watchers.
        self.settle(&item.id, ended).await;
    }

    /// Whether the job holding `lease` was cancelled. `None` when the stored
    /// lease is another, or there is none. The entry is not removed.
    async fn cancelled_under(&self, id: &DownloadId, lease: LeaseId) -> Option<bool> {
        let active = self.active.lock().await;
        let job = active.get(id).filter(|job| job.lease == lease);
        let cancelled = job.map(|job| job.cancel.is_cancelled());
        drop(active);
        cancelled
    }

    /// Handle the result of a completed job.
    ///
    /// Returns how the download ended, when this file ended it: `None` while
    /// it has files still to come.
    async fn handle_job_result(
        &self,
        item: &QueuedItem,
        result: Result<CompletedJob, DownloadError>,
    ) -> Option<DownloadOutcome> {
        match result {
            Ok(completed) => self.handle_success(item, completed).await,
            Err(DownloadError::Cancelled) => Some(DownloadOutcome::Cancelled),
            Err(e) => {
                tracing::warn!(id = %item.id, error = %e, "Download failed");
                Some(DownloadOutcome::Failed {
                    error: e.to_string(),
                })
            }
        }
    }

    /// Handle successful download completion.
    async fn handle_success(
        &self,
        item: &QueuedItem,
        completed: CompletedJob,
    ) -> Option<DownloadOutcome> {
        // The file's bytes join those of the files before it.
        if let Some(meter) = self.meters().get_mut(&item.id) {
            meter.file_done();
        }

        if let Some(group_id) = &item.group_id {
            if let Some(shard_info) = &item.shard_info {
                return self
                    .handle_shard_completion(item, group_id, shard_info, completed)
                    .await;
            }
        }

        // Single-file download - register immediately
        Some(self.handle_single_file_completion(item, completed).await)
    }

    /// Handle completion of a shard in a multi-shard download.
    async fn handle_shard_completion(
        &self,
        item: &QueuedItem,
        group_id: &ShardGroupId,
        shard_info: &ShardInfo,
        completed: CompletedJob,
    ) -> Option<DownloadOutcome> {
        // Retrieve the group's file entries, with OIDs and roles, from map
        let file_entries = {
            let map = self.file_entries_map.lock().await;
            map.get(&item.id.to_string()).cloned().unwrap_or_default()
        };

        // The same identity for every file of the group, the projector's too
        let metadata = GroupMetadata::of(&completed, file_entries);

        let group_complete = {
            let mut tracker = self.shard_tracker.lock().await;
            tracker.on_shard_done(
                group_id,
                shard_info.shard_index,
                completed.primary_path.clone(),
                metadata.expected_files(shard_info),
                &metadata,
            )
        };

        // Only register and emit event if group is complete
        if let Some(complete) = group_complete {
            tracing::info!(
                id = %item.id,
                file_count = complete.ordered_paths.len(),
                "All files downloaded, registering model"
            );
            Some(self.register_completed_model(&item.id, complete).await)
        } else {
            tracing::debug!(
                id = %item.id,
                shard = shard_info.shard_index,
                "Shard downloaded, waiting for remaining shards"
            );
            None
        }
    }

    /// Handle completion of a single-file download.
    async fn handle_single_file_completion(
        &self,
        item: &QueuedItem,
        completed: CompletedJob,
    ) -> DownloadOutcome {
        tracing::info!(id = %item.id, "Single-file download completed");

        // Retrieve file entries with OIDs from map
        let file_entries = {
            let map = self.file_entries_map.lock().await;
            map.get(&item.id.to_string()).cloned().unwrap_or_default()
        };

        let complete = shard_group_tracker::GroupComplete {
            ordered_paths: completed.all_paths.clone(),
            metadata: GroupMetadata::of(&completed, file_entries),
        };
        self.register_completed_model(&item.id, complete).await
    }

    /// Register a completed model (all shards downloaded).
    ///
    /// This is the single point of model registration, called only when
    /// all shards in a group are complete (or for single-file downloads).
    /// Returns how the download ended: a model that could not be registered
    /// is a failed download, whatever is on disk.
    async fn register_completed_model(
        &self,
        id: &DownloadId,
        complete: shard_group_tracker::GroupComplete,
    ) -> DownloadOutcome {
        // Phase 1 of finalization: bytes are on disk, we are about to gather
        // metadata (HF tags etc.). The phase is published so every surface
        // reads "Finalizing" instead of a bar frozen at 100%.
        self.set_phase(id, DownloadPhase::Finalizing).await;

        // Fetch HF model tags (nice-to-have metadata). Bounded by a strict
        // 5-second timeout: a stalled/slow connection must never delay the
        // DB insert. On timeout or error we log a warning and proceed with
        // an empty tag list — a failed tag fetch is never a hard failure.
        let hf_tags = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.hf_client.get_model_info(&complete.metadata.repo_id),
        )
        .await
        {
            Ok(Ok(info)) => info.tags,
            Ok(Err(e)) => {
                tracing::warn!(
                    error = %e,
                    repo_id = %complete.metadata.repo_id,
                    "Failed to fetch HF tags during finalization; registering without tags",
                );
                Vec::new()
            }
            Err(_elapsed) => {
                tracing::warn!(
                    repo_id = %complete.metadata.repo_id,
                    "Timed out fetching HF tags (5 s); registering without tags",
                );
                Vec::new()
            }
        };

        // The weights are the model's files; a projector is handed over
        // apart, to be linked.
        let completed = complete.into_completed_download(hf_tags);

        // Phase 2 of finalization: writing the model row to the database.
        self.set_phase(id, DownloadPhase::Registering).await;

        // Register model (soft-fail)
        match self.model_registrar.register_model(&completed).await {
            Ok(registered) => {
                tracing::info!(
                    model_id = registered.model.id,
                    model_name = %registered.model.name,
                    shard_count = group_completion::shard_count(&completed),
                    "Model registered successfully"
                );
                let refusal = registered.projector_refusal.as_deref();
                if let Some(reason) = refusal {
                    tracing::warn!(
                        model_id = registered.model.id,
                        reason,
                        "Projector not linked"
                    );
                }

                // The outcome says so when the projector was not linked
                DownloadOutcome::Completed {
                    message: Some(group_completion::completion_message(&completed, refusal)),
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %completed.primary_path.display(),
                    "Failed to register model - files downloaded but won't appear in library"
                );
                // The files are there and the model is not: the download
                // failed, and says why.
                DownloadOutcome::Failed {
                    error: format!("Registration failed: {e}"),
                }
            }
        }
    }

    /// Extract files from a queued item.
    fn extract_files(item: &QueuedItem) -> Vec<String> {
        item.shard_info.as_ref().map_or_else(
            || {
                tracing::error!(id = %item.id, "BUG: Queue item missing shard_info");
                vec![]
            },
            |shard| vec![shard.filename.clone()],
        )
    }

    /// Handle state transitions between drained and busy queue states.
    #[allow(clippy::cognitive_complexity)]
    async fn handle_drain_transitions(&self, is_drained: bool) {
        let mut prev = self.prev_is_drained.lock().await;

        let was_drained = *prev;
        if was_drained && !is_drained {
            self.start_new_queue_run().await;
        } else if !was_drained && is_drained {
            self.finalize_queue_run().await;
        }

        *prev = is_drained;
    }

    /// Start a new queue run when transitioning from drained to busy.
    async fn start_new_queue_run(&self) {
        *self.current_run.lock().await = Some(QueueRunState::new());
        tracing::info!(target: "gglib.download", "Queue run STARTED");
    }

    /// Finalize and emit the current queue run when transitioning from busy to drained.
    async fn finalize_queue_run(&self) {
        let run = self.current_run.lock().await.take();
        match run {
            Some(run) => {
                tracing::info!(
                    target: "gglib.download",
                    unique_downloaded = run.completions.len(),
                    "Queue run COMPLETED - emitting summary"
                );
                self.emit_queue_run_complete(run);
            }
            None => {
                tracing::warn!(target: "gglib.download", "Queue drained but no run state found");
            }
        }
    }

    /// Emit queue run complete event with summary.
    fn emit_queue_run_complete(&self, run: QueueRunState) {
        use gglib_core::download::QueueRunSummary;

        let completed_at_ms = Self::get_current_timestamp_ms();
        let mut items = Self::build_completion_details(run.completions);
        items.sort_by_key(|b| std::cmp::Reverse(b.last_completed_at_ms));

        let (total_attempts_downloaded, total_attempts_failed, total_attempts_cancelled) =
            Self::calculate_total_attempts(&items);
        let (unique_downloaded, unique_failed, unique_cancelled) =
            Self::calculate_unique_counts(&items);

        let truncated = items.len() > 20;
        items.truncate(20);

        let summary = QueueRunSummary {
            run_id: run.run_id,
            started_at_ms: run.started_at_ms,
            completed_at_ms,
            total_attempts_downloaded,
            total_attempts_failed,
            total_attempts_cancelled,
            unique_models_downloaded: unique_downloaded,
            unique_models_failed: unique_failed,
            unique_models_cancelled: unique_cancelled,
            truncated,
            items,
        };

        tracing::info!(
            run_id = %summary.run_id,
            item_count = summary.items.len(),
            unique_downloaded = summary.unique_models_downloaded,
            unique_failed = summary.unique_models_failed,
            total_attempts = summary.total_attempts(),
            "Queue run complete"
        );

        self.emit(DownloadEvent::QueueRunComplete { summary });
    }

    fn get_current_timestamp_ms() -> u64 {
        use std::time::SystemTime;

        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(0)
    }

    fn build_completion_details(
        completions: indexmap::IndexMap<gglib_core::download::CompletionKey, CompletionAggregate>,
    ) -> Vec<gglib_core::download::CompletionDetail> {
        use gglib_core::download::{AttemptCounts, CompletionDetail};

        completions
            .into_iter()
            .map(|(key, agg)| {
                let attempt_counts = AttemptCounts {
                    downloaded: agg.success_count,
                    failed: agg.failure_count,
                    cancelled: agg.cancelled_count,
                };

                CompletionDetail {
                    key,
                    display_name: agg.display_name,
                    download_ids: agg.download_ids,
                    attempt_counts,
                    last_result: agg.last_result,
                    last_completed_at_ms: agg.last_attempt_ms,
                }
            })
            .collect()
    }

    fn calculate_total_attempts(
        items: &[gglib_core::download::CompletionDetail],
    ) -> (u32, u32, u32) {
        items.iter().fold((0, 0, 0), |(d, f, c), detail| {
            (
                d + detail.attempt_counts.downloaded,
                f + detail.attempt_counts.failed,
                c + detail.attempt_counts.cancelled,
            )
        })
    }

    fn calculate_unique_counts(
        items: &[gglib_core::download::CompletionDetail],
    ) -> (u32, u32, u32) {
        use gglib_core::download::CompletionKind;

        items
            .iter()
            .fold((0, 0, 0), |(d, f, c), detail| match detail.last_result {
                CompletionKind::Downloaded | CompletionKind::AlreadyPresent => (d + 1, f, c),
                CompletionKind::Failed => (d, f + 1, c),
                CompletionKind::Cancelled => (d, f, c + 1),
            })
    }
}

/// GGUF magic number: "GGUF" in little-endian.
const GGUF_MAGIC: [u8; 4] = [0x47, 0x47, 0x55, 0x46];

/// Validate a cached GGUF file before allowing `hf_hub_download` to skip it.
///
/// Returns `Ok(())` if the file looks valid, or `Err(reason)` if it should
/// be deleted and re-downloaded.
///
/// Checks:
/// 1. File size matches the expected size from HF metadata (if known)
/// 2. File starts with the 4-byte GGUF magic number
fn validate_cached_gguf(path: &std::path::Path, expected_size: Option<u64>) -> Result<(), String> {
    use std::io::Read;

    let metadata = std::fs::metadata(path).map_err(|e| format!("cannot stat file: {e}"))?;
    let actual_size = metadata.len();

    // Check size first (cheap)
    if let Some(expected) = expected_size {
        if actual_size != expected {
            return Err(format!(
                "size mismatch: expected {expected} bytes, got {actual_size}"
            ));
        }
    }

    // Check GGUF magic (read only 4 bytes)
    let mut file = std::fs::File::open(path).map_err(|e| format!("cannot open file: {e}"))?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .map_err(|e| format!("cannot read magic bytes: {e}"))?;

    if magic != GGUF_MAGIC {
        return Err(format!(
            "invalid GGUF magic: expected {GGUF_MAGIC:?}, got {magic:?}"
        ));
    }

    Ok(())
}

// =============================================================================
// DownloadManagerPort implementation
// =============================================================================

#[async_trait]
impl DownloadManagerPort for DownloadManagerImpl {
    async fn queue_smart(
        self: Arc<Self>,
        repo_id: String,
        quantization: Option<String>,
    ) -> Result<DownloadId, DownloadError> {
        let id = self.queue_download_smart(&repo_id, quantization).await?;
        self.ensure_runner();
        Ok(id)
    }

    async fn get_queue_snapshot(&self) -> Result<QueueSnapshot, DownloadError> {
        // Built by the builder the event stream is served from, under the
        // same mutex, so this snapshot has its own place in their order.
        let mut revision = self.publish.lock().await;
        Ok(self.build_snapshot(&mut revision).await)
    }

    async fn cancel_download(&self, id: &DownloadId) -> Result<(), DownloadError> {
        if self.stop_download(id).await {
            Ok(())
        } else {
            Err(DownloadError::not_in_queue(id.to_string()))
        }
    }

    async fn cancel_all(&self) -> Result<(), DownloadError> {
        // The one being fetched is told to stop; the rest end here.
        self.stop_all().await;

        // Bounded drain: wait up to 5s for active downloads to actually
        // finalize so we don't return while the Python helper subprocesses
        // are still cleaning up. Without this the CLI would exit with
        // partially-written files and no DB row — exactly the symptom
        // tracked in #466.
        let drain_deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut poll = tokio::time::interval(std::time::Duration::from_millis(50));
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            poll.tick().await;
            if self.active.lock().await.is_empty() {
                break;
            }
            if tokio::time::Instant::now() >= drain_deadline {
                tracing::warn!(
                    "cancel_all drain deadline exceeded; returning while jobs still draining"
                );
                break;
            }
        }

        tracing::info!("Cancelled all downloads");
        Ok(())
    }

    async fn active_count(&self) -> Result<u32, DownloadError> {
        #[allow(clippy::cast_possible_truncation)]
        Ok(self.active.lock().await.len() as u32)
    }

    async fn remove_from_queue(&self, id: &DownloadId) -> Result<(), DownloadError> {
        self.take_off(id).await
    }

    async fn reorder_queue(
        &self,
        id: &DownloadId,
        new_position: u32,
    ) -> Result<u32, DownloadError> {
        let actual_position = {
            let mut queue = self.queue.write().await;
            let running = self.running_id(&queue).await;
            queue.reorder(id, new_position, running.as_ref())?
        };
        tracing::info!(id = %id, position = actual_position, "Reordered download");
        self.publish().await;
        Ok(actual_position)
    }

    async fn set_max_queue_size(&self, size: u32) -> Result<(), DownloadError> {
        self.queue.write().await.set_max_size(size);
        tracing::info!(size = size, "Set max queue size");
        // The snapshot carries the size and whether the queue is full.
        self.publish().await;
        Ok(())
    }
}

// =============================================================================
// Queueing
// =============================================================================

impl DownloadManagerImpl {
    /// Queue a download with smart quantization selection, and answer its
    /// ID. A request for a download already waiting or running answers that
    /// download's ID.
    ///
    /// This queues and does not start the runner: `queue_smart`, the port's
    /// one way in, does both.
    pub async fn queue_download_smart(
        &self,
        repo_id: impl Into<String>,
        quantization: Option<String>,
    ) -> Result<DownloadId, DownloadError> {
        let repo_id = repo_id.into();

        let selection = self
            .selector
            .select(&repo_id, quantization.as_deref())
            .await?;

        let quant_str = selection.quantization.to_string();
        let id = DownloadId::new(&repo_id, Some(&quant_str));

        if selection.auto_selected {
            tracing::info!(
                repo_id = %repo_id,
                selected = %quant_str,
                available = ?selection.available.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "Auto-selected quantization"
            );
        }

        let resolution = self
            .resolver
            .resolve(&repo_id, selection.quantization)
            .await?;

        // `None` is a repeat request, attached to the download already in
        // flight: the same answer, and nothing new to announce.
        if let Some(position) = self.enqueue_group(&id, &resolution).await? {
            tracing::info!(
                id = %id,
                position = position,
                sharded = resolution.is_sharded,
                files = resolution.files.len(),
                "Download queued via queue_download_smart"
            );

            self.queue_notify.notify_one();
            self.publish().await;
        }

        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lease_id_equality() {
        let l1 = LeaseId(1);
        let l2 = LeaseId(1);
        let l3 = LeaseId(2);

        assert_eq!(l1, l2);
        assert_ne!(l1, l3);
    }
}
