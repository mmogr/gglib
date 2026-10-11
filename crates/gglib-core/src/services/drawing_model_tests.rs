//! The model rule's table, row by row: a named model first and never
//! replaced, then the settings' default, then the only complete one.

use async_trait::async_trait;

use super::resolve_image_model;
use crate::domain::{ComponentRole, ImageFamily, ModelSamplingDefaults};
use crate::ports::{
    CatalogError, ImageError, ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError, ModelSummary,
};

/// Models by id or name; `broken` fails every question.
#[derive(Debug, Default)]
pub(super) struct Catalog {
    pub(super) specs: Vec<ModelLaunchSpec>,
    pub(super) broken: bool,
}

impl Catalog {
    pub(super) fn of(specs: Vec<ModelLaunchSpec>) -> Self {
        Self {
            specs,
            broken: false,
        }
    }

    fn check(&self) -> Result<(), CatalogError> {
        if self.broken {
            return Err(CatalogError::QueryFailed("the database is locked".into()));
        }
        Ok(())
    }
}

#[async_trait]
impl ModelCatalogPort for Catalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        self.check()?;
        Ok(self
            .specs
            .iter()
            .map(|spec| ModelSummary {
                image_output: spec.image_family.is_some(),
                ..ModelSummary::bare(spec.id, &spec.name)
            })
            .collect())
    }

    async fn resolve_model(&self, _name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(None)
    }

    async fn resolve_for_launch(
        &self,
        name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        self.check()?;
        Ok(self
            .specs
            .iter()
            .find(|s| s.name == name || s.id.to_string() == name)
            .cloned())
    }
}

/// A model with no components linked: complete for SDXL, which needs none,
/// and lacking for the other families.
pub(super) fn spec(id: u32, name: &str, family: Option<ImageFamily>) -> ModelLaunchSpec {
    ModelLaunchSpec {
        model_sampling: ModelSamplingDefaults::default(),
        id,
        name: name.to_owned(),
        file_path: format!("/nonexistent/{name}.gguf").into(),
        projector: None,
        image_family: family,
        components: Vec::new(),
        tags: Vec::new(),
        architecture: None,
        quantization: None,
        context_length: None,
        server_defaults: None,
        file_size_bytes: 0,
        kv_elems_per_token: None,
        kv_memory_is_partial: false,
    }
}

pub(super) const SDXL: u32 = 1;
pub(super) const FLUX: u32 = 2;
pub(super) const CHAT: u32 = 3;
pub(super) const TURBO: u32 = 4;

/// Two complete image models (`sdxl`, `sdxl-turbo`), one that lacks its
/// three files (`flux`), and a chat model (`qwen`).
pub(super) fn library() -> Catalog {
    Catalog::of(vec![
        spec(SDXL, "sdxl", Some(ImageFamily::Sdxl)),
        spec(FLUX, "flux", Some(ImageFamily::Flux1)),
        spec(CHAT, "qwen", None),
        spec(TURBO, "sdxl-turbo", Some(ImageFamily::Sdxl)),
    ])
}

/// One complete image model, one lacking, and a chat model.
pub(super) fn one_complete() -> Catalog {
    Catalog::of(vec![
        spec(SDXL, "sdxl", Some(ImageFamily::Sdxl)),
        spec(FLUX, "flux", Some(ImageFamily::Flux1)),
        spec(CHAT, "qwen", None),
    ])
}

async fn pick(catalog: &Catalog, default: Option<u32>, named: Option<&str>) -> String {
    resolve_image_model(catalog, default.map(i64::from), named)
        .await
        .map(|spec| spec.name)
        .unwrap_or_else(|e| panic!("expected a model, got {e:?}"))
}

async fn refusal(catalog: &Catalog, default: Option<u32>, named: Option<&str>) -> ImageError {
    resolve_image_model(catalog, default.map(i64::from), named)
        .await
        .expect_err("expected a refusal")
}

fn unavailable(error: ImageError) -> String {
    assert_eq!(error.code(), Some("drawing_unavailable"), "{error:?}");
    match error {
        ImageError::Unavailable { reason } => reason,
        other => panic!("expected Unavailable, got {other:?}"),
    }
}

// ── row 1: the named model ───────────────────────────────────────────────

/// A named model draws whatever the default is, by name or by id, and
/// although several complete image models make the unnamed rule refuse.
#[tokio::test]
async fn row_1_a_named_model_draws_before_the_default_is_looked_at() {
    let library = library();
    assert_eq!(pick(&library, Some(TURBO), Some("sdxl")).await, "sdxl");
    assert_eq!(pick(&library, Some(TURBO), Some("1")).await, "sdxl");
    assert_eq!(pick(&library, None, Some("sdxl-turbo")).await, "sdxl-turbo");
}

/// A named model nobody has is `model_not_found`, though a default and a
/// complete image model could have drawn.
#[tokio::test]
async fn row_1_a_named_model_that_is_unknown_is_refused_not_replaced() {
    let refused = refusal(&library(), Some(SDXL), Some("nope")).await;
    assert!(
        matches!(&refused, ImageError::Runtime(ModelRuntimeError::ModelNotFound(name)) if name == "nope"),
        "{refused:?}"
    );
    assert_eq!(refused.http_status(), 404);
}

/// A named chat model is `not_an_image_model`, though a default could have
/// drawn.
#[tokio::test]
async fn row_1_a_named_chat_model_is_refused_not_replaced() {
    let refused = refusal(&library(), Some(SDXL), Some("qwen")).await;
    assert!(
        matches!(&refused, ImageError::NotAnImageModel { model } if model == "qwen"),
        "{refused:?}"
    );
    assert_eq!(refused.code(), Some("not_an_image_model"));
}

/// A named image model that lacks a file is `image_model_incomplete` with
/// every missing role, though the default, and the only complete one, could
/// have drawn.
#[tokio::test]
async fn row_1_a_named_model_lacking_a_file_is_refused_not_replaced() {
    for catalog in [library(), one_complete()] {
        let refused = refusal(&catalog, Some(SDXL), Some("flux")).await;
        match &refused {
            ImageError::Runtime(ModelRuntimeError::ImageModelIncomplete { model, missing }) => {
                assert_eq!(model, "flux");
                assert_eq!(
                    missing,
                    &[
                        ComponentRole::Vae,
                        ComponentRole::ClipL,
                        ComponentRole::T5xxl
                    ]
                );
            }
            other => panic!("expected ImageModelIncomplete, got {other:?}"),
        }
        assert_eq!(refused.http_status(), 400);
        assert!(refused.to_string().contains("--component vae=<path>"));
    }
}

// ── row 2: the settings' default ─────────────────────────────────────────

/// With none named the default draws, though another complete image model
/// stands beside it.
#[tokio::test]
async fn row_2_the_default_draws_when_none_is_named() {
    assert_eq!(pick(&library(), Some(TURBO), None).await, "sdxl-turbo");
    assert_eq!(pick(&library(), Some(SDXL), None).await, "sdxl");
}

/// A default that is gone, or is a chat model now, is passed over for the
/// only complete image model.
#[tokio::test]
async fn row_2_a_default_that_is_gone_or_chats_is_passed_over() {
    assert_eq!(pick(&one_complete(), Some(99), None).await, "sdxl");
    assert_eq!(pick(&one_complete(), Some(CHAT), None).await, "sdxl");
}

/// A default that lacks a file is refused with what it lacks, and the only
/// complete image model does not draw in its place.
#[tokio::test]
async fn row_2_a_default_lacking_a_file_is_refused() {
    let reason = unavailable(refusal(&one_complete(), Some(FLUX), None).await);
    assert!(
        reason.contains("flux") && reason.contains("vae"),
        "{reason}"
    );
}

// ── row 3: the only complete image model ─────────────────────────────────

/// With none named and no default, the only complete image model draws.
#[tokio::test]
async fn row_3_the_only_complete_image_model_draws() {
    assert_eq!(pick(&one_complete(), None, None).await, "sdxl");
}

/// None, one that lacks a file, several that all do, and several complete
/// ones each refuse in their own words.
#[tokio::test]
async fn row_3_none_or_several_is_refused_with_what_would_fix_it() {
    let chat_only = Catalog::of(vec![spec(CHAT, "qwen", None)]);
    let none = unavailable(refusal(&chat_only, None, None).await);
    assert!(none.contains("no image model"), "{none}");

    let flux_only = Catalog::of(vec![spec(FLUX, "flux", Some(ImageFamily::Flux1))]);
    let lacking = unavailable(refusal(&flux_only, None, None).await);
    assert!(lacking.contains("--component vae=<path>"), "{lacking}");

    let all_lacking = Catalog::of(vec![
        spec(FLUX, "flux", Some(ImageFamily::Flux1)),
        spec(5, "qwen-image", Some(ImageFamily::QwenImage21)),
    ]);
    let neither = unavailable(refusal(&all_lacking, None, None).await);
    assert!(neither.contains("flux, qwen-image"), "{neither}");

    let several = unavailable(refusal(&library(), None, None).await);
    assert!(several.contains("sdxl, sdxl-turbo"), "{several}");
    assert!(several.contains("--default-image-model"), "{several}");
}

/// A catalogue that cannot be read is its own failure, never "no image
/// model".
#[tokio::test]
async fn a_catalogue_that_cannot_be_read_is_an_internal_failure() {
    let broken = Catalog {
        broken: true,
        ..library()
    };
    for named in [None, Some("sdxl")] {
        let refused = refusal(&broken, None, named).await;
        assert!(
            matches!(refused, ImageError::Runtime(ModelRuntimeError::Internal(_))),
            "{refused:?}"
        );
    }
}
