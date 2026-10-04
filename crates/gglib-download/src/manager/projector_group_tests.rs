//! Tests for a download group that carries a projector: what is queued, and
//! which events its files are reported with.

use std::sync::Mutex as StdMutex;

use gglib_core::download::{GgufFileRole, Quantization};
use gglib_core::ports::NoopEmitter;

use super::duplicate_guard_tests::NoRegistrar;
use super::*;
use crate::test_hub::RepoHub;

const REPO: &str = "owner/zeta-GGUF";

/// A manager over a repository of three `Q8_0` shards and an `F16` projector.
fn manager() -> DownloadManagerImpl {
    let hub = RepoHub::new(&[
        ("mmproj-F16.gguf", 300),
        ("zeta.Q8_0-00001-of-00003.gguf", 1_000),
        ("zeta.Q8_0-00002-of-00003.gguf", 1_000),
        ("zeta.Q8_0-00003-of-00003.gguf", 500),
    ]);
    DownloadManagerImpl::new(
        Arc::new(NoRegistrar),
        Arc::new(hub),
        Arc::new(NoopEmitter::new()),
        DownloadManagerConfig::default(),
    )
}

/// Keeps every download event emitted.
#[derive(Default)]
struct Recorded(StdMutex<Vec<DownloadEvent>>);

impl AppEventEmitter for Recorded {
    fn emit(&self, event: AppEvent) {
        if let AppEvent::Download { event } = event {
            self.0.lock().unwrap().push(event);
        }
    }
}

impl Recorded {
    fn events(&self) -> Vec<DownloadEvent> {
        self.0.lock().unwrap().clone()
    }
}

/// The projector's place in a group of three shards of 1000, 1000 and 500
/// bytes: it follows them, and its 300 bytes end the group's 2800.
fn projector_place() -> ShardInfo {
    ShardInfo::with_size(3, 3, "mmproj-F16.gguf", 300)
        .with_role(GgufFileRole::Projector)
        .with_group_offsets(2_500, 2_800)
}

// ── What is queued ───────────────────────────────────────────────────────

#[tokio::test]
async fn the_projector_is_queued_in_the_models_group_and_is_not_a_shard() {
    let manager = manager();

    let queued = manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .unwrap();

    assert_eq!(queued.queued, 3, "three shards, whatever else is fetched");
    assert_eq!(queued.root_id.to_string(), "owner/zeta-GGUF:Q8_0");
    let snapshot = manager.get_queue_snapshot().await.unwrap();
    assert_eq!(snapshot.items.len(), 4, "progress covers every file");
    let group = snapshot.items[0].group_id.clone();
    assert!(group.is_some());
    for item in &snapshot.items {
        assert_eq!(item.id, "owner/zeta-GGUF:Q8_0", "one id for the model");
        assert_eq!(item.group_id, group, "one group for the model");
        let place = item.shard_info.as_ref().unwrap();
        assert_eq!(place.total_shards, 3);
        assert_eq!(place.group_total_bytes, Some(2_800));
    }
    let last = snapshot.items[3].shard_info.as_ref().unwrap();
    assert_eq!(last.role, GgufFileRole::Projector);
    assert_eq!(last.filename, "mmproj-F16.gguf");
    assert_eq!(
        snapshot.items[3].display_name,
        "owner/zeta-GGUF:Q8_0 (Projector)"
    );
    assert_eq!(
        snapshot.items[2].display_name,
        "owner/zeta-GGUF:Q8_0 (Part 3/3)"
    );
}

/// The files are kept for registration by both ways in: the request a user
/// makes, and the one a repair makes through the port.
#[tokio::test]
async fn both_ways_of_queueing_keep_the_groups_files_for_registration() {
    let by_user = manager();
    by_user
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .unwrap();
    let by_repair = manager();
    let request = DownloadRequest::new(REPO.to_string(), Quantization::Q8_0);
    let id = by_repair.queue_download(request).await.unwrap();

    for manager in [&by_user, &by_repair] {
        let kept = manager
            .file_entries_map
            .lock()
            .await
            .get(&id.to_string())
            .cloned();
        let files = kept.expect("the group's files are kept");
        let roles: Vec<_> = files.iter().map(|f| f.role).collect();
        assert_eq!(files.len(), 4);
        assert_eq!(files[3].path, "mmproj-F16.gguf");
        assert_eq!(roles[..3], [GgufFileRole::Weights; 3]);
        assert_eq!(roles[3], GgufFileRole::Projector);
        assert_eq!(manager.queue.read().await.pending_len(), 4);
    }
}

// ── How its files are reported ───────────────────────────────────────────

fn emit(place: &ShardInfo, downloaded: u64) -> DownloadEvent {
    let recorded = Arc::new(Recorded::default());
    let emitter: Arc<dyn AppEventEmitter> = recorded.clone();
    let progress = ProgressUpdate::new(downloaded, place.file_size.unwrap_or(0), 1);
    emit_progress(
        &emitter,
        "owner/zeta-GGUF:Q8_0",
        Some(place),
        &progress,
        None,
        None,
    );
    let mut events = recorded.events();
    assert_eq!(events.len(), 1);
    events.remove(0)
}

/// The projector's bytes move the model's bar on: plain progress over the
/// whole group, with no shard number that would read "shard 4/3".
#[test]
fn a_projectors_progress_is_the_groups_progress_without_a_shard_number() {
    let event = emit(&projector_place(), 150);

    let DownloadEvent::DownloadProgress {
        id,
        downloaded,
        total,
        ..
    } = event
    else {
        panic!("plain progress, not {event:?}");
    };
    assert_eq!(id, "owner/zeta-GGUF:Q8_0");
    assert_eq!((downloaded, total), (2_650, 2_800));
}

#[test]
fn a_weights_shards_progress_keeps_its_shard_number() {
    let third = ShardInfo::with_size(2, 3, "zeta.Q8_0-00003-of-00003.gguf", 500)
        .with_group_offsets(2_000, 2_800);

    let event = emit(&third, 250);

    let DownloadEvent::ShardProgress {
        shard_index,
        total_shards,
        aggregate_downloaded,
        aggregate_total,
        ..
    } = event
    else {
        panic!("shard progress, not {event:?}");
    };
    assert_eq!((shard_index, total_shards), (2, 3));
    assert_eq!((aggregate_downloaded, aggregate_total), (2_250, 2_800));
}

#[tokio::test]
async fn a_projector_starts_without_a_shard_number_and_a_shard_with_one() {
    let recorded = Arc::new(Recorded::default());
    let manager = DownloadManagerImpl::new(
        Arc::new(NoRegistrar),
        Arc::new(RepoHub::new(&[])),
        recorded.clone(),
        DownloadManagerConfig::default(),
    );
    let id = DownloadId::new(REPO, Some("Q8_0"));
    let key = gglib_core::download::CompletionKey::HfFile {
        repo_id: REPO.to_string(),
        revision: "unspecified".to_string(),
        filename_canon: "zeta.Q8_0.gguf".to_string(),
        quantization: Some("Q8_0".to_string()),
    };
    let item = |place: ShardInfo| {
        QueuedItem::new_shard(id.clone(), ShardGroupId::new("g"), place, key.clone())
    };

    manager.emit_started_event(&item(projector_place()));
    manager.emit_started_event(&item(ShardInfo::new(1, 3, "zeta-00002-of-00003.gguf")));

    let started: Vec<_> = recorded
        .events()
        .into_iter()
        .map(|event| match event {
            DownloadEvent::DownloadStarted {
                shard_index,
                total_shards,
                ..
            } => (shard_index, total_shards),
            other => panic!("a started event, not {other:?}"),
        })
        .collect();
    assert_eq!(started, [(None, None), (Some(1), Some(3))]);
}

/// The snapshot of a projector being downloaded: the model's bytes so far,
/// under the model's id.
#[test]
fn the_active_projector_reports_the_groups_bytes() {
    let id = DownloadId::new(REPO, Some("Q8_0"));
    let progress = ProgressUpdate::new(150, 300, 1);

    let dto = build_active_dto(
        &id,
        &progress,
        Some(&projector_place()),
        Some("g"),
        None,
        None,
    );

    assert_eq!(dto.id, "owner/zeta-GGUF:Q8_0");
    assert_eq!((dto.downloaded_bytes, dto.total_bytes), (2_650, 2_800));
    assert_eq!(
        dto.shard_info.map(|place| place.role),
        Some(GgufFileRole::Projector)
    );
}
