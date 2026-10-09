//! Tests for an image model's download, which brings its family's
//! companions: what is queued under the one id, where each file goes, that
//! a companion already on disk is taken as it is, what the registrar is
//! handed, and the quantization chosen when none is asked.

use std::path::{Path, PathBuf};
use std::time::Instant;

use gglib_core::domain::{ComponentRole, ImageFamily};
use gglib_core::download::{CompletionKey, FilePlace};
use gglib_core::ports::NoopEmitter;

use super::group_registration_tests::RecordingRegistrar;
use super::test_support::{Started, start_next};
use super::*;
use crate::test_hub::{ImageHeadParser, RepoHub};

const FLUX_REPO: &str = "leejet/FLUX.1-schnell-gguf";
const FINETUNE_REPO: &str = "someone/flux-finetune-gguf";
const WEIGHTS: &str = "flux1-schnell-q8_0.gguf";
/// A finetune's weights, at another quantization: this Hub answers every
/// repository with the same files, so each download is told apart by its
/// quantization.
const FINETUNE_WEIGHTS: &str = "finetune-Q6_K.gguf";

/// Each companion's size on this Hub, and so on disk.
const COMPANION_SIZE: u64 = 10;

/// What a companion already on disk holds: as many bytes as the Hub lists,
/// and not a GGUF file's.
const COMPANION_ON_DISK: &[u8; 10] = b"safetensor";

/// What a weights file already on disk holds: a GGUF file's start, and the
/// size the Hub lists for [`WEIGHTS`] and [`FINETUNE_WEIGHTS`].
const WEIGHTS_ON_DISK: &[u8; 8] = b"GGUFflux";

/// A manager over a Flux.1 repository holding `weights`, whose downloads
/// go under `models` (or wherever the directory resolves, with none).
fn manager(
    weights: &[(&str, u64)],
    models: Option<&Path>,
) -> (DownloadManagerImpl, Arc<RecordingRegistrar>) {
    let registrar = Arc::new(RecordingRegistrar::default());
    let config = DownloadManagerConfig {
        models_directory: models.map(Path::to_path_buf),
        ..DownloadManagerConfig::default()
    };
    let manager = DownloadManagerImpl::new(
        registrar.clone(),
        Arc::new(RepoHub::image(ImageFamily::Flux1, weights, COMPANION_SIZE)),
        Arc::new(NoopEmitter::new()),
        config,
        Arc::new(ImageHeadParser),
    );
    (manager, registrar)
}

/// Every file waiting on the queue, in the order it will run.
async fn queued_files(manager: &DownloadManagerImpl) -> Vec<QueuedItem> {
    let mut queue = manager.queue.write().await;
    std::iter::from_fn(|| queue.dequeue()).collect()
}

/// The roles of the Flux.1 recipe's companions, in its order.
const FLUX_ROLES: [ComponentRole; 3] = [
    ComponentRole::Vae,
    ComponentRole::ClipL,
    ComponentRole::T5xxl,
];

// ── What is queued ───────────────────────────────────────────────────────

/// One row and one id for the model, whose total covers the companions;
/// behind it the weights and then the three companions, each named by its
/// role and fetched from its own repository, all of one group and one
/// completion key.
#[tokio::test]
async fn an_image_models_companions_are_queued_under_its_one_id() {
    let (manager, _) = manager(&[(WEIGHTS, 1_000)], None);

    let id = manager
        .queue_download_smart(FLUX_REPO, Some("Q8_0".to_string()))
        .await
        .unwrap();

    assert_eq!(id.to_string(), "leejet/FLUX.1-schnell-gguf:Q8_0");
    let snapshot = manager.get_queue_snapshot().await.unwrap();
    assert_eq!(snapshot.waiting.len(), 1, "one row for the model");
    assert_eq!(snapshot.waiting[0].total_bytes, Some(1_030));
    let files = queued_files(&manager).await;
    let seen: Vec<_> = files
        .iter()
        .map(|file| {
            let place = file.shard_info.as_ref().unwrap();
            (place.filename.as_str(), file.repo(), place.place())
        })
        .collect();
    assert_eq!(
        seen,
        [
            (WEIGHTS, FLUX_REPO, Some(FilePlace::Weights)),
            (
                "ae.safetensors",
                "unsloth/FLUX.1-schnell",
                Some(FilePlace::Component(ComponentRole::Vae))
            ),
            (
                "clip_l.safetensors",
                "comfyanonymous/flux_text_encoders",
                Some(FilePlace::Component(ComponentRole::ClipL))
            ),
            (
                "t5xxl_fp16.safetensors",
                "comfyanonymous/flux_text_encoders",
                Some(FilePlace::Component(ComponentRole::T5xxl))
            ),
        ]
    );
    for file in &files {
        assert_eq!(file.id, id);
        assert_eq!(file.group_id, files[0].group_id, "one group");
        assert_eq!(file.completion_key, files[0].completion_key);
        assert_eq!(file.shard_info.as_ref().unwrap().total_shards, 1);
    }
    let CompletionKey::HfFile { repo_id, .. } = &files[0].completion_key else {
        panic!("a Hub file's key");
    };
    assert_eq!(repo_id, FLUX_REPO, "the main repository's key");
}

/// With no quantization asked, an image repository is fetched at `Q8_0`,
/// though the preference list a chat model is chosen by would take another.
#[tokio::test]
async fn an_image_repository_with_no_quantization_asked_is_fetched_at_q8_0() {
    let (manager, _) = manager(
        &[
            ("flux1-schnell-q4_k_m.gguf", 600),
            ("flux1-schnell-q5_k_m.gguf", 700),
            (WEIGHTS, 1_000),
        ],
        None,
    );

    let id = manager.queue_download_smart(FLUX_REPO, None).await.unwrap();

    assert_eq!(id.quantization(), Some("Q8_0"));
    let files = queued_files(&manager).await;
    assert_eq!(files[0].shard_info.as_ref().unwrap().filename, WEIGHTS);
    assert_eq!(files.len(), 4, "with its companions");
}

/// An image repository with no `Q8_0` is chosen for as a chat one is: the
/// quantization it has, with its companions.
#[tokio::test]
async fn an_image_repository_without_q8_0_is_fetched_at_what_it_has() {
    let (manager, _) = manager(&[("flux1-schnell-q4_k_m.gguf", 600)], None);

    let id = manager.queue_download_smart(FLUX_REPO, None).await.unwrap();

    assert_eq!(id.quantization(), Some("Q4_K_M"));
    let files = queued_files(&manager).await;
    assert_eq!(
        files[0].shard_info.as_ref().unwrap().filename,
        "flux1-schnell-q4_k_m.gguf"
    );
    assert_eq!(files.len(), 4, "with its companions");
}

/// A chat repository with no quantization asked is chosen for as before:
/// the preference list's first, not `Q8_0`.
#[tokio::test]
async fn a_chat_repository_with_no_quantization_asked_is_chosen_for_as_before() {
    let registrar = Arc::new(RecordingRegistrar::default());
    let manager = DownloadManagerImpl::new(
        registrar,
        Arc::new(RepoHub::new(&[
            ("zeta.Q4_K_M.gguf", 600),
            ("zeta.Q8_0.gguf", 1_000),
        ])),
        Arc::new(NoopEmitter::new()),
        DownloadManagerConfig::default(),
        Arc::new(ImageHeadParser),
    );

    let id = manager
        .queue_download_smart("owner/zeta-GGUF", None)
        .await
        .unwrap();

    assert_eq!(id.quantization(), Some("Q4_K_M"));
}

/// A quantization asked for is the one fetched, an image model's too.
#[tokio::test]
async fn a_quantization_asked_for_an_image_model_is_the_one_fetched() {
    let (manager, _) = manager(
        &[("flux1-schnell-q4_k_m.gguf", 600), (WEIGHTS, 1_000)],
        None,
    );

    let id = manager
        .queue_download_smart(FLUX_REPO, Some("Q4_K_M".to_string()))
        .await
        .unwrap();

    assert_eq!(id.quantization(), Some("Q4_K_M"));
}

// ── Where each file goes ─────────────────────────────────────────────────

/// The weights go in the model's folder, and each companion in the folder
/// of the repository it comes from.
#[tokio::test]
async fn a_companion_is_planned_in_its_own_repositorys_folder() {
    let models = tempfile::tempdir().unwrap();
    let (manager, _) = manager(&[(WEIGHTS, 1_000)], Some(models.path()));
    manager
        .queue_download_smart(FLUX_REPO, Some("Q8_0".to_string()))
        .await
        .unwrap();

    let planned: Vec<PathBuf> = queued_files(&manager)
        .await
        .iter()
        .map(|item| manager.destination(item).unwrap().primary_path().unwrap())
        .collect();

    let root = models.path();
    assert_eq!(
        planned,
        [
            root.join("leejet_FLUX.1-schnell-gguf").join(WEIGHTS),
            root.join("unsloth_FLUX.1-schnell").join("ae.safetensors"),
            root.join("comfyanonymous_flux_text_encoders")
                .join("clip_l.safetensors"),
            root.join("comfyanonymous_flux_text_encoders")
                .join("t5xxl_fp16.safetensors"),
        ]
    );
}

/// The worker is sent to fetch each file from the repository it comes
/// from, under the download's one id.
#[tokio::test]
async fn each_files_job_is_fetched_from_its_own_repository() {
    let models = tempfile::tempdir().unwrap();
    let (manager, _) = manager(&[(WEIGHTS, 1_000)], Some(models.path()));
    let id = manager
        .queue_download_smart(FLUX_REPO, Some("Q8_0".to_string()))
        .await
        .unwrap();

    let jobs: Vec<(DownloadId, String)> = queued_files(&manager)
        .await
        .iter()
        .map(|item| {
            let destination = manager.destination(item).unwrap();
            let (progress, _) = watch::channel(ProgressUpdate::default());
            let job =
                DownloadManagerImpl::job_for(item, destination, CancellationToken::new(), progress);
            (job.id, job.repo)
        })
        .collect();

    let repos: Vec<&str> = jobs.iter().map(|(_, repo)| repo.as_str()).collect();
    assert_eq!(
        repos,
        [
            FLUX_REPO,
            "unsloth/FLUX.1-schnell",
            "comfyanonymous/flux_text_encoders",
            "comfyanonymous/flux_text_encoders",
        ]
    );
    assert!(jobs.iter().all(|(job_id, _)| *job_id == id));
}

// ── A companion already on disk ──────────────────────────────────────────

/// Put `bytes` at `path`, making its folder.
fn put(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// A file already on disk is kept when it is the size the Hub lists: a
/// `.gguf` file only when it starts as a GGUF file does, and any other, a
/// companion's safetensors file, by its size alone.
#[test]
fn a_cached_file_is_held_to_the_gguf_magic_only_when_named_gguf() {
    let dir = tempfile::tempdir().unwrap();
    let safetensors = dir.path().join("ae.safetensors");
    let gguf = dir.path().join("flux.gguf");
    put(&safetensors, COMPANION_ON_DISK);
    put(&gguf, COMPANION_ON_DISK);

    assert_eq!(
        validate_cached_file(&safetensors, Some(COMPANION_SIZE)),
        Ok(())
    );
    assert!(validate_cached_file(&safetensors, Some(COMPANION_SIZE + 1)).is_err());
    assert!(validate_cached_file(&gguf, Some(COMPANION_SIZE)).is_err());
}

/// Fetch `started`'s file as the run loop does and finalize it, and answer
/// how many of its bytes came off the network.
///
/// Every file is on disk before it is fetched, so nothing is asked of the
/// network; a file planned anywhere else stops the test before the worker
/// is sent for it.
async fn fetch_and_finish(manager: &DownloadManagerImpl, started: Started) -> u64 {
    let planned = manager
        .destination(&started.item)
        .unwrap()
        .primary_path()
        .unwrap();
    assert!(
        planned.exists(),
        "{} is not on disk, and the worker would fetch it from the network",
        planned.display()
    );
    let (progress, read) = watch::channel(ProgressUpdate::default());
    let fetched = manager
        .fetch(&started.item, started.cancel.clone(), progress)
        .await
        .expect("fetched with nothing to transfer");
    let reading = read.borrow().clone();
    manager.observe(&started.item.id, &reading, Instant::now());
    manager
        .finalize_job(&started.item, started.lease, Ok(fetched))
        .await;
    reading.progress.wire
}

/// Two models of the family, from different repositories, name the same
/// companions. The first model's download left them on disk; the second's
/// plans them at the same place, takes each as it is with no byte off the
/// network and without deleting it for not being a GGUF file, and both
/// models are handed the one file in each role.
#[tokio::test]
async fn a_companion_two_models_name_is_fetched_once_and_linked_to_both() {
    gglib_core::paths::isolate_data_root();
    let models = tempfile::tempdir().unwrap();
    let root = models.path();
    let (manager, registrar) = manager(&[(WEIGHTS, 8), (FINETUNE_WEIGHTS, 8)], Some(root));
    let companions = [
        root.join("unsloth_FLUX.1-schnell").join("ae.safetensors"),
        root.join("comfyanonymous_flux_text_encoders")
            .join("clip_l.safetensors"),
        root.join("comfyanonymous_flux_text_encoders")
            .join("t5xxl_fp16.safetensors"),
    ];
    for companion in &companions {
        put(companion, COMPANION_ON_DISK);
    }
    put(
        &root.join("leejet_FLUX.1-schnell-gguf").join(WEIGHTS),
        WEIGHTS_ON_DISK,
    );
    put(
        &root
            .join("someone_flux-finetune-gguf")
            .join(FINETUNE_WEIGHTS),
        WEIGHTS_ON_DISK,
    );

    let mut received = Vec::new();
    for (repo, quantization) in [(FLUX_REPO, "Q8_0"), (FINETUNE_REPO, "Q6_K")] {
        manager
            .queue_download_smart(repo, Some(quantization.to_string()))
            .await
            .unwrap();
        for _ in 0..4 {
            let started = start_next(&manager).await;
            let name = started.item.shard_info.as_ref().unwrap().filename.clone();
            received.push((name, fetch_and_finish(&manager, started).await));
        }
    }

    assert!(
        received.iter().all(|(_, wire)| *wire == 0),
        "nothing came off the network: {received:?}"
    );
    for companion in &companions {
        assert_eq!(std::fs::read(companion).unwrap(), COMPANION_ON_DISK);
    }
    let registered = registrar.registered.lock().unwrap().clone();
    assert_eq!(registered.len(), 2, "both models registered");
    for download in &registered {
        let roles: Vec<_> = download.components.iter().map(|(role, _)| *role).collect();
        assert_eq!(roles, FLUX_ROLES);
        let paths: Vec<_> = download
            .components
            .iter()
            .map(|(_, path)| path.clone())
            .collect();
        assert_eq!(paths, companions, "one file in each role, for both");
        assert_eq!(download.file_paths, None, "the weights are one file");
        assert!(!download.is_sharded, "a companion is not a shard");
    }
    assert_eq!(registered[0].repo_id, FLUX_REPO);
    assert_eq!(registered[1].repo_id, FINETUNE_REPO);
}

// ── What is announced ────────────────────────────────────────────────────

/// A finished image model's download is announced with the roles linked
/// and the reason each companion not linked was not.
#[tokio::test]
async fn the_companions_linked_and_refused_are_announced_at_the_end() {
    gglib_core::paths::isolate_data_root();
    let models = tempfile::tempdir().unwrap();
    let root = models.path();
    let refusal = "the model keeps its t5xxl link to /elsewhere/t5.safetensors";
    let registrar = Arc::new(RecordingRegistrar {
        component_refusals: vec![refusal.to_string()],
        ..RecordingRegistrar::default()
    });
    let announced = Arc::new(super::test_support::Recorded::default());
    let manager = DownloadManagerImpl::new(
        registrar.clone(),
        Arc::new(RepoHub::image(
            ImageFamily::Flux1,
            &[(WEIGHTS, 8)],
            COMPANION_SIZE,
        )),
        announced.clone(),
        DownloadManagerConfig {
            models_directory: Some(root.to_path_buf()),
            ..DownloadManagerConfig::default()
        },
        Arc::new(ImageHeadParser),
    );
    put(
        &root.join("leejet_FLUX.1-schnell-gguf").join(WEIGHTS),
        WEIGHTS_ON_DISK,
    );
    for (folder, file) in [
        ("unsloth_FLUX.1-schnell", "ae.safetensors"),
        ("comfyanonymous_flux_text_encoders", "clip_l.safetensors"),
        (
            "comfyanonymous_flux_text_encoders",
            "t5xxl_fp16.safetensors",
        ),
    ] {
        put(&root.join(folder).join(file), COMPANION_ON_DISK);
    }
    manager
        .queue_download_smart(FLUX_REPO, Some("Q8_0".to_string()))
        .await
        .unwrap();

    for _ in 0..4 {
        let started = start_next(&manager).await;
        fetch_and_finish(&manager, started).await;
    }

    let texts: Vec<String> = announced
        .endings()
        .into_iter()
        .filter_map(|event| match event {
            DownloadEvent::DownloadCompleted { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    let [text] = texts.as_slice() else {
        panic!("one completion: {texts:?}");
    };
    assert!(
        text.ends_with(&format!(
            "Linked its components: vae, clip_l, t5xxl. A component was not linked: {refusal}"
        )),
        "{text}"
    );
}
