//! What the manager publishes: one snapshot for REST and the event stream,
//! in order, at every change of phase and on every tick of the meter.

use std::sync::{Condvar, Mutex as StdMutex};
use std::time::Instant;

use gglib_core::ports::{ModelRegistrarPort, NoopEmitter};

use super::super::duplicate_guard_tests::NoRegistrar;
use super::super::group_registration_tests::RecordingRegistrar;
use super::super::test_support::{End, Recorded, reading, run_next};
use super::super::*;
use crate::test_hub::RepoHub;

pub(super) const REPO: &str = "owner/zeta-GGUF";
pub(super) const ID: &str = "owner/zeta-GGUF:Q8_0";

pub(super) struct Fixture {
    pub(super) manager: Arc<DownloadManagerImpl>,
    pub(super) recorded: Arc<Recorded>,
}

/// A manager over a repository of one weights file and a projector, with
/// that download queued, keeping every event it emits.
pub(super) async fn queued(registrar: Arc<dyn ModelRegistrarPort>) -> Fixture {
    let recorded = Arc::new(Recorded::default());
    let manager = Arc::new(DownloadManagerImpl::new(
        registrar,
        Arc::new(RepoHub::new(&[
            ("mmproj-F16.gguf", 300),
            ("zeta.Q8_0.gguf", 1_000),
        ])),
        recorded.clone(),
        DownloadManagerConfig::default(),
        Arc::new(gglib_core::ports::NoopGgufParser),
    ));
    manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .unwrap();
    Fixture { manager, recorded }
}

pub(super) fn failed(error: &str) -> DownloadOutcome {
    DownloadOutcome::Failed {
        error: error.to_string(),
    }
}

// ── One snapshot ─────────────────────────────────────────────────────────

/// The REST route and the event stream are served by one builder: with a
/// file half in, a download waiting and one ended, the two snapshots differ
/// in their revision alone.
#[tokio::test]
async fn rest_and_sse_carry_the_same_snapshot() {
    let f = queued(Arc::new(NoRegistrar)).await;
    f.manager
        .queue_download_smart("owner/other", Some("Q8_0".to_string()))
        .await
        .unwrap();
    let ended = DownloadId::new("owner/gone", Some("Q8_0"));
    let mut queue = f.manager.queue.write().await;
    queue.record_outcome(&ended, failed("no route"));
    drop(queue);
    let (_lease, item, _cancel, _progress) = f.manager.next_job().await.unwrap();
    f.manager
        .observe(&item.id, &reading(400, 1_000), Instant::now());

    f.manager.publish().await;
    let sent = f.recorded.snapshots().pop().expect("a snapshot was sent");
    let served = f.manager.get_queue_snapshot().await.unwrap();

    let active = sent.active.as_ref().expect("a running row");
    assert_eq!(
        (active.downloaded_bytes, active.total_bytes),
        (400, Some(1_300))
    );
    assert_eq!(active.text.file.as_deref(), Some("weights"));
    assert_eq!((sent.waiting.len(), sent.finished.len()), (1, 1));
    assert_eq!(
        served,
        QueueSnapshot {
            revision: sent.revision + 1,
            ..sent.clone()
        }
    );
}

/// Every snapshot built has the next revision, whoever asked for it.
#[tokio::test]
async fn each_snapshot_has_the_next_revision() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let queued_at = f.recorded.snapshots().pop().unwrap().revision;

    let first = f.manager.get_queue_snapshot().await.unwrap().revision;
    f.manager.publish().await;
    let second = f.recorded.snapshots().pop().unwrap().revision;
    let third = f.manager.get_queue_snapshot().await.unwrap().revision;

    assert_eq!(
        [first, second, third],
        [queued_at + 1, queued_at + 2, queued_at + 3]
    );
}

/// Keeps the revision of each snapshot it is sent. It holds the first one
/// back until the second has arrived, or a fifth of a second has passed: a
/// publisher that let go of the publish mutex before sending would be
/// overtaken here by the next.
#[derive(Default)]
struct HoldsTheFirst {
    sent: StdMutex<Vec<u64>>,
    another: Condvar,
}

impl AppEventEmitter for HoldsTheFirst {
    fn emit(&self, event: AppEvent) {
        let AppEvent::Download {
            event: DownloadEvent::QueueSnapshot(snapshot),
        } = event
        else {
            return;
        };
        if snapshot.revision == 1 {
            let wait = Duration::from_millis(200);
            self.another
                .wait_timeout_while(self.sent.lock().unwrap(), wait, |sent| sent.is_empty())
                .unwrap()
                .0
                .push(snapshot.revision);
        } else {
            self.sent.lock().unwrap().push(snapshot.revision);
            self.another.notify_all();
        }
    }
}

/// Two threads publish at once. Whichever builds revision 1 is still
/// sending it when the other wants to build revision 2, and must be let
/// finish.
#[test]
fn snapshots_are_emitted_in_revision_order() {
    let held = Arc::new(HoldsTheFirst::default());
    let manager = Arc::new(DownloadManagerImpl::new(
        Arc::new(NoRegistrar),
        Arc::new(RepoHub::new(&[])),
        held.clone(),
        DownloadManagerConfig::default(),
        Arc::new(gglib_core::ports::NoopGgufParser),
    ));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    let publishers: Vec<_> = (0..2)
        .map(|_| {
            let (manager, handle) = (Arc::clone(&manager), runtime.handle().clone());
            std::thread::spawn(move || handle.block_on(manager.publish()))
        })
        .collect();
    for publisher in publishers {
        publisher.join().unwrap();
    }

    assert_eq!(*held.sent.lock().unwrap(), [1, 2]);
}

// ── Phases ───────────────────────────────────────────────────────────────

/// Once the last file is in, the download is finalized and then registered,
/// and each is a snapshot: a client that only reads snapshots sees both.
#[tokio::test]
async fn registering_publishes_a_snapshot() {
    let f = queued(Arc::new(RecordingRegistrar::default())).await;

    run_next(&f.manager, End::OnDisk).await;
    run_next(&f.manager, End::OnDisk).await;

    let snapshots = f.recorded.snapshots();
    let mut phases: Vec<DownloadPhase> = snapshots
        .iter()
        .filter_map(|snapshot| snapshot.active.as_ref().map(|row| row.phase))
        .collect();
    phases.dedup();
    assert_eq!(
        phases,
        [
            DownloadPhase::Downloading,
            DownloadPhase::Finalizing,
            DownloadPhase::Registering,
        ]
    );
    let registering = snapshots
        .iter()
        .filter_map(|snapshot| snapshot.active.as_ref())
        .find(|row| row.phase == DownloadPhase::Registering)
        .unwrap();
    assert_eq!(registering.text.status, "Registering…");
    assert_eq!(registering.downloaded_bytes, 1_300, "every byte is in");
    assert_eq!(registering.speed_bps, None);
    let revisions: Vec<u64> = snapshots.iter().map(|snapshot| snapshot.revision).collect();
    assert!(revisions.is_sorted_by(|a, b| a < b), "{revisions:?}");
}

// ── The meter task ───────────────────────────────────────────────────────

/// The task feeds the meter and publishes on its tick, takes the file's
/// last reading when the worker is done, and ends there.
#[tokio::test]
async fn the_meter_task_reads_the_last_count_and_ends_on_finished() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let (_lease, item, cancel, progress_tx) = f.manager.next_job().await.unwrap();
    let finished = CancellationToken::new();
    let task = tokio::spawn(Arc::clone(&f.manager).run_meter(
        item.id.clone(),
        progress_tx.subscribe(),
        cancel,
        finished.clone(),
    ));

    progress_tx.send_modify(|state| *state = reading(250, 1_000));
    let published = async {
        loop {
            let seen = f.recorded.snapshots().pop().and_then(|s| s.active);
            if seen.is_some_and(|row| row.downloaded_bytes == 250) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(5), published)
        .await
        .expect("a tick publishes the reading");

    progress_tx.send_modify(|state| *state = reading(1_000, 1_000));
    finished.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("the task ends on the finished signal")
        .unwrap();

    let last = f.manager.meters().get(&item.id).unwrap().reading();
    assert_eq!(last.bytes, 1_000, "the count the worker ended on");
}

#[tokio::test]
async fn the_meter_task_ends_on_cancellation() {
    let manager = Arc::new(DownloadManagerImpl::new(
        Arc::new(NoRegistrar),
        Arc::new(RepoHub::new(&[])),
        Arc::new(NoopEmitter::new()),
        DownloadManagerConfig::default(),
        Arc::new(gglib_core::ports::NoopGgufParser),
    ));
    let (progress_tx, _rx) = watch::channel(ProgressUpdate::default());
    let cancel = CancellationToken::new();
    let task = tokio::spawn(Arc::clone(&manager).run_meter(
        DownloadId::new(REPO, Some("Q8_0")),
        progress_tx.subscribe(),
        cancel.clone(),
        CancellationToken::new(),
    ));

    cancel.cancel();

    let joined = tokio::time::timeout(Duration::from_secs(5), task).await;
    assert!(joined.is_ok(), "the task ends when the job is cancelled");
}

/// A note from the worker is the row's status until bytes arrive again.
#[tokio::test]
async fn a_notice_is_the_rows_status() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let (_lease, item, _cancel, _progress) = f.manager.next_job().await.unwrap();
    let mut noted = reading(0, 1_000);
    noted.notice = Some("using direct transfer…".to_string());

    f.manager.observe(&item.id, &noted, Instant::now());
    let with_note = f
        .manager
        .get_queue_snapshot()
        .await
        .unwrap()
        .active
        .unwrap();
    f.manager
        .observe(&item.id, &reading(10, 1_000), Instant::now());
    let after = f
        .manager
        .get_queue_snapshot()
        .await
        .unwrap()
        .active
        .unwrap();

    assert_eq!(with_note.text.status, "using direct transfer…");
    assert_eq!(after.text.status, "Downloading");
}
