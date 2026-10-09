//! Tests for the repair of an image model: a component it links is never
//! deleted, whatever else is repaired.

use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::Utc;

use super::tests::{REPO, Rows, WEIGHTS, row, sha256};
use super::*;
use crate::domain::{ComponentRole, ImageFamily, ModelComponent, NewModel};
use crate::paths::canonical_model_path;
use crate::ports::AskedDownloads;
use crate::ports::huggingface::fake_hub::{FakeHub, hub_file};
use crate::services::model_projector::tests::OneModelRepo;

/// A Flux.1 model in `dir`, downloaded from [`REPO`] at `Q8_0`, whose VAE
/// link is `vae`; its repair queues on `queued`.
fn service(
    dir: &Path,
    vae: &Path,
    rows: Vec<ModelFile>,
    queued: &Arc<AskedDownloads>,
) -> ModelVerificationService {
    let mut new = NewModel::new("flux".to_owned(), dir.join(WEIGHTS), 12.0, Utc::now());
    new.hf_repo_id = Some(REPO.to_owned());
    new.quantization = Some("Q8_0".to_owned());
    new.image_family = Some(ImageFamily::Flux1);
    new.components = vec![ModelComponent {
        role: ComponentRole::Vae,
        path: canonical_model_path(vae).unwrap(),
    }];
    let hub = FakeHub {
        weights: vec![hub_file(WEIGHTS, 7, "w-oid")],
        ..FakeHub::default()
    };
    ModelVerificationService::new(
        Arc::new(OneModelRepo(Mutex::new(Model::stored(1, &new)))),
        Arc::new(Rows(rows)),
        Arc::new(hub),
        queued.clone(),
    )
}

/// A damaged VAE in its own repository's folder, recorded by its absolute
/// path as a download records a companion, and the row for it.
fn damaged_vae(models: &Path) -> (std::path::PathBuf, ModelFile) {
    let folder = models.join("unsloth_FLUX.1-schnell");
    std::fs::create_dir_all(&folder).unwrap();
    let vae = folder.join("ae.safetensors");
    std::fs::write(&vae, "damaged").unwrap();
    let recorded = canonical_model_path(&vae).unwrap();
    let row = row(1, &recorded.to_string_lossy(), &sha256("vae"));
    (vae, row)
}

/// The only unhealthy file is the model's VAE: it is named with the role
/// and the command that links another, left on disk, and nothing is queued.
#[tokio::test]
async fn a_repair_never_deletes_a_component_the_model_links() {
    let models = tempfile::tempdir().unwrap();
    let dir = models.path().join("owner_zeta-GGUF");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(WEIGHTS), "weights").unwrap();
    let (vae, vae_row) = damaged_vae(models.path());
    let rows = vec![row(0, WEIGHTS, &sha256("weights")), vae_row];
    let queued = Arc::new(AskedDownloads::default());

    // Every file, and the VAE asked for by its index, each by a service of
    // its own: a repair releases the model's lock in a task spawned as it
    // answers, which may not have run before the next is asked.
    for asked in [None, Some(vec![1])] {
        let service = service(&dir, &vae, rows.clone(), &queued);
        let refused = service
            .repair_model(1, asked.clone())
            .await
            .expect_err("nothing to repair but the VAE");

        assert!(refused.contains("ae.safetensors is unhealthy"), "{refused}");
        assert!(refused.contains("the model's vae component"), "{refused}");
        assert!(
            refused.contains("`gglib model update 1 --component vae=<path>`"),
            "{refused}"
        );
        assert_eq!(
            std::fs::read_to_string(&vae).unwrap(),
            "damaged",
            "{asked:?}"
        );
        assert!(queued.asked().is_empty());
    }
}

/// Damaged weights beside the damaged VAE: the weights are deleted and
/// fetched again, and the VAE stays where it is.
#[tokio::test]
async fn a_repair_of_the_weights_leaves_a_damaged_component_in_place() {
    let models = tempfile::tempdir().unwrap();
    let dir = models.path().join("owner_zeta-GGUF");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(WEIGHTS), "damaged").unwrap();
    let (vae, vae_row) = damaged_vae(models.path());
    let rows = vec![row(0, WEIGHTS, &sha256("weights")), vae_row];
    let queued = Arc::new(AskedDownloads::default());
    let service = service(&dir, &vae, rows, &queued);

    let started = service.repair_model(1, None).await.unwrap();

    assert_eq!(started.files, [WEIGHTS]);
    assert!(!dir.join(WEIGHTS).exists(), "the weights are fetched again");
    assert!(vae.exists(), "the VAE is never deleted");
    assert_eq!(queued.asked().len(), 1);
}
