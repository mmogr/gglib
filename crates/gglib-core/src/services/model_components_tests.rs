//! Tests for [`super`]: a retag reading a family from a model's file, a
//! component linked by hand, a download's components linked, and the
//! projector's link still read from its header alone.
//!
//! The files are the measured goldens' text written to disk, and
//! [`GoldenParser`] reads them back as the real parser reads a weights file's
//! tensor table; the binary round trip of the same goldens through the
//! fixture writers is pinned in `gglib-gguf`, which this crate cannot depend
//! on.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;

use super::super::ModelService;
use super::super::model_projector::tests::OneModelRepo;
use super::link_downloaded_components;
use crate::domain::image_family_goldens::{self, Golden};
use crate::domain::{ComponentRole, ImageFamily, Model, ModelComponent, NewModel};
use crate::download::GgufFileRole;
use crate::ports::{
    GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort, ModelRepository,
    RepositoryError, TensorTable,
};
use crate::services::LinkError;

/// Reads a file's tensor table from the golden text it holds; a file that
/// is not there is not found, as the real parser says.
pub(crate) struct GoldenParser;

impl GgufParserPort for GoldenParser {
    fn parse(&self, file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        Err(GgufParseError::InvalidFormat(format!(
            "no header is read here, and {} was asked",
            file_path.display()
        )))
    }
    fn detect_capabilities(&self, _metadata: &GgufMetadata) -> GgufCapabilities {
        GgufCapabilities::empty()
    }
    fn tensor_table(&self, path: &Path) -> Result<TensorTable, GgufParseError> {
        let text = std::fs::read_to_string(path)
            .map_err(|_| GgufParseError::NotFound(path.display().to_string()))?;
        let name = path.file_name().unwrap().to_string_lossy();
        Ok(image_family_goldens::parse(&name, &text))
    }
    fn tensor_table_of_head(&self, _head: &[u8]) -> Result<TensorTable, GgufParseError> {
        Err(GgufParseError::InvalidFormat(
            "no head is read here".to_owned(),
        ))
    }
}

/// Write `golden`'s text into `dir` under the measured file's name.
pub(crate) fn write_golden(dir: &Path, golden: Golden) -> PathBuf {
    let path = dir.join(golden.file_name());
    std::fs::write(&path, golden.text()).unwrap();
    path
}

/// A service over model 1, whose weights are `file` and whose family is
/// `family`.
fn service(file: &Path, family: Option<ImageFamily>) -> (ModelService, Arc<OneModelRepo>) {
    let mut new = NewModel::new("model".to_owned(), file.to_path_buf(), 12.0, Utc::now());
    new.image_family = family;
    let repo = Arc::new(OneModelRepo(Mutex::new(Model::stored(1, &new))));
    (ModelService::new(repo.clone()), repo)
}

fn stored_family(repo: &OneModelRepo) -> Option<ImageFamily> {
    repo.0.lock().unwrap().image_family
}

#[tokio::test]
async fn a_retag_fills_a_missing_family_from_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_golden(dir.path(), Golden::FluxSchnellQ8);
    let (service, repo) = service(&file, None);

    let diff = service
        .retag_model(1, &GoldenParser, false)
        .await
        .unwrap()
        .expect("finding a family is a change");

    assert_eq!(diff.family_found, Some(ImageFamily::Flux1));
    assert!(diff.is_changed());
    assert_eq!(stored_family(&repo), Some(ImageFamily::Flux1));
}

/// A family already set is never changed, even by a file that now reads as
/// another, and in a full rebuild too.
#[tokio::test]
async fn a_retag_never_changes_a_set_family() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_golden(dir.path(), Golden::QwenImage21Q8);
    let (service, repo) = service(&file, Some(ImageFamily::Flux1));

    for full in [false, true] {
        let diff = service.retag_model(1, &GoldenParser, full).await.unwrap();
        assert_eq!(diff, None, "full = {full}");
        assert_eq!(stored_family(&repo), Some(ImageFamily::Flux1));
    }
}

/// Tags are re-derived from stored metadata, so the file does not have to
/// exist; a missing one is no error and finds no family.
#[tokio::test]
async fn a_retag_skips_a_missing_file_without_error() {
    let dir = tempfile::tempdir().unwrap();
    let (service, repo) = service(&dir.path().join("gone.gguf"), None);

    let diff = service.retag_model(1, &GoldenParser, false).await.unwrap();

    assert_eq!(diff, None);
    assert_eq!(stored_family(&repo), None);
}

#[tokio::test]
async fn a_retag_of_a_component_or_a_chat_file_finds_no_family() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_golden(dir.path(), Golden::Qwen3Vl8bQ8);
    let (service, repo) = service(&file, None);

    let diff = service.retag_model(1, &GoldenParser, true).await.unwrap();

    assert_eq!(diff, None);
    assert_eq!(stored_family(&repo), None);
}

// ── set_component ───────────────────────────────────────────────────────────

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap()
}

fn links(repo: &OneModelRepo) -> Vec<ModelComponent> {
    repo.0.lock().unwrap().components.clone()
}

/// Every recipe component is linked from its measured file, under its
/// canonical path, in role order whatever order they were linked in.
#[tokio::test]
async fn each_recipe_component_links_from_its_file() {
    let dir = tempfile::tempdir().unwrap();
    for (family, files) in [
        (
            ImageFamily::Flux1,
            vec![
                (ComponentRole::T5xxl, Golden::T5xxl),
                (ComponentRole::Vae, Golden::FluxVae),
                (ComponentRole::ClipL, Golden::ClipL),
            ],
        ),
        (
            ImageFamily::QwenImage21,
            vec![
                (ComponentRole::Llm, Golden::Qwen3Vl8bQ8),
                (ComponentRole::Vae, Golden::QwenImage21Vae),
            ],
        ),
    ] {
        let (service, repo) = service(&dir.path().join("main.gguf"), Some(family));
        let mut expected = Vec::new();
        for (role, golden) in files {
            let file = write_golden(dir.path(), golden);
            service
                .set_component(1, role, Some(&file), &GoldenParser)
                .await
                .unwrap_or_else(|e| panic!("{family} {role}: {e}"));
            expected.push(ModelComponent {
                role,
                path: canonical(&file),
            });
        }
        expected.sort_by_key(|c| c.role);
        assert_eq!(links(&repo), expected, "{family}");
        assert!(repo.0.lock().unwrap().missing_components().is_empty());
    }
}

#[tokio::test]
async fn a_chat_model_takes_no_component() {
    let dir = tempfile::tempdir().unwrap();
    let file = write_golden(dir.path(), Golden::FluxVae);
    let (service, repo) = service(&dir.path().join("qwen3.gguf"), None);

    for path in [Some(file.as_path()), None] {
        let refused = service
            .set_component(1, ComponentRole::Vae, path, &GoldenParser)
            .await
            .unwrap_err();
        assert!(
            matches!(refused, LinkError::NotAnImageModel(_)),
            "{refused}"
        );
        assert_eq!(
            refused.to_string(),
            "model is not an image model, so it takes no components"
        );
    }
    assert!(links(&repo).is_empty());
}

#[tokio::test]
async fn a_role_outside_the_recipe_is_refused_before_reading() {
    let dir = tempfile::tempdir().unwrap();
    let (service, repo) = service(&dir.path().join("flux.gguf"), Some(ImageFamily::Flux1));

    let refused = service
        .set_component(
            1,
            ComponentRole::Llm,
            Some(&dir.path().join("never-read")),
            &GoldenParser,
        )
        .await
        .unwrap_err();

    assert!(
        matches!(
            refused,
            LinkError::NotInRecipe {
                family: ImageFamily::Flux1,
                role: ComponentRole::Llm
            }
        ),
        "{refused}"
    );
    assert_eq!(
        refused.to_string(),
        "the Flux.1 recipe has no llm component"
    );
    assert!(links(&repo).is_empty());
}

/// A file whose tensors are another role's, or another family's VAE, is
/// refused by name with what was expected, and nothing is written.
#[tokio::test]
async fn a_file_of_the_wrong_role_is_refused_with_what_was_expected() {
    let dir = tempfile::tempdir().unwrap();
    let (service, repo) = service(&dir.path().join("flux.gguf"), Some(ImageFamily::Flux1));
    let qwen_vae = write_golden(dir.path(), Golden::QwenImage21Vae);
    let clip = write_golden(dir.path(), Golden::ClipL);

    let refused = service
        .set_component(1, ComponentRole::Vae, Some(&qwen_vae), &GoldenParser)
        .await
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        format!(
            "{} is not a vae for this model: expected a Flux.1 VAE: decoder.conv_in.weight \
             with 16 input channels, and encoder.conv_in.weight",
            canonical(&qwen_vae).display()
        )
    );
    let refused = service
        .set_component(1, ComponentRole::T5xxl, Some(&clip), &GoldenParser)
        .await
        .unwrap_err();
    assert!(
        matches!(&refused, LinkError::WrongRole { role: ComponentRole::T5xxl, path, .. } if *path == canonical(&clip)),
        "{refused}"
    );
    assert!(links(&repo).is_empty());
}

#[tokio::test]
async fn a_missing_component_file_is_named_as_a_component() {
    let dir = tempfile::tempdir().unwrap();
    let (service, _repo) = service(&dir.path().join("flux.gguf"), Some(ImageFamily::Flux1));
    let missing = dir.path().join("ae.safetensors");

    let refused = service
        .set_component(1, ComponentRole::Vae, Some(&missing), &GoldenParser)
        .await
        .unwrap_err();

    assert!(
        refused
            .to_string()
            .starts_with(&format!("no component file at {}: ", missing.display())),
        "{refused}"
    );
}

/// A link is stored under the file's canonical path; `None` clears the role
/// and leaves the others.
#[tokio::test]
async fn a_link_is_stored_canonical_and_none_clears_it() {
    let dir = tempfile::tempdir().unwrap();
    let vae = write_golden(dir.path(), Golden::FluxVae);
    let clip = write_golden(dir.path(), Golden::ClipL);
    let (service, repo) = service(&dir.path().join("flux.gguf"), Some(ImageFamily::Flux1));

    let linked = service
        .set_component(
            1,
            ComponentRole::Vae,
            Some(&dir.path().join(".").join("ae.safetensors")),
            &GoldenParser,
        )
        .await
        .unwrap();
    service
        .set_component(1, ComponentRole::ClipL, Some(&clip), &GoldenParser)
        .await
        .unwrap();
    assert_eq!(linked.components[0].path, canonical(&vae));

    let cleared = service
        .set_component(1, ComponentRole::Vae, None, &GoldenParser)
        .await
        .unwrap();
    assert_eq!(
        cleared.components,
        vec![ModelComponent {
            role: ComponentRole::ClipL,
            path: canonical(&clip),
        }]
    );
    assert_eq!(links(&repo), cleared.components);
}

/// A link made through a symlinked folder.
#[cfg(unix)]
mod through_a_symlink {
    use super::*;

    #[tokio::test]
    async fn a_link_through_a_symlinked_folder_is_stored_canonical() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let vae = write_golden(&real, Golden::FluxVae);
        let via = dir.path().join("via");
        std::os::unix::fs::symlink(&real, &via).unwrap();
        let (service, repo) = service(&dir.path().join("flux.gguf"), Some(ImageFamily::Flux1));

        service
            .set_component(
                1,
                ComponentRole::Vae,
                Some(&via.join("ae.safetensors")),
                &GoldenParser,
            )
            .await
            .unwrap();

        assert_eq!(links(&repo)[0].path, canonical(&vae));
    }
}

// ── The projector's link reads the header only ──────────────────────────────

/// Says every header is a projector's, and fails the test if a tensor table
/// is read.
struct HeaderOnlyParser;

impl GgufParserPort for HeaderOnlyParser {
    fn parse(&self, _file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        Ok(GgufMetadata {
            role: GgufFileRole::Projector,
            ..GgufMetadata::default()
        })
    }
    fn detect_capabilities(&self, _metadata: &GgufMetadata) -> GgufCapabilities {
        GgufCapabilities::empty()
    }
    fn tensor_table(&self, path: &Path) -> Result<TensorTable, GgufParseError> {
        panic!("a projector's tensor table was read: {}", path.display());
    }
    fn tensor_table_of_head(&self, _head: &[u8]) -> Result<TensorTable, GgufParseError> {
        panic!("a projector's head was read");
    }
}

/// ADR 0015 decision 1: one header check links a projector.
#[tokio::test]
async fn the_projector_link_reads_the_header_and_never_the_tensor_table() {
    let dir = tempfile::tempdir().unwrap();
    let projector = dir.path().join("mmproj-F16.gguf");
    std::fs::write(&projector, b"projector").unwrap();
    let (service, _repo) = service(&dir.path().join("qwen.gguf"), None);

    let linked = service
        .set_projector(1, Some(&projector), &HeaderOnlyParser)
        .await
        .unwrap();

    assert_eq!(linked.projector_path, Some(canonical(&projector)));
}

// ── A download's components ─────────────────────────────────────────────────

/// Holds `held` as the model at every path, or no model.
struct HeldRepo(Option<Model>);

#[async_trait]
impl ModelRepository for HeldRepo {
    async fn list(&self) -> Result<Vec<Model>, RepositoryError> {
        Ok(self.0.iter().cloned().collect())
    }
    async fn get_by_id(&self, id: i64) -> Result<Model, RepositoryError> {
        Err(RepositoryError::NotFound(format!("id={id}")))
    }
    async fn get_by_name(&self, name: &str) -> Result<Model, RepositoryError> {
        Err(RepositoryError::NotFound(format!("name={name}")))
    }
    async fn find_by_path(&self, _path: &Path) -> Result<Option<Model>, RepositoryError> {
        Ok(self.0.clone())
    }
    async fn insert(&self, _model: &NewModel) -> Result<Model, RepositoryError> {
        Err(RepositoryError::Storage("read-only".to_owned()))
    }
    async fn update(&self, _model: &Model) -> Result<(), RepositoryError> {
        Err(RepositoryError::Storage("read-only".to_owned()))
    }
    async fn delete(&self, _id: i64) -> Result<(), RepositoryError> {
        Ok(())
    }
}

fn downloaded(dir: &Path, family: Option<ImageFamily>) -> NewModel {
    let mut new = NewModel::new(
        "flux1-schnell".to_owned(),
        dir.join("flux1-schnell-q8_0.gguf"),
        12.0,
        Utc::now(),
    );
    new.image_family = family;
    new
}

/// Each file that fits is linked; one that does not is named with the
/// reason, and the model is still registered with the rest.
#[tokio::test]
async fn a_downloads_components_are_linked_and_a_refused_one_is_named() {
    let dir = tempfile::tempdir().unwrap();
    let vae = write_golden(dir.path(), Golden::FluxVae);
    let clip = write_golden(dir.path(), Golden::ClipL);
    let wrong = write_golden(dir.path(), Golden::QwenImage21Vae);
    let mut model = downloaded(dir.path(), Some(ImageFamily::Flux1));

    let refusals = link_downloaded_components(
        &HeldRepo(None),
        &mut model,
        &[
            (ComponentRole::ClipL, clip.clone()),
            (ComponentRole::Vae, vae.clone()),
            (ComponentRole::T5xxl, wrong.clone()),
        ],
        &GoldenParser,
    )
    .await;

    assert_eq!(
        model.components,
        vec![
            ModelComponent {
                role: ComponentRole::Vae,
                path: canonical(&vae),
            },
            ModelComponent {
                role: ComponentRole::ClipL,
                path: canonical(&clip),
            },
        ]
    );
    assert_eq!(refusals.len(), 1, "{refusals:?}");
    assert!(
        refusals[0].starts_with(&format!(
            "{} is not a t5xxl for this model",
            canonical(&wrong).display()
        )),
        "{refusals:?}"
    );
}

/// A model the library holds keeps every link it has: one to another file
/// is said, one to the same file is not, and the roles it lacked are linked.
#[tokio::test]
async fn a_held_link_is_kept_and_the_answer_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let vae = write_golden(dir.path(), Golden::FluxVae);
    let clip = write_golden(dir.path(), Golden::ClipL);
    let t5 = write_golden(dir.path(), Golden::T5xxl);
    let mut held = downloaded(dir.path(), Some(ImageFamily::Flux1));
    held.components = vec![
        ModelComponent {
            role: ComponentRole::Vae,
            path: PathBuf::from("/models/my-own-ae.safetensors"),
        },
        ModelComponent {
            role: ComponentRole::ClipL,
            path: canonical(&clip),
        },
    ];
    let repo = HeldRepo(Some(Model::stored(1, &held)));
    let mut model = downloaded(dir.path(), Some(ImageFamily::Flux1));

    let refusals = link_downloaded_components(
        &repo,
        &mut model,
        &[
            (ComponentRole::Vae, vae),
            (ComponentRole::ClipL, clip),
            (ComponentRole::T5xxl, t5.clone()),
        ],
        &GoldenParser,
    )
    .await;

    assert_eq!(
        refusals,
        vec!["the model keeps its vae link to /models/my-own-ae.safetensors".to_owned()]
    );
    assert_eq!(
        model.components,
        vec![ModelComponent {
            role: ComponentRole::T5xxl,
            path: canonical(&t5),
        }]
    );
}

#[tokio::test]
async fn a_chat_download_links_no_component() {
    let dir = tempfile::tempdir().unwrap();
    let vae = write_golden(dir.path(), Golden::FluxVae);
    let mut model = downloaded(dir.path(), None);

    let refusals = link_downloaded_components(
        &HeldRepo(None),
        &mut model,
        &[(ComponentRole::Vae, vae)],
        &GoldenParser,
    )
    .await;

    assert_eq!(
        refusals,
        vec!["flux1-schnell is not an image model, so it takes no components".to_owned()]
    );
    assert!(model.components.is_empty());
    let none = link_downloaded_components(&HeldRepo(None), &mut model, &[], &GoldenParser).await;
    assert!(none.is_empty());
}

/// A download offering a file in a role the family draws without is told
/// so, as a hand-made link is.
#[tokio::test]
async fn a_downloads_component_outside_the_recipe_is_refused_by_the_recipe() {
    let dir = tempfile::tempdir().unwrap();
    let llm = write_golden(dir.path(), Golden::Qwen3Vl8bQ8);
    let mut model = downloaded(dir.path(), Some(ImageFamily::Flux1));

    let refusals = link_downloaded_components(
        &HeldRepo(None),
        &mut model,
        &[(ComponentRole::Llm, llm)],
        &GoldenParser,
    )
    .await;

    assert_eq!(
        refusals,
        vec!["the Flux.1 recipe has no llm component".to_owned()]
    );
    assert!(model.components.is_empty());
}
