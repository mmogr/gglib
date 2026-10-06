//! Tests for a download group that carries a projector: what is queued, and
//! which events its files are reported with.

use std::time::Instant;

use gglib_core::download::{GgufFileRole, Quantization};
use gglib_core::ports::NoopEmitter;

use super::duplicate_guard_tests::NoRegistrar;
use super::test_support::{reading, size_of};
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

// ── What is queued ───────────────────────────────────────────────────────

#[tokio::test]
async fn the_projector_is_queued_in_the_models_group_and_is_not_a_shard() {
    let manager = manager();

    let queued = manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .unwrap();

    assert_eq!(queued.to_string(), "owner/zeta-GGUF:Q8_0");
    let snapshot = manager.get_queue_snapshot().await.unwrap();
    assert!(snapshot.active.is_none());
    assert_eq!(snapshot.waiting.len(), 1, "one row for the model");
    let row = &snapshot.waiting[0];
    assert_eq!(row.id, "owner/zeta-GGUF:Q8_0");
    assert_eq!(row.text.title, "owner/zeta-GGUF:Q8_0", "no file's name");
    assert_eq!(row.text.file.as_deref(), Some("3 parts"));
    assert_eq!(row.total_bytes, Some(2_800), "the projector's bytes too");

    // The four files behind that row, in the order they will run
    let mut queue = manager.queue.write().await;
    let files: Vec<_> = std::iter::from_fn(|| queue.dequeue()).collect();
    assert_eq!(files.len(), 4, "progress covers every file");
    for file in &files {
        assert_eq!(file.id.to_string(), "owner/zeta-GGUF:Q8_0");
        assert_eq!(file.group_id, files[0].group_id, "one group for the model");
        let place = file.shard_info.as_ref().unwrap();
        assert_eq!(place.total_shards, 3);
        assert_eq!(place.group_total_bytes, Some(2_800));
    }
    let last = files[3].shard_info.as_ref().unwrap();
    assert_eq!(last.role, GgufFileRole::Projector);
    assert_eq!(last.filename, "mmproj-F16.gguf");
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

/// The row while each file of the group is fetched: which file it names, and
/// the bytes it has when that file is half in.
#[tokio::test]
async fn the_row_names_each_file_and_carries_the_groups_bytes() {
    let manager = manager();
    manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .unwrap();

    let mut seen = Vec::new();
    for _ in 0..4 {
        let (_lease, item, _cancel, _progress) = manager.next_job().await.unwrap();
        let size = size_of(&item);
        manager.observe(&item.id, &reading(size / 2, size), Instant::now());

        let snapshot = manager.get_queue_snapshot().await.unwrap();
        let row = snapshot.active.expect("the model's row");
        assert_eq!(row.id, "owner/zeta-GGUF:Q8_0");
        assert_eq!(row.total_bytes, Some(2_800));
        seen.push((row.text.file.unwrap(), row.downloaded_bytes));

        // The file ends, without the registration its last would bring.
        manager.observe(&item.id, &reading(size, size), Instant::now());
        manager.meters().get_mut(&item.id).unwrap().file_done();
        manager.active.lock().await.remove(&item.id);
    }

    // The projector's bytes move the model's row on, and it is not "part 4/3".
    let named = |file: &str, bytes: u64| (file.to_string(), bytes);
    assert_eq!(
        seen,
        [
            named("part 1/3", 500),
            named("part 2/3", 1_500),
            named("part 3/3", 2_250),
            named("projector", 2_650),
        ]
    );
}
