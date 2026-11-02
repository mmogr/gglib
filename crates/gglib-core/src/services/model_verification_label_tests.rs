//! A downloaded model added before gglib kept the label its download asked
//! for: the update check and the repair look nothing up by its header's
//! type, which may name other files than the ones it came as.

use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::Utc;

use super::tests::{REPO, Rows, WEIGHTS, row, sha256};
use super::*;
use crate::domain::{FileType, NewModel};
use crate::ports::huggingface::fake_hub::{FakeHub, hub_file};
use crate::ports::{AskedDownloads, FirstBytesParser};
use crate::services::model_projector::tests::OneModelRepo;

/// Model 1, a download of [`REPO`] with no label and a `Q8_0` header, over
/// `hub`: its weights in `dir` are damaged, so a repair would replace them.
fn unlabelled(
    dir: &Path,
    hub: Arc<FakeHub>,
    queued: Arc<AskedDownloads>,
) -> ModelVerificationService {
    std::fs::write(dir.join(WEIGHTS), "damaged").unwrap();
    let mut new = NewModel::new("zeta".to_owned(), dir.join(WEIGHTS), 7.0, Utc::now());
    new.hf_repo_id = Some(REPO.to_owned());
    new.file_type = Some(FileType(7));
    ModelVerificationService::new(
        Arc::new(OneModelRepo(Mutex::new(Model::stored(1, &new)))),
        Arc::new(Rows(vec![row(0, WEIGHTS, &sha256("weights"))])),
        hub,
        queued,
        Arc::new(FirstBytesParser),
    )
}

fn hub() -> Arc<FakeHub> {
    Arc::new(FakeHub {
        weights: vec![hub_file(WEIGHTS, 7, "w-oid")],
        ..Default::default()
    })
}

#[tokio::test]
async fn the_update_check_says_it_cannot_check_rather_than_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub();
    let service = unlabelled(dir.path(), Arc::clone(&hub), Arc::default());

    let refused = service.check_for_updates(1).await.unwrap_err();

    assert!(
        matches!(&refused, RepositoryError::Constraint(why) if why == NO_HUB_LABEL),
        "{refused}"
    );
    assert!(hub.quantizations_asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_repair_is_refused_and_deletes_and_queues_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub();
    let queued = Arc::new(AskedDownloads::default());
    let service = unlabelled(dir.path(), Arc::clone(&hub), Arc::clone(&queued));

    let refused = service.repair_model(1, None).await.unwrap_err();

    assert_eq!(refused, NO_HUB_LABEL);
    assert!(dir.path().join(WEIGHTS).exists(), "the file is kept");
    assert!(queued.asked().is_empty());
    assert!(hub.quantizations_asked.lock().unwrap().is_empty());
}

/// The same model with its Hub file name kept: the variant that holds that
/// file is checked, at its label.
#[tokio::test]
async fn an_unlabelled_model_is_checked_at_the_variant_holding_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub();
    std::fs::write(dir.path().join(WEIGHTS), "damaged").unwrap();
    let mut new = NewModel::new("zeta".to_owned(), dir.path().join(WEIGHTS), 7.0, Utc::now());
    new.hf_repo_id = Some(REPO.to_owned());
    new.hf_filename = Some(WEIGHTS.to_owned());
    let service = ModelVerificationService::new(
        Arc::new(OneModelRepo(Mutex::new(Model::stored(1, &new)))),
        Arc::new(Rows(vec![row(0, WEIGHTS, "w-oid")])),
        Arc::clone(&hub) as Arc<_>,
        Arc::new(AskedDownloads::default()),
        Arc::new(FirstBytesParser),
    );

    let checked = service.check_for_updates(1).await.unwrap();

    assert!(!checked.update_available);
    assert_eq!(*hub.quantizations_asked.lock().unwrap(), ["zeta.Q8_0"]);
}
