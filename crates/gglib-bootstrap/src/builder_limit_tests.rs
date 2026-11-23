//! A download queued through the wired core while Hugging Face's rate limit
//! refuses what choosing it reads.

use gglib_core::download::DownloadError;
use gglib_core::hub_limit::HubBucket;
use gglib_core::ports::huggingface::fake_hub::{FakeHub, RoleParser, hub_file};
use gglib_core::ports::{DownloadManagerConfig, NoopEmitter};

use super::*;

/// The core is wired with the gate the download manager keeps, which the
/// Hub client keeps too ([`holders`]).
#[tokio::test]
async fn the_wired_core_keeps_the_download_managers_gate() {
    let dir = tempfile::tempdir().unwrap();
    let gguf_parser: Arc<dyn GgufParserPort> = Arc::new(RoleParser);
    let pool = gglib_db::setup_database(&dir.path().join("gglib.db"), gguf_parser.as_ref())
        .await
        .unwrap();
    let config = BootstrapConfig::for_db(dir.path().join("gglib.db"));
    let Holders {
        download_config,
        core,
        ..
    } = holders(&config, || None);
    let gate = Arc::clone(&download_config.hub_gate);
    let repos = CoreFactory::build_repos(pool.clone());

    let built = wire(
        pool,
        repos,
        Arc::new(FakeHub::default()),
        gguf_parser,
        download_config,
        core,
        Arc::new(NoopEmitter::new()),
    );

    assert!(Arc::ptr_eq(&built.app.hub_gate(), &gate));
}

/// The head that says what the asked-for variant is was refused: the queue
/// answers the limit, with its allowance, and queues nothing.
#[tokio::test]
async fn a_queue_the_hubs_limit_refuses_is_the_limit_and_queues_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let gguf_parser: Arc<dyn GgufParserPort> = Arc::new(RoleParser);
    let pool = gglib_db::setup_database(&dir.path().join("gglib.db"), gguf_parser.as_ref())
        .await
        .unwrap();
    let hub = FakeHub {
        weights: vec![hub_file("zeta.Q8_0.gguf", 7, "w-oid")],
        heads_limited: true,
        ..FakeHub::default()
    };
    let repos = CoreFactory::build_repos(pool.clone());
    let built = wire(
        pool,
        repos,
        Arc::new(hub),
        gguf_parser,
        DownloadManagerConfig {
            models_directory: Some(dir.path().join("models")),
            ..DownloadManagerConfig::default()
        },
        CoreHub::default(),
        Arc::new(NoopEmitter::new()),
    );

    let refused = Arc::clone(&built.downloads)
        .queue_smart("owner/zeta-GGUF".to_owned(), Some("Q8_0".to_owned()), None)
        .await;

    let Err(limited @ DownloadError::HubLimited { .. }) = refused else {
        panic!("expected the Hub's rate limit, got {refused:?}");
    };
    assert_eq!(
        limited.hub_limit().map(|(bucket, _)| bucket),
        Some(HubBucket::Resolvers)
    );
    let queue = built.downloads.get_queue_snapshot().await.unwrap();
    assert!(queue.active.is_none() && queue.waiting.is_empty());
}
