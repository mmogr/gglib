//! A [`ModelCatalogPort`] over a fixed set of names, for tests.

use std::collections::HashSet;

use async_trait::async_trait;

use super::{CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelSummary};

/// A catalogue holding a fixed set of names, resolving by exact match: the
/// behaviour of the `SQLite` repository (`WHERE name = ?`).
///
/// Every model it resolves is [`ModelSummary::bare`] under id 1. It lists
/// nothing and has nothing to launch.
#[derive(Debug)]
pub struct NamedCatalog {
    names: HashSet<String>,
}

impl NamedCatalog {
    /// A catalogue holding `names`.
    #[must_use]
    pub fn new(names: &[&str]) -> Self {
        Self {
            names: names.iter().map(|n| (*n).to_owned()).collect(),
        }
    }
}

#[async_trait]
impl ModelCatalogPort for NamedCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(Vec::new())
    }

    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(self
            .names
            .contains(name)
            .then(|| ModelSummary::bare(1, name)))
    }

    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}
