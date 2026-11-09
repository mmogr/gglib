//! A resident that cannot serve a request is recycled: one launched at
//! another context, or one that fails its health check, is stopped and its
//! slot emptied, and the request's count goes back.

use super::*;
use crate::process::RuntimeBinaries;
use async_trait::async_trait;
use gglib_core::domain::{CacheRamHealth, ModelSamplingDefaults};
use gglib_core::ports::ModelSummary;
use tokio::time::Instant;

#[derive(Debug)]
struct StubCatalog;

#[async_trait]
impl ModelCatalogPort for StubCatalog {
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

/// A port nothing listens on, so a health check there fails.
fn dead_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// A set whose primary holds `qwen` (model 1) at 4096 tokens on `port`.
pub(super) fn set_with_resident(port: u16) -> ResidentSet {
    let set = ResidentSet::unobserved(
        Arc::new(StubCatalog),
        ServerConfigOptions::default(),
        CacheRamSetting::Auto,
    );
    drop(set.queue().install(PRIMARY_SLOT, resident(port)));
    set
}

/// `qwen` (model 1) at 4096 tokens on `port`, launched with no projector.
pub(super) fn resident(port: u16) -> Resident {
    Resident {
        model_sampling: ModelSamplingDefaults::default(),
        model_id: 1,
        model_name: "qwen".to_owned(),
        context_size: 4096,
        port,
        projector: None,
        runtime: gglib_core::domain::RuntimeKind::Llama,
        components: Vec::new(),
        slot_restore_supported: true,
        cache_ram_health: CacheRamHealth::LlamaDefault,
        narration: None,
        inflight: 0,
        resident_since: Instant::now(),
        weights_bytes: 1024,
    }
}

pub(super) fn core() -> Arc<RwLock<GuiProcessCore>> {
    Arc::new(RwLock::new(GuiProcessCore::new(
        19_300,
        RuntimeBinaries::llama_only("/nonexistent/llama-server"),
    )))
}

/// What a `Serve` leaves for `serve` to claim: a request counted against
/// `qwen`'s slot, as `poll` counts it.
pub(super) fn granted(set: &ResidentSet) {
    let counted = set.queue().lease(PRIMARY_SLOT).expect("qwen is resident");
    counted.disarm();
}

#[tokio::test]
async fn a_request_at_another_context_recycles_the_model() {
    let set = set_with_resident(dead_port());

    granted(&set);
    let recycled = set
        .serve(PRIMARY_SLOT, (1, 8192, None), &core())
        .await
        .unwrap();

    assert!(recycled.is_none(), "the next pass launches it again");
    assert!(set.queue().slot(PRIMARY_SLOT).is_none(), "recycled");
}

#[tokio::test]
async fn a_failed_health_check_recycles_the_model() {
    let set = set_with_resident(dead_port());

    granted(&set);
    let recycled = set
        .serve(PRIMARY_SLOT, (1, 4096, None), &core())
        .await
        .unwrap();

    assert!(recycled.is_none(), "the next pass launches it again");
    assert!(set.queue().slot(PRIMARY_SLOT).is_none(), "recycled");
}
