//! Tests for [`ModelService::set_projector`].

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;

use super::*;
use crate::domain::NewModel;
use crate::download::GgufFileRole;
use crate::ports::{GgufCapabilities, GgufMetadata, GgufParseError, ModelRepository};

/// Holds model 1 and nothing else.
pub(crate) struct OneModelRepo(pub(crate) Mutex<Model>);

#[async_trait]
impl ModelRepository for OneModelRepo {
    async fn list(&self) -> Result<Vec<Model>, RepositoryError> {
        Ok(vec![self.0.lock().unwrap().clone()])
    }
    async fn get_by_id(&self, id: i64) -> Result<Model, RepositoryError> {
        let model = self.0.lock().unwrap().clone();
        if model.id == id {
            Ok(model)
        } else {
            Err(RepositoryError::NotFound(format!("id={id}")))
        }
    }
    async fn get_by_name(&self, name: &str) -> Result<Model, RepositoryError> {
        Err(RepositoryError::NotFound(format!("name={name}")))
    }
    async fn find_by_path(&self, _path: &Path) -> Result<Option<Model>, RepositoryError> {
        Ok(None)
    }
    async fn insert(&self, _model: &NewModel) -> Result<Model, RepositoryError> {
        Err(RepositoryError::Storage("read-only".to_owned()))
    }
    async fn update(&self, model: &Model) -> Result<(), RepositoryError> {
        self.0.lock().unwrap().clone_from(model);
        Ok(())
    }
    async fn delete(&self, _id: i64) -> Result<(), RepositoryError> {
        Ok(())
    }
}

/// Reads a file's role from its first bytes, as the real parser reads it from
/// the header: `projector`, `weights`, or anything else for "not a GGUF".
pub(crate) struct FirstBytesParser;

impl GgufParserPort for FirstBytesParser {
    fn parse(&self, file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        let bytes = std::fs::read(file_path).map_err(|e| GgufParseError::Io(e.to_string()))?;
        let role = if bytes.starts_with(b"projector") {
            GgufFileRole::Projector
        } else if bytes.starts_with(b"weights") {
            GgufFileRole::Weights
        } else {
            return Err(GgufParseError::InvalidFormat("no GGUF magic".to_owned()));
        };
        Ok(GgufMetadata {
            role,
            ..Default::default()
        })
    }
    fn detect_capabilities(&self, _metadata: &GgufMetadata) -> GgufCapabilities {
        GgufCapabilities::empty()
    }
}

/// Fails the test if a header is read at all.
struct NeverReadParser;

impl GgufParserPort for NeverReadParser {
    fn parse(&self, file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        panic!("no header should be read, and {} was", file_path.display());
    }
    fn detect_capabilities(&self, _metadata: &GgufMetadata) -> GgufCapabilities {
        GgufCapabilities::empty()
    }
}

/// A service over model 1, linked to `projector` to begin with.
fn service(projector: Option<&str>) -> (ModelService, Arc<OneModelRepo>) {
    let mut new = NewModel::new(
        "qwen".to_owned(),
        PathBuf::from("/models/qwen.Q8_0.gguf"),
        27.0,
        Utc::now(),
    );
    new.projector_path = projector.map(PathBuf::from);
    let repo = Arc::new(OneModelRepo(Mutex::new(Model::stored(1, &new))));
    (ModelService::new(repo.clone()), repo)
}

fn stored(repo: &OneModelRepo) -> Option<PathBuf> {
    repo.0.lock().unwrap().projector_path.clone()
}

/// A file named `name` in `dir` whose first bytes are `content`.
fn file(dir: &tempfile::TempDir, name: &str, content: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).unwrap();
    path
}

/// The header decides, not the name: this file's name carries no `mmproj`.
#[tokio::test]
async fn a_file_whose_header_says_projector_is_linked() {
    let dir = tempfile::tempdir().unwrap();
    let path = file(&dir, "vision-tower.gguf", "projector");
    let (service, repo) = service(None);

    let model = service
        .set_projector(1, Some(&path), &FirstBytesParser)
        .await
        .unwrap();

    let canonical = std::fs::canonicalize(&path).unwrap();
    assert_eq!(model.projector_path.as_deref(), Some(canonical.as_path()));
    assert!(model.image_input());
    assert_eq!(stored(&repo), Some(canonical));
}

/// Named like a projector, and refused all the same, with the file named in
/// the refusal.
#[tokio::test]
async fn a_weights_file_is_refused_by_name_and_nothing_is_stored() {
    let dir = tempfile::tempdir().unwrap();
    let path = file(&dir, "mmproj-F16.gguf", "weights");
    let (service, repo) = service(Some("/models/old-mmproj.gguf"));

    let refused = service
        .set_projector(1, Some(&path), &FirstBytesParser)
        .await
        .unwrap_err();

    let canonical = std::fs::canonicalize(&path).unwrap();
    assert!(
        matches!(&refused, ProjectorError::Weights(named) if *named == canonical),
        "{refused:?}"
    );
    assert!(refused.to_string().contains("mmproj-F16.gguf"), "{refused}");
    assert_eq!(
        stored(&repo),
        Some(PathBuf::from("/models/old-mmproj.gguf"))
    );
}

#[tokio::test]
async fn none_clears_the_link_without_reading_a_file() {
    let (service, repo) = service(Some("/models/mmproj-F16.gguf"));

    let model = service
        .set_projector(1, None, &NeverReadParser)
        .await
        .unwrap();

    assert_eq!(model.projector_path, None);
    assert!(!model.image_input());
    assert_eq!(stored(&repo), None);
}

/// Two spellings of one file are one link: the stored path is the resolved
/// one, whatever the caller typed.
#[tokio::test]
async fn the_path_is_stored_in_its_canonical_form() {
    let dir = tempfile::tempdir().unwrap();
    let path = file(&dir, "mmproj-F16.gguf", "projector");
    std::fs::create_dir(dir.path().join("beside")).unwrap();
    let spelled = dir.path().join("beside").join("..").join("mmproj-F16.gguf");
    let (service, repo) = service(None);

    service
        .set_projector(1, Some(&spelled), &FirstBytesParser)
        .await
        .unwrap();

    let canonical = std::fs::canonicalize(&path).unwrap();
    assert_ne!(spelled, canonical);
    assert_eq!(stored(&repo), Some(canonical));
}

#[tokio::test]
async fn a_path_with_no_file_is_refused_before_any_header_is_read() {
    let (service, repo) = service(None);
    let missing = Path::new("/nonexistent/mmproj-F16.gguf");

    let refused = service
        .set_projector(1, Some(missing), &NeverReadParser)
        .await
        .unwrap_err();

    assert!(
        matches!(&refused, ProjectorError::Missing { path, .. } if path == missing),
        "{refused:?}"
    );
    assert_eq!(stored(&repo), None);
}

#[tokio::test]
async fn a_file_that_is_not_a_gguf_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = file(&dir, "mmproj-F16.gguf", "<html>");
    let (service, repo) = service(None);

    let refused = service
        .set_projector(1, Some(&path), &FirstBytesParser)
        .await
        .unwrap_err();

    assert!(
        matches!(refused, ProjectorError::Unreadable { .. }),
        "{refused:?}"
    );
    assert_eq!(stored(&repo), None);
}

#[tokio::test]
async fn an_unknown_model_is_a_repository_error() {
    let (service, _repo) = service(None);

    let refused = service
        .set_projector(2, None, &NeverReadParser)
        .await
        .unwrap_err();

    assert!(
        matches!(
            refused,
            ProjectorError::Repository(RepositoryError::NotFound(_))
        ),
        "{refused:?}"
    );
}

/// A caller that speaks `CoreError` keeps the two kinds apart: a refused file
/// is the caller's input, a repository failure is not.
#[test]
fn a_refusal_becomes_a_validation_error_and_a_repository_error_stays_one() {
    let refused: CoreError = ProjectorError::Weights(PathBuf::from("/m/x.Q8_0.gguf")).into();
    assert!(
        matches!(&refused, CoreError::Validation(text) if text.contains("/m/x.Q8_0.gguf")),
        "{refused:?}"
    );

    let lost: CoreError =
        ProjectorError::Repository(RepositoryError::NotFound("id=9".to_owned())).into();
    assert!(
        matches!(lost, CoreError::Repository(RepositoryError::NotFound(_))),
        "{lost:?}"
    );
}
