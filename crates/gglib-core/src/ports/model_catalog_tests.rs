//! Tests for the catalog port's defaulted methods.

use super::*;

/// A read-only implementor that never overrides `record_template_caps` —
/// the shape of every test double this port has across the workspace.
#[derive(Debug)]
struct ReadOnlyCatalog;

#[async_trait]
impl ModelCatalogPort for ReadOnlyCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(Vec::new())
    }
    async fn resolve_model(&self, _name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(None)
    }
    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

/// The default body is a successful no-op, so read-only implementors need
/// not implement persistence they do not have — and a caps observation
/// against one is dropped, never an error that could fail a launch.
#[tokio::test]
async fn record_template_caps_defaults_to_a_successful_no_op() {
    let result = ReadOnlyCatalog
        .record_template_caps(1, TemplateCaps::default())
        .await;
    assert!(result.is_ok());
}

/// A launch spec names the roles its family needs and has no file for, as
/// the catalogue's model does; a model that chats needs none.
#[test]
fn a_launch_spec_names_its_missing_components() {
    use crate::domain::{ComponentRole, ImageFamily, ModelComponent};
    let mut spec = ModelLaunchSpec {
        model_sampling: ModelSamplingDefaults::default(),
        id: 1,
        name: "flux".to_owned(),
        file_path: "/m/flux.gguf".into(),
        projector: None,
        image_family: Some(ImageFamily::Flux1),
        components: vec![ModelComponent {
            role: ComponentRole::ClipL,
            path: "/m/clip_l.safetensors".into(),
        }],
        tags: Vec::new(),
        architecture: None,
        quantization: None,
        context_length: None,
        server_defaults: None,
        file_size_bytes: 0,
        kv_elems_per_token: None,
        kv_memory_is_partial: false,
    };
    assert_eq!(
        spec.missing_components(),
        [ComponentRole::Vae, ComponentRole::T5xxl]
    );
    spec.image_family = None;
    assert!(spec.missing_components().is_empty());
}
