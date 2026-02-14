//! Download manager implementation.
//!
//! This module provides the concrete implementation of `DownloadManagerPort`
//! with a long-lived runner, lease-based state management, and clean separation
//! between the worker (core download logic) and bridges (event emission).
//!
//! # Architecture
//!
//! - **Manager**: Orchestrates queue, leases, and worker lifecycle
//! - **Worker**: Executes downloads, writes only to `watch::Sender` (no events)
//! - **Bridge tasks**: Subscribe to watch channels, emit events with rate-limiting
//!
//! # Concurrency Model
//!
//! - Single long-lived runner (never resets `runner_started`)
//! - `Notify` for efficient wake-on-work
//! - Lease tokens prevent stale finalize commits
//! - Lock order: queue → active (consistent everywhere)

mod paths;
mod shard_group_tracker;
mod worker;

use crate::queue::ShardGroupId;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use indexmap::IndexMap;

use gglib_core::utils::shard_filename::base_shard_filename;
use tokio::sync::{Mutex, Notify, RwLock, watch};
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

use gglib_core::download::{
    DownloadError, DownloadEvent, DownloadId, DownloadSummary, QueueSnapshot, ShardInfo,
};
use gglib_core::ports::{
    DownloadEventEmitterPort, DownloadManagerConfig, DownloadManagerPort, DownloadRequest,
    DownloadStateRepositoryPort, HfClientPort, ModelRegistrarPort, QuantizationResolver,
};

use crate::quant_selector::QuantizationSelector;
use crate::queue::{DownloadQueue, QueuedItem};
use crate::resolver::HfQuantizationResolver;

use shard_group_tracker::{GroupMetadata, ShardGroupTracker};

pub use paths::DownloadDestination;
pub use worker::{CompletedJob, DownloadJob, ProgressUpdate, WorkerDeps};

/// EWA smoothing factor for speed calculation (2% of instant speed, 98% of previous).
const EWA_SMOOTHING: f64 = 0.02;

/// Lease ID for tracking active downloads.
///
/// Used to prevent stale finalize commits when a download is cancelled
/// or replaced while running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LeaseId(u64);

/// State for an active download.
struct ActiveJob {
    /// Unique lease for this execution.
    lease: LeaseId,
    /// Cancellation token.
    cancel: CancellationToken,
    /// Progress sender (bridges subscribe to this).
    progress_tx: watch::Sender<ProgressUpdate>,
    /// Shard information if this is a sharded download.
    shard_info: Option<ShardInfo>,
    /// Group ID if this is part of a shard group.
    group_id: Option<String>,
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
pub struct DownloadManagerDeps<R, D, H, E>
where
    R: ModelRegistrarPort + 'static,
    D: DownloadStateRepositoryPort + 'static,
    H: HfClientPort + 'static,
    E: DownloadEventEmitterPort + 'static,
{
    /// Port for registering completed downloads as models.
    pub model_registrar: Arc<R>,
    /// Port for persisting download queue state.
    pub download_repo: Arc<D>,
    /// Port for `HuggingFace` API access.
    pub hf_client: Arc<H>,
    /// Port for emitting download events.
    pub event_emitter: Arc<E>,
    /// Configuration for the download manager.
    pub config: DownloadManagerConfig,
}

/// Build a download manager from its dependencies.
///
/// Returns an implementation of `DownloadManagerPort` that can be
/// stored as `Arc<dyn DownloadManagerPort>` in adapters.
pub fn build_download_manager<R, D, H, E>(
    deps: DownloadManagerDeps<R, D, H, E>,
) -> DownloadManagerImpl
where
    R: ModelRegistrarPort + 'static,
    D: DownloadStateRepositoryPort + 'static,
    H: HfClientPort + 'static,
    E: DownloadEventEmitterPort + 'static,
{
    DownloadManagerImpl::new(
        deps.model_registrar,
        deps.download_repo,
        deps.hf_client,
        deps.event_emitter,
        deps.config,
    )
}

/// Concrete implementation of the download manager.
///
/// This struct is public but adapters should typically use
/// `Arc<dyn DownloadManagerPort>` instead of depending on this type directly.
pub struct DownloadManagerImpl {
    /// Model registrar for completed downloads.
    model_registrar: Arc<dyn ModelRegistrarPort>,
    /// Repository for persisting queue state (reserved for future persistence).
    _download_repo: Arc<dyn DownloadStateRepositoryPort>,
    /// Event emitter for download events.
    event_emitter: Arc<dyn DownloadEventEmitterPort>,
    /// `HuggingFace` client for fetching model metadata.
    hf_client: Arc<dyn HfClientPort>,
    /// File resolver.
    resolver: HfQuantizationResolver,
    /// Quantization selector for choosing best quantization.
    selector: QuantizationSelector,
    /// Queue state (protected by `RwLock` for async access).
    queue: RwLock<DownloadQueue>,
    /// Configuration.
    config: DownloadManagerConfig,
    /// Active downloads (keyed by download ID).
    /// Lock order: always acquire queue lock before active lock.
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
}

impl DownloadManagerImpl {
    /// Create a new download manager.
    fn new<R, D, H, E>(
        model_registrar: Arc<R>,
        download_repo: Arc<D>,
        hf_client: Arc<H>,
        event_emitter: Arc<E>,
        config: DownloadManagerConfig,
    ) -> Self
    where
        R: ModelRegistrarPort + 'static,
        D: DownloadStateRepositoryPort + 'static,
        H: HfClientPort + 'static,
        E: DownloadEventEmitterPort + 'static,
    {
        let hf_client_dyn: Arc<dyn HfClientPort> = hf_client;
        let resolver = HfQuantizationResolver::new(Arc::clone(&hf_client_dyn));
        let resolver_arc: Arc<dyn QuantizationResolver> =
            Arc::new(HfQuantizationResolver::new(Arc::clone(&hf_client_dyn)));
        let selector = QuantizationSelector::new(resolver_arc);

        Self {
            model_registrar,
            _download_repo: download_repo,
            event_emitter: event_emitter as Arc<dyn DownloadEventEmitterPort>,
            hf_client: hf_client_dyn,
            resolver,
            selector,
            queue: RwLock::new(DownloadQueue::new(config.max_queue_size)),
            config,
            active: Mutex::new(HashMap::new()),
            shard_tracker: Mutex::new(ShardGroupTracker::new()),
            lease_counter: AtomicU64::new(0),
            queue_notify: Notify::new(),
            runner_started: AtomicBool::new(false),
            current_run: Mutex::new(None),
            prev_is_drained: Mutex::new(true), // Start in drained state
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

        // Generate display name from completion key
        let display_name = item.completion_key.to_string();

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

    /// Subscribe to progress updates for an active download.
    ///
    /// Returns `None` if the download is not active.
    pub async fn subscribe_progress(
        &self,
        id: &DownloadId,
    ) -> Option<watch::Receiver<ProgressUpdate>> {
        let active = self.active.lock().await;
        active.get(id).map(|job| job.progress_tx.subscribe())
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
            let manager = Arc::clone(self);
            tokio::spawn(async move {
                manager.run_loop().await;
            });
        }
    }

    /// The main runner loop.
    ///
    /// This runs for the lifetime of the manager, waiting on `queue_notify`
    /// when there's no work and draining the queue when there is.
    async fn run_loop(&self) {
        loop {
            // Try to get the next job
            if let Some((lease, item, cancel, progress_tx)) = self.next_job().await {
                // Spawn progress bridge task
                let bridge_handle = self.spawn_progress_bridge(
                    &item.id,
                    item.shard_info.as_ref(),
                    progress_tx.subscribe(),
                    cancel.clone(),
                );

                // Create worker deps and job
                let deps = WorkerDeps {
                    config: self.config.clone(),
                };

                let files = Self::extract_files(&item);
                let destination =
                    DownloadDestination::plan(&self.config.models_directory, &item.id, files);

                // Save primary file path before destination is moved into the job.
                let primary_file_path = destination.primary_path();

                // Pre-download validation: if the file already exists on disk,
                // verify it is a valid GGUF file with the expected size.
                // A previous interrupted download may have left a truncated
                // file that hf_hub_download would silently accept as "cached".
                if let Some(ref path) = primary_file_path {
                    if path.exists() {
                        let expected_size = item.shard_info.as_ref().and_then(|s| s.file_size);
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
                }

                // Clone the progress sender so we can detect cache hits after
                // run_job consumes the original.
                let progress_tx_clone = progress_tx.clone();

                let job = DownloadJob {
                    id: item.id.clone(),
                    destination,
                    revision: item.revision.clone(),
                    cancel: cancel.clone(),
                    progress_tx,
                };

                // Emit started event (include shard info if this is a sharded download)
                if let Some(shard) = &item.shard_info {
                    self.event_emitter.emit(DownloadEvent::started_shard(
                        item.id.to_string(),
                        shard.shard_index,
                        shard.total_shards,
                    ));
                } else {
                    self.event_emitter.emit(DownloadEvent::DownloadStarted {
                        id: item.id.to_string(),
                        shard_index: None,
                        total_shards: None,
                    });
                }

                // Run the worker
                let result = worker::run_job(job, &deps).await;

                // Cache-hit detection: if the download succeeded but no
                // progress callbacks fired (seq == 0), hf_hub_download found
                // the file in cache and returned instantly.  Emit a synthetic
                // 100% progress update so the bridge produces at least one
                // ShardProgress event for the UI.
                if result.is_ok() && progress_tx_clone.borrow().seq == 0 {
                    let file_size = primary_file_path
                        .as_ref()
                        .and_then(|p| std::fs::metadata(p).ok())
                        .map(|m| m.len())
                        .or_else(|| item.shard_info.as_ref().and_then(|s| s.file_size))
                        .unwrap_or(0);

                    progress_tx_clone.send_modify(|state| {
                        state.downloaded = file_size;
                        state.total = file_size;
                        state.seq = 1;
                    });

                    tracing::debug!(
                        id = %item.id,
                        file_size,
                        "File already cached — emitted synthetic 100% progress"
                    );
                }

                // Drop the cloned sender so the bridge sees all senders
                // dropped and can emit its final progress event.
                drop(progress_tx_clone);

                // Wait for bridge to finish (it will exit when sender is dropped)
                drop(bridge_handle);

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

    /// Get the next job from the queue.
    ///
    /// Returns `None` if the queue is empty.
    /// Lock order: queue → active.
    async fn next_job(
        &self,
    ) -> Option<(
        LeaseId,
        QueuedItem,
        CancellationToken,
        watch::Sender<ProgressUpdate>,
    )> {
        // Acquire queue lock first, then active lock
        let item = {
            let mut queue = self.queue.write().await;
            queue.dequeue()?
        };

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
                    progress_tx: progress_tx.clone(),
                    shard_info: item.shard_info.clone(),
                    group_id: item.group_id.as_ref().map(std::string::ToString::to_string),
                },
            );
        }

        // Emit queue snapshot (item now active)
        self.emit_queue_snapshot().await;

        Some((lease, item, cancel, progress_tx))
    }

    /// Finalize a job after it completes or fails.
    ///
    /// Verifies the lease to prevent stale commits.
    /// Lock order: active → queue.
    async fn finalize_job(
        &self,
        item: &QueuedItem,
        lease: LeaseId,
        result: Result<CompletedJob, DownloadError>,
    ) {
        // Verify lease and remove from active
        if !self.verify_and_remove_lease(&item.id, lease).await {
            tracing::debug!(id = %item.id, "Ignoring stale finalize (lease mismatch)");
            return;
        }

        // Handle the result with shard coordination
        self.handle_job_result(item, result).await;

        // Emit updated snapshot
        self.emit_queue_snapshot().await;
    }

    /// Verify lease matches and remove from active map.
    async fn verify_and_remove_lease(&self, id: &DownloadId, lease: LeaseId) -> bool {
        let mut active = self.active.lock().await;
        active
            .get(id)
            .is_some_and(|job| job.lease == lease)
            .then(|| active.remove(id))
            .is_some()
    }

    /// Handle the result of a completed job.
    async fn handle_job_result(
        &self,
        item: &QueuedItem,
        result: Result<CompletedJob, DownloadError>,
    ) {
        match result {
            Ok(completed) => self.handle_success(item, completed).await,
            Err(DownloadError::Cancelled) => self.handle_cancellation(item).await,
            Err(e) => self.handle_failure(item, e).await,
        }
    }

    /// Handle successful download completion.
    async fn handle_success(&self, item: &QueuedItem, completed: CompletedJob) {
        if let Some(group_id) = &item.group_id {
            if let Some(shard_info) = &item.shard_info {
                self.handle_shard_completion(item, group_id, shard_info, completed)
                    .await;
                return;
            }
        }

        // Single-file download - register immediately
        self.handle_single_file_completion(item, completed).await;
    }

    /// Handle completion of a shard in a multi-shard download.
    async fn handle_shard_completion(
        &self,
        item: &QueuedItem,
        group_id: &ShardGroupId,
        shard_info: &ShardInfo,
        completed: CompletedJob,
    ) {
        // Use stable base filename so all shards compute the same identity
        let base_filename = completed
            .files
            .first()
            .map_or_else(|| "unknown".to_string(), |f| base_shard_filename(f));

        let metadata = GroupMetadata {
            repo_id: completed.repo_id.clone(),
            commit_sha: completed.commit_sha.clone(),
            quantization: completed.quantization,
            primary_filename: base_filename,
            hf_tags: vec![],
        };

        let group_complete = {
            let mut tracker = self.shard_tracker.lock().await;
            tracker.on_shard_done(
                group_id,
                shard_info.shard_index,
                completed.primary_path.clone(),
                shard_info.total_shards,
                &metadata,
            )
        };

        // Only register and emit event if group is complete
        if let Some(complete) = group_complete {
            tracing::info!(
                id = %item.id,
                shard_count = complete.ordered_paths.len(),
                "All shards downloaded, registering model"
            );
            // Record completion ONCE per group (not per shard)
            self.record_completion_in_run(item, CompletionKind::Downloaded)
                .await;
            self.register_completed_model(complete).await;
        } else {
            tracing::debug!(
                id = %item.id,
                shard = shard_info.shard_index,
                "Shard downloaded, waiting for remaining shards"
            );
        }
    }

    /// Handle completion of a single-file download.
    async fn handle_single_file_completion(&self, item: &QueuedItem, completed: CompletedJob) {
        tracing::info!(id = %item.id, "Single-file download completed");
        let metadata = GroupMetadata {
            repo_id: completed.repo_id.clone(),
            commit_sha: completed.commit_sha.clone(),
            quantization: completed.quantization,
            primary_filename: completed
                .files
                .first()
                .cloned()
                .unwrap_or_else(|| "unknown".to_string()),
            hf_tags: vec![],
        };
        let complete = shard_group_tracker::GroupComplete {
            ordered_paths: completed.all_paths.clone(),
            metadata,
        };
        // Record completion before registering
        self.record_completion_in_run(item, CompletionKind::Downloaded)
            .await;
        self.register_completed_model(complete).await;
    }

    /// Handle download cancellation.
    async fn handle_cancellation(&self, item: &QueuedItem) {
        tracing::info!(id = %item.id, "Download cancelled");

        // Clean up shard tracker if this was part of a group
        if let Some(group_id) = &item.group_id {
            self.shard_tracker.lock().await.on_group_failed(group_id);
        }

        // Record cancellation
        self.record_completion_in_run(item, CompletionKind::Cancelled)
            .await;

        self.event_emitter.emit(DownloadEvent::DownloadCancelled {
            id: item.id.to_string(),
        });
    }

    /// Handle download failure.
    async fn handle_failure(&self, item: &QueuedItem, e: DownloadError) {
        tracing::warn!(id = %item.id, error = %e, "Download failed");

        // Clean up shard tracker if this was part of a group
        if let Some(group_id) = &item.group_id {
            self.shard_tracker.lock().await.on_group_failed(group_id);
        }

        // Record failure
        self.record_completion_in_run(item, CompletionKind::Failed)
            .await;

        let queued_item = QueuedItem::new(item.id.clone(), item.completion_key.clone());
        self.queue
            .write()
            .await
            .mark_failed(queued_item, e.to_string());
    }

    /// Register a completed model (all shards downloaded).
    ///
    /// This is the single point of model registration, called only when
    /// all shards in a group are complete (or for single-file downloads).
    async fn register_completed_model(&self, complete: shard_group_tracker::GroupComplete) {
        use gglib_core::ports::CompletedDownload;

        let primary_path = complete
            .ordered_paths
            .first()
            .expect("GroupComplete should have at least one path");

        // Fetch HF model info to get tags (soft fail if unavailable)
        let hf_tags = self
            .hf_client
            .get_model_info(&complete.metadata.repo_id)
            .await
            .ok()
            .map(|info| info.tags)
            .unwrap_or_default();

        let completed = CompletedDownload {
            primary_path: primary_path.clone(),
            all_paths: complete.ordered_paths.clone(),
            quantization: complete.metadata.quantization,
            repo_id: complete.metadata.repo_id.clone(),
            commit_sha: complete.metadata.commit_sha.clone(),
            is_sharded: complete.ordered_paths.len() > 1,
            total_bytes: 0, // TODO: track total bytes
            file_paths: if complete.ordered_paths.len() > 1 {
                Some(complete.ordered_paths.clone())
            } else {
                None
            },
            hf_tags,
        };

        // Register model (soft-fail)
        match self.model_registrar.register_model(&completed).await {
            Ok(model) => {
                tracing::info!(
                    model_id = model.id,
                    model_name = %model.name,
                    shard_count = complete.ordered_paths.len(),
                    "Model registered successfully"
                );

                // Emit completion event
                self.event_emitter.emit(DownloadEvent::DownloadCompleted {
                    id: format!(
                        "{}:{}",
                        complete.metadata.repo_id, complete.metadata.quantization
                    ),
                    message: Some(format!(
                        "Downloaded {} to {}",
                        if completed.is_sharded {
                            format!("{} shards", complete.ordered_paths.len())
                        } else {
                            "model".to_string()
                        },
                        primary_path.display()
                    )),
                });
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %primary_path.display(),
                    "Failed to register model - files downloaded but won't appear in library"
                );
            }
        }
    }

    /// Spawn a progress bridge task that rate-limits event emission.
    fn spawn_progress_bridge(
        &self,
        id: &DownloadId,
        shard_info: Option<&ShardInfo>,
        mut rx: watch::Receiver<ProgressUpdate>,
        cancel: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        let event_emitter = Arc::clone(&self.event_emitter);
        let id_str = id.to_string();
        let shard_info = shard_info.cloned();

        tokio::spawn(async move {
            use std::time::Instant;

            let mut tick = interval(Duration::from_millis(250));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            let mut last_emitted = ProgressUpdate::default();
            let mut last_bytes = 0u64;
            let mut last_time = Instant::now();
            let mut ewa_speed = 0.0f64;
            let mut first_update = true;

            loop {
                tokio::select! {
                    biased;

                    () = cancel.cancelled() => {
                        // Don't emit progress on cancel - let DownloadCancelled event be final
                        break;
                    }

                    result = rx.changed() => {
                        if result.is_err() {
                            // Sender dropped (job finished), emit final and exit
                            let final_progress = rx.borrow().clone();
                            if final_progress.seq > last_emitted.seq {
                                let now = Instant::now();
                                let (speed, eta) = calculate_speed_eta(
                                    final_progress.downloaded,
                                    final_progress.total,
                                    last_bytes,
                                    last_time,
                                    now,
                                    ewa_speed,
                                    first_update,
                                );
                                emit_progress(
                                    &event_emitter,
                                    &id_str,
                                    shard_info.as_ref(),
                                    &final_progress,
                                    speed,
                                    eta,
                                );
                            }
                            break;
                        }
                        // Progress changed, will be picked up on next tick
                    }

                    _ = tick.tick() => {
                        let current = rx.borrow().clone();
                        if current.seq > last_emitted.seq {
                            let now = Instant::now();
                            let elapsed = now.duration_since(last_time).as_secs_f64();

                            if elapsed > 0.0 {
                                let bytes_delta = current.downloaded.saturating_sub(last_bytes);
                                #[allow(clippy::cast_precision_loss)]
                                let instant_speed = bytes_delta as f64 / elapsed;

                                // Update EWA speed
                                if first_update {
                                    ewa_speed = instant_speed;
                                    first_update = false;
                                } else {
                                    ewa_speed = EWA_SMOOTHING.mul_add(
                                        instant_speed,
                                        (1.0 - EWA_SMOOTHING) * ewa_speed
                                    );
                                }

                                last_bytes = current.downloaded;
                                last_time = now;
                            }

                            // Calculate ETA
                            #[allow(clippy::cast_precision_loss)]
                            let eta = if ewa_speed > 0.0 && current.downloaded < current.total {
                                let remaining = current.total.saturating_sub(current.downloaded);
                                remaining as f64 / ewa_speed
                            } else {
                                0.0
                            };

                            emit_progress(
                                &event_emitter,
                                &id_str,
                                shard_info.as_ref(),
                                &current,
                                ewa_speed,
                                eta,
                            );
                            last_emitted = current;
                        }
                    }
                }
            }
        })
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

    /// Check if there's an active download.
    async fn has_active(&self) -> bool {
        !self.active.lock().await.is_empty()
    }

    /// Emit a queue snapshot event.
    async fn emit_queue_snapshot(&self) {
        let Ok(snapshot) = self.get_queue_snapshot().await else {
            return;
        };

        // Handle queue drain state transitions
        let is_drained = self.check_queue_drained(&snapshot).await;
        self.handle_drain_transitions(is_drained).await;

        // Emit snapshot event
        self.emit_snapshot_event(&snapshot);
    }

    /// Check if the queue is fully drained (no pending, no active, no open shard groups).
    async fn check_queue_drained(&self, snapshot: &QueueSnapshot) -> bool {
        let has_open_groups = self.shard_tracker.lock().await.has_open_groups();
        let is_drained =
            snapshot.pending_count == 0 && snapshot.active_count == 0 && !has_open_groups;

        tracing::debug!(
            target: "gglib.download",
            pending = snapshot.pending_count,
            active = snapshot.active_count,
            has_open_groups,
            is_drained,
            "Queue drain check"
        );

        is_drained
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

    /// Emit the `QueueSnapshot` event to subscribers.
    fn emit_snapshot_event(&self, snapshot: &QueueSnapshot) {
        let items: Vec<DownloadSummary> = snapshot
            .items
            .iter()
            .map(|item| DownloadSummary {
                id: item.id.clone(),
                display_name: item.display_name.clone(),
                status: item.status,
                position: item.position,
                error: None,
                group_id: item.group_id.clone(),
                shard_info: item.shard_info.clone(),
            })
            .collect();

        tracing::debug!(
            target: "gglib.download",
            items_count = items.len(),
            max_size = snapshot.max_size,
            "Emitting QueueSnapshot event",
        );

        self.event_emitter.emit(DownloadEvent::QueueSnapshot {
            items,
            max_size: snapshot.max_size,
        });
    }

    /// Emit queue run complete event with summary.
    fn emit_queue_run_complete(&self, run: QueueRunState) {
        use gglib_core::download::QueueRunSummary;

        let completed_at_ms = Self::get_current_timestamp_ms();
        let mut items = Self::build_completion_details(run.completions);
        items.sort_by(|a, b| b.last_completed_at_ms.cmp(&a.last_completed_at_ms));

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

        self.event_emitter
            .emit(DownloadEvent::QueueRunComplete { summary });
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

/// Helper to calculate speed and ETA from progress deltas.
fn calculate_speed_eta(
    downloaded: u64,
    total: u64,
    last_bytes: u64,
    last_time: std::time::Instant,
    now: std::time::Instant,
    ewa_speed: f64,
    first_update: bool,
) -> (f64, f64) {
    let elapsed = now.duration_since(last_time).as_secs_f64();

    if elapsed <= 0.0 {
        return (ewa_speed, 0.0);
    }

    let bytes_delta = downloaded.saturating_sub(last_bytes);
    #[allow(clippy::cast_precision_loss)]
    let instant_speed = bytes_delta as f64 / elapsed;

    let speed = if first_update {
        instant_speed
    } else {
        EWA_SMOOTHING.mul_add(instant_speed, (1.0 - EWA_SMOOTHING) * ewa_speed)
    };

    #[allow(clippy::cast_precision_loss)]
    let eta = if speed > 0.0 && downloaded < total {
        let remaining = total.saturating_sub(downloaded);
        remaining as f64 / speed
    } else {
        0.0
    };

    (speed, eta)
}

/// Emit a progress event.
///
/// Emits `ShardProgress` if `shard_info` is present, otherwise `DownloadProgress`.
fn emit_progress(
    emitter: &Arc<dyn DownloadEventEmitterPort>,
    id: &str,
    shard_info: Option<&ShardInfo>,
    progress: &ProgressUpdate,
    speed_bps: f64,
    _eta_seconds: f64,
) {
    if let Some(shard) = shard_info {
        // Calculate aggregate totals across all shards
        // For sequential shard downloads: completed shards + current shard progress
        let (aggregate_downloaded, aggregate_total) = shard.file_size.map_or_else(
            || {
                // No size info: use current shard progress as approximation
                // This will make aggregate == shard progress, but at least UI will show shard index
                (
                    progress.downloaded,
                    progress.total * u64::from(shard.total_shards),
                )
            },
            |shard_size| {
                // We have size info: calculate proper aggregates
                // Sum of all completed shards + current progress
                let completed_bytes = u64::from(shard.shard_index) * shard_size;
                let aggregate_downloaded = completed_bytes + progress.downloaded;
                // Total = size per shard * number of shards (assumes equal-sized shards)
                let aggregate_total = shard_size * u64::from(shard.total_shards);
                (aggregate_downloaded, aggregate_total)
            },
        );

        emitter.emit(DownloadEvent::shard_progress(
            id,
            shard.shard_index,
            shard.total_shards,
            &shard.filename,
            progress.downloaded,
            progress.total,
            aggregate_downloaded,
            aggregate_total,
            speed_bps,
        ));
    } else {
        // Non-sharded download: emit regular progress
        emitter.emit(DownloadEvent::progress(
            id,
            progress.downloaded,
            progress.total,
            speed_bps,
        ));
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
fn validate_cached_gguf(
    path: &std::path::Path,
    expected_size: Option<u64>,
) -> Result<(), String> {
    use std::io::Read;

    let metadata = std::fs::metadata(path)
        .map_err(|e| format!("cannot stat file: {e}"))?;
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
    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("cannot open file: {e}"))?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)
        .map_err(|e| format!("cannot read magic bytes: {e}"))?;

    if magic != GGUF_MAGIC {
        return Err(format!(
            "invalid GGUF magic: expected {:?}, got {:?}",
            GGUF_MAGIC, magic
        ));
    }

    Ok(())
}

// =============================================================================
// DownloadManagerPort implementation
// =============================================================================

#[async_trait]
impl DownloadManagerPort for DownloadManagerImpl {
    async fn queue_download(&self, request: DownloadRequest) -> Result<DownloadId, DownloadError> {
        let id = DownloadId::new(&request.repo_id, Some(request.quantization.to_string()));

        // Resolve files (outside lock)
        let resolution = self
            .resolver
            .resolve(&request.repo_id, request.quantization)
            .await?;

        let has_active = self.has_active().await;

        // Build shard files list (outside lock)
        let shard_files: Vec<_> = resolution
            .files
            .iter()
            .map(|f| (f.path.clone(), f.size))
            .collect();

        // Compute completion key from first file (canonical base for shards)
        let first_path = resolution
            .files
            .first()
            .ok_or_else(|| DownloadError::resolution_failed("no files resolved".to_string()))?
            .path
            .as_str();
        let filename = std::path::Path::new(first_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(first_path);
        let filename_canon = base_shard_filename(filename);
        let completion_key = gglib_core::download::CompletionKey::HfFile {
            repo_id: request.repo_id.clone(),
            revision: request
                .revision
                .clone()
                .unwrap_or_else(|| "unspecified".to_string()),
            filename_canon,
            quantization: Some(request.quantization.to_string()),
        };

        // Minimal lock scope: mutate queue and get snapshot
        let position = {
            let mut queue = self.queue.write().await;
            queue.queue_sharded(&id, &completion_key, shard_files, has_active)?
        };

        tracing::info!(
            id = %id,
            position = position,
            sharded = resolution.is_sharded,
            files = resolution.files.len(),
            "Download queued"
        );

        // Notify runner and emit snapshot (outside lock)
        self.queue_notify.notify_one();
        self.emit_queue_snapshot().await;

        Ok(id)
    }

    async fn queue_and_process(
        self: Arc<Self>,
        request: DownloadRequest,
    ) -> Result<DownloadId, DownloadError> {
        let id = self.queue_download(request).await?;
        self.ensure_runner();
        Ok(id)
    }

    async fn queue_smart(
        self: Arc<Self>,
        repo_id: String,
        quantization: Option<String>,
    ) -> Result<(usize, usize), DownloadError> {
        let result = self.queue_download_smart(&repo_id, quantization).await?;
        self.ensure_runner();
        Ok((1, result.queued as usize))
    }

    async fn get_queue_snapshot(&self) -> Result<QueueSnapshot, DownloadError> {
        // Build current item DTO if there's an active download (short lock scope)
        let current_dto = {
            let active = self.active.lock().await;
            active.iter().next().map(|(id, job)| {
                let mut dto = gglib_core::download::QueuedDownload::new(
                    id.to_string(),
                    id.model_id(),
                    id.to_string(),
                    1,
                    0,
                )
                .with_status(gglib_core::download::DownloadStatus::Downloading);

                // Preserve shard info if this is a sharded download
                if let Some(shard) = &job.shard_info {
                    if let Some(group) = &job.group_id {
                        dto = dto.with_shard_info(group.clone(), shard.clone());
                    }
                }

                dto
            })
        };

        let queue = self.queue.read().await;
        Ok(queue.snapshot(current_dto))
    }

    async fn cancel_download(&self, id: &DownloadId) -> Result<(), DownloadError> {
        // Check if active, cancel via token
        {
            let active = self.active.lock().await;
            if let Some(job) = active.get(id) {
                job.cancel.cancel();
                tracing::info!(id = %id, "Cancelled active download");
                return Ok(());
            }
        }

        // Otherwise remove from queue
        self.queue.write().await.remove(id)?;
        tracing::info!(id = %id, "Removed download from queue");
        self.emit_queue_snapshot().await;
        Ok(())
    }

    async fn cancel_all(&self) -> Result<(), DownloadError> {
        // Cancel all active downloads
        {
            let active = self.active.lock().await;
            for job in active.values() {
                job.cancel.cancel();
            }
        }

        // Clear queue
        self.queue.write().await.clear();
        self.emit_queue_snapshot().await;
        tracing::info!("Cancelled all downloads");
        Ok(())
    }

    async fn has_download(&self, id: &DownloadId) -> Result<bool, DownloadError> {
        // Check active
        if self.active.lock().await.contains_key(id) {
            return Ok(true);
        }

        // Check queue
        let queue = self.queue.read().await;
        Ok(queue.is_queued(id) || queue.is_failed(id))
    }

    async fn active_count(&self) -> Result<u32, DownloadError> {
        #[allow(clippy::cast_possible_truncation)]
        Ok(self.active.lock().await.len() as u32)
    }

    async fn pending_count(&self) -> Result<u32, DownloadError> {
        let queue = self.queue.read().await;
        #[allow(clippy::cast_possible_truncation)]
        Ok(queue.pending_len() as u32)
    }

    async fn remove_from_queue(&self, id: &DownloadId) -> Result<(), DownloadError> {
        self.queue.write().await.remove(id)?;
        tracing::info!(id = %id, "Removed download from queue");
        Ok(())
    }

    async fn reorder_queue(
        &self,
        id: &DownloadId,
        new_position: u32,
    ) -> Result<u32, DownloadError> {
        let has_active = self.has_active().await;
        let actual_position = self
            .queue
            .write()
            .await
            .reorder(id, new_position, has_active)?;
        tracing::info!(id = %id, position = actual_position, "Reordered download");
        self.emit_queue_snapshot().await;
        Ok(actual_position)
    }

    async fn cancel_group(&self, group_id: &str) -> Result<(), DownloadError> {
        use crate::queue::ShardGroupId;

        let shard_group_id = ShardGroupId::new(group_id);
        let removed = self.queue.write().await.remove_group(&shard_group_id);
        self.emit_queue_snapshot().await;
        tracing::info!(group_id = %group_id, removed = removed, "Cancelled shard group");
        Ok(())
    }

    async fn retry(&self, id: &DownloadId) -> Result<u32, DownloadError> {
        let has_active = self.has_active().await;
        let position = self.queue.write().await.retry_failed(id, has_active)?;
        tracing::info!(id = %id, position = position, "Retried failed download");
        self.emit_queue_snapshot().await;
        self.queue_notify.notify_one();
        Ok(position)
    }

    async fn clear_failed(&self) -> Result<(), DownloadError> {
        self.queue.write().await.clear_failed();
        tracing::info!("Cleared failed downloads");
        Ok(())
    }

    async fn set_max_queue_size(&self, size: u32) -> Result<(), DownloadError> {
        self.queue.write().await.set_max_size(size);
        tracing::info!(size = size, "Set max queue size");
        Ok(())
    }

    async fn get_max_queue_size(&self) -> Result<u32, DownloadError> {
        let queue = self.queue.read().await;
        Ok(queue.max_size())
    }
}

// =============================================================================
// Convenience methods for GUI / AppCore compatibility
// =============================================================================

/// Result of queuing a download with auto-detection.
#[derive(Debug, Clone)]
pub struct QueueAutoResult {
    /// The root download ID for this request.
    pub root_id: DownloadId,
    /// Number of items queued (1 for single file, N for sharded).
    pub queued: u32,
    /// Group ID if this is a sharded download.
    pub group_id: Option<String>,
}

impl DownloadManagerImpl {
    /// Queue a download with smart quantization selection.
    pub async fn queue_download_smart(
        &self,
        repo_id: impl Into<String>,
        quantization: Option<String>,
    ) -> Result<QueueAutoResult, DownloadError> {
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

        let has_active = self.has_active().await;
        let shard_count = resolution.files.len();

        // Build shard files outside lock
        let shard_files: Vec<_> = resolution
            .files
            .iter()
            .map(|f| (f.path.clone(), f.size))
            .collect();

        // Compute completion key from first file (canonical base for shards)
        let first_path = resolution
            .files
            .first()
            .ok_or_else(|| DownloadError::resolution_failed("no files resolved".to_string()))?
            .path
            .as_str();
        let filename = std::path::Path::new(first_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(first_path);
        let filename_canon = base_shard_filename(filename);
        let completion_key = gglib_core::download::CompletionKey::HfFile {
            repo_id: repo_id.clone(),
            revision: "unspecified".to_string(),
            filename_canon,
            quantization: Some(selection.quantization.to_string()),
        };

        // Minimal lock scope
        let position = {
            let mut queue = self.queue.write().await;
            queue.queue_sharded(&id, &completion_key, shard_files, has_active)?
        };

        let group_id = Some(id.to_string());

        tracing::info!(
            id = %id,
            position = position,
            sharded = resolution.is_sharded,
            files = shard_count,
            "Download queued via queue_download_smart"
        );

        self.queue_notify.notify_one();
        self.emit_queue_snapshot().await;

        #[allow(clippy::cast_possible_truncation)]
        Ok(QueueAutoResult {
            root_id: id,
            queued: shard_count as u32,
            group_id,
        })
    }

    /// Shutdown cleanup for process termination.
    pub fn shutdown_cleanup(&self) -> usize {
        // Cancel all tokens synchronously
        // We can't block_read() on tokio::sync::Mutex, so use try_lock
        self.active.try_lock().map_or_else(
            |_| {
                tracing::warn!("Shutdown cleanup: couldn't acquire lock");
                0
            },
            |active| {
                let count = active.len();
                for job in active.values() {
                    job.cancel.cancel();
                }
                tracing::info!(count = count, "Shutdown cleanup: cancelled download tokens");
                count
            },
        )
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

    #[test]
    fn progress_update_seq_comparison() {
        let p1 = ProgressUpdate::new(100, 1000, 1);
        let p2 = ProgressUpdate::new(200, 1000, 2);

        assert!(p2.seq > p1.seq);
    }
}
