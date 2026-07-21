//! The single catalog round-trip that produces a [`ModelContext`].

use tracing::{debug, warn};

use super::ModelContext;
use crate::ports::ModelCatalogPort;

/// Resolve the [`ModelContext`] for a model in one catalog round-trip.
///
/// `model` is `None` when the caller has no model to name — an agent session
/// against an already-running server, say. That yields a passthrough context
/// without touching the catalog, which is why every caller can hand its
/// `Option` straight in rather than open-coding the empty case.
///
/// Any failure to resolve returns [`ModelContext::passthrough`]: an unknown or
/// unreachable model costs the request its model-specific handling, never the
/// request itself. The two failure modes are logged differently on purpose —
/// an unknown model is routine (clients name models the catalog has never
/// heard of), while a catalog error means something is actually broken.
pub async fn resolve(catalog: &dyn ModelCatalogPort, model: Option<&str>) -> ModelContext {
    let Some(model_name) = model else {
        return ModelContext::passthrough();
    };

    match catalog.resolve_model(model_name).await {
        Ok(Some(summary)) => ModelContext::from(&summary),
        Ok(None) => {
            debug!(model = %model_name, "model not found in catalog; using pass-through context");
            ModelContext::passthrough()
        }
        Err(e) => {
            warn!(model = %model_name, error = %e, "failed to resolve model context; using pass-through context");
            ModelContext::passthrough()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::summary;
    use super::*;
    use crate::domain::ModelCapabilities;
    use crate::ports::{CatalogError, ModelLaunchSpec, ModelSummary};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Counts lookups so tests can assert the catalog was never consulted.
    #[derive(Debug, Default)]
    struct SpyCatalog {
        /// `None` → the model is unknown; `Some` → this summary is returned.
        found: Option<ModelSummary>,
        /// When set, every lookup fails instead.
        fails: bool,
        lookups: AtomicUsize,
    }

    #[async_trait]
    impl ModelCatalogPort for SpyCatalog {
        async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
            unimplemented!("not exercised by these tests")
        }

        async fn resolve_model(&self, _name: &str) -> Result<Option<ModelSummary>, CatalogError> {
            self.lookups.fetch_add(1, Ordering::SeqCst);
            if self.fails {
                return Err(CatalogError::QueryFailed("catalog is down".into()));
            }
            Ok(self.found.clone())
        }

        async fn resolve_for_launch(
            &self,
            _name: &str,
        ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
            unimplemented!("not exercised by these tests")
        }
    }

    #[tokio::test]
    async fn found_model_yields_all_three_fields() {
        let catalog = SpyCatalog {
            found: Some(ModelSummary {
                tags: vec!["format:qwen".to_string()],
                capabilities: ModelCapabilities::REQUIRES_STRICT_TURNS,
                ..summary()
            }),
            ..Default::default()
        };

        let ctx = resolve(&catalog, Some("qwen3")).await;
        assert_eq!(ctx.tags, vec!["format:qwen".to_string()]);
        assert_eq!(ctx.capabilities, ModelCapabilities::REQUIRES_STRICT_TURNS);
    }

    #[tokio::test]
    async fn unknown_model_yields_passthrough() {
        let catalog = SpyCatalog::default();
        assert_eq!(
            resolve(&catalog, Some("ghost")).await,
            ModelContext::passthrough()
        );
    }

    /// A broken catalog must degrade the request, not fail it.
    #[tokio::test]
    async fn catalog_error_yields_passthrough() {
        let catalog = SpyCatalog {
            fails: true,
            ..Default::default()
        };
        assert_eq!(
            resolve(&catalog, Some("qwen3")).await,
            ModelContext::passthrough()
        );
    }

    #[tokio::test]
    async fn no_model_name_skips_the_catalog_entirely() {
        let catalog = SpyCatalog {
            found: Some(summary()),
            ..Default::default()
        };
        assert_eq!(resolve(&catalog, None).await, ModelContext::passthrough());
        assert_eq!(catalog.lookups.load(Ordering::SeqCst), 0);
    }
}
