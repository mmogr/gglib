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
