//! Tests for the default image model check in [`SettingsService::update`].

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use chrono::Utc;

use super::*;
use crate::domain::{ImageFamily, Model, NewModel};
use crate::ports::InMemorySettings;

/// Model 1, `qwen`, chats; model 2, `flux`, draws. Counts id lookups.
struct Library {
    models: Vec<Model>,
    lookups: AtomicUsize,
}

impl Library {
    fn new() -> Self {
        let model = |id, name: &str, family| {
            let mut new = NewModel::new(
                name.to_owned(),
                PathBuf::from(format!("/models/{name}.gguf")),
                8.0,
                Utc::now(),
            );
            new.image_family = family;
            Model::stored(id, &new)
        };
        Self {
            models: vec![
                model(1, "qwen", None),
                model(2, "flux", Some(ImageFamily::Flux1)),
            ],
            lookups: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl ModelRepository for Library {
    async fn list(&self) -> Result<Vec<Model>, RepositoryError> {
        Ok(self.models.clone())
    }
    async fn get_by_id(&self, id: i64) -> Result<Model, RepositoryError> {
        self.lookups.fetch_add(1, Ordering::SeqCst);
        let found = self.models.iter().find(|m| m.id == id).cloned();
        found.ok_or_else(|| RepositoryError::NotFound(format!("id={id}")))
    }
    async fn get_by_name(&self, name: &str) -> Result<Model, RepositoryError> {
        Err(RepositoryError::NotFound(format!("name={name}")))
    }
    async fn find_by_path(&self, _path: &Path) -> Result<Option<Model>, RepositoryError> {
        Ok(None)
    }
    async fn insert(&self, _model: &NewModel) -> Result<Model, RepositoryError> {
        unimplemented!("not exercised by these tests")
    }
    async fn update(&self, _model: &Model) -> Result<(), RepositoryError> {
        unimplemented!("not exercised by these tests")
    }
    async fn delete(&self, _id: i64) -> Result<(), RepositoryError> {
        unimplemented!("not exercised by these tests")
    }
}

fn service() -> (SettingsService, Arc<Library>) {
    let library = Arc::new(Library::new());
    let models: Arc<dyn ModelRepository> = library.clone();
    let service = SettingsService::new(Arc::new(InMemorySettings::default())).with_models(models);
    (service, library)
}

fn image_model(id: Option<i64>) -> SettingsUpdate {
    SettingsUpdate {
        default_image_model_id: Some(id),
        ..Default::default()
    }
}

/// A model that draws is stored, and read back as stored.
#[tokio::test]
async fn an_image_model_is_stored_as_the_default_image_model() {
    let (service, _) = service();

    let stored = service.update(image_model(Some(2))).await.unwrap();

    assert_eq!(stored.default_image_model_id, Some(2));
    assert_eq!(service.get().await.unwrap().default_image_model_id, Some(2));
}

/// A chat model is refused in one sentence that names it, and nothing in
/// the update is stored, not even the valid field beside it.
#[tokio::test]
async fn a_model_that_does_not_draw_is_refused_with_a_sentence_and_nothing_is_stored() {
    let (service, _) = service();
    let mut update = image_model(Some(1));
    update.mcp_drawing = Some(Some(true));

    let refused = service.update(update).await.unwrap_err();

    assert_eq!(
        refused.to_string(),
        "Model 1 (qwen) does not draw images, so it cannot be the default image model"
    );
    assert!(
        matches!(
            refused,
            CoreError::Settings(SettingsError::NotAnImageModel { id: 1, .. })
        ),
        "{refused:?}"
    );
    let after = service.get().await.unwrap();
    assert_eq!(after.default_image_model_id, None);
    assert_eq!(after.mcp_drawing, None);
}

/// An id the library does not hold is refused, and says so.
#[tokio::test]
async fn an_id_no_model_has_is_refused_with_a_sentence() {
    let (service, _) = service();

    let refused = service.update(image_model(Some(99))).await.unwrap_err();

    assert_eq!(
        refused.to_string(),
        "No model has id 99, so it cannot be the default image model"
    );
    assert_eq!(service.get().await.unwrap().default_image_model_id, None);
}

/// Absent means none: clearing the setting stores `None` and looks up no
/// model, and an update that leaves the field alone looks up none either,
/// so a chosen model removed later never blocks another setting's write.
#[tokio::test]
async fn clearing_or_leaving_the_setting_reads_no_model() {
    let (service, library) = service();
    service.update(image_model(Some(2))).await.unwrap();
    assert_eq!(library.lookups.load(Ordering::SeqCst), 1);

    let other = SettingsUpdate {
        proxy_port: Some(Some(9191)),
        ..Default::default()
    };
    let kept = service.update(other).await.unwrap();
    assert_eq!(kept.default_image_model_id, Some(2));

    let cleared = service.update(image_model(None)).await.unwrap();
    assert_eq!(cleared.default_image_model_id, None);
    assert_eq!(
        library.lookups.load(Ordering::SeqCst),
        1,
        "only the write looked"
    );
}
