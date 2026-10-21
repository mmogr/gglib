//! `queue_upgrade_with`, with a hand-written check in place of the Hub and
//! a recording download manager in place of the queue.

use std::path::PathBuf;
use std::sync::Mutex;

use gglib_core::domain::NewModel;
use gglib_core::ports::{ModelRuntimePort, NoopEmitter, NoopGgufParser, NoopModelRuntime};
use gglib_core::services::AppCore;

use super::*;
use crate::models::ModelDeps;
use crate::test_support::{MockDownloadManager, RunningRuntime};

const REPO: &str = "owner/zeta-GGUF";
const RECORDED: &str = "1111111111111111111111111111111111111111";
const NEWER: &str = "2222222222222222222222222222222222222222";

/// A library holding one model downloaded from [`REPO`] at [`RECORDED`],
/// whose downloads queue on a manager that keeps what it is asked.
struct Library {
    core: Arc<AppCore>,
    calls: Arc<Mutex<Vec<String>>>,
    id: i64,
}

async fn library() -> Library {
    gglib_core::paths::isolate_data_root();
    let pool = gglib_db::setup_test_database().await.expect("a database");
    let manager = MockDownloadManager::new();
    let calls = Arc::clone(&manager.calls);
    let repos = gglib_db::CoreFactory::build_repos(pool);
    let core = Arc::new(AppCore::queueing_on(repos, Arc::new(manager)));
    let mut model = NewModel::new(
        "zeta".to_string(),
        PathBuf::from("/models/old/zeta.Q8_0.gguf"),
        8.0,
        chrono::Utc::now(),
    );
    model.hf_repo_id = Some(REPO.to_string());
    model.hf_commit_sha = Some(RECORDED.to_string());
    model.quantization = Some("Q8_0".to_string());
    let id = core.models().add(model).await.expect("a model").id;
    Library { core, calls, id }
}

impl Library {
    /// `ModelOps` over the library that asks `runtime` what is being served.
    fn ops(&self, runtime: Arc<dyn ModelRuntimePort>) -> ModelOps {
        ModelOps::new(ModelDeps {
            core: Arc::clone(&self.core),
            runtime,
            gguf_parser: Arc::new(NoopGgufParser),
            emitter: Arc::new(NoopEmitter::new()),
        })
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    async fn recorded_sha(&self) -> Option<String> {
        let model = crate::helpers::resolve_model(self.core.models(), self.id);
        model.await.unwrap().hf_commit_sha
    }
}

/// A check that finds `latest` on the Hub.
fn finds(
    recorded: Option<String>,
    latest: &str,
) -> std::future::Ready<anyhow::Result<UpdateCheckResult>> {
    std::future::ready(Ok(UpdateCheckResult {
        has_update: recorded.as_deref() != Some(latest),
        current_sha: recorded,
        latest_sha: latest.to_string(),
    }))
}

/// A model with a newer commit on the Hub has its upgrade queued: its
/// repository and quantization, its id, and the commit the check found. The
/// answer is the download's ID, at once; the row is rewritten when the
/// download completes, not here.
#[tokio::test]
async fn an_upgrade_is_queued_at_the_commit_the_check_found() {
    let library = library().await;
    let ops = library.ops(Arc::new(NoopModelRuntime));

    let queued = ops
        .queue_upgrade_with(library.id, |repo, recorded, _| {
            assert_eq!(repo, REPO);
            finds(recorded, NEWER)
        })
        .await
        .expect("the upgrade is queued");

    assert_eq!(
        queued,
        UpgradeQueued {
            latest_sha: NEWER.to_string(),
            id: Some(format!("{REPO}:Q8_0")),
        }
    );
    let id = library.id;
    assert_eq!(
        library.calls(),
        [format!("queue_upgrade {REPO} Q8_0 {id} {NEWER}")]
    );
    assert_eq!(library.recorded_sha().await.as_deref(), Some(RECORDED));
}

/// A model already at the Hub's commit queues nothing, and the answer has
/// no download to watch.
#[tokio::test]
async fn a_current_model_queues_nothing() {
    let library = library().await;
    let ops = library.ops(Arc::new(NoopModelRuntime));

    let queued = ops
        .queue_upgrade_with(library.id, |_, recorded, _| finds(recorded, RECORDED))
        .await
        .expect("a check");

    assert_eq!(queued.id, None);
    assert_eq!(queued.latest_sha, RECORDED);
    assert!(library.calls().is_empty(), "{:?}", library.calls());
}

/// A model that is being served is refused, as `apply_upgrade` refuses it,
/// before the Hub is asked anything, and nothing is queued.
#[tokio::test]
async fn a_served_model_is_refused_and_nothing_is_queued() {
    let library = library().await;
    let ops = library.ops(Arc::new(RunningRuntime::new(library.id, 9001)));

    let check = |_, _, _| -> std::future::Ready<anyhow::Result<UpdateCheckResult>> {
        panic!("the Hub was asked about a served model")
    };
    let refused = ops.queue_upgrade_with(library.id, check).await.unwrap_err();

    assert!(matches!(refused, GuiError::Conflict(_)), "{refused}");
    assert!(library.calls().is_empty(), "{:?}", library.calls());
}
