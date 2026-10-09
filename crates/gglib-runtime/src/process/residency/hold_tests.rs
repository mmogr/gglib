//! A resident a run holds is not recycled: the request that would have
//! recycled it is refused, retryably, and the model stays.

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

const NEVER_FITS: SecondarySlotDecision = SecondarySlotDecision::RefuseTooLarge {
    footprint_bytes: 9 * 1024 * 1024 * 1024,
    ceiling_bytes: 2 * 1024 * 1024 * 1024,
};

/// A port nothing listens on, so a health check there fails.
fn dead_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// A set whose primary holds `qwen` (model 1) at 4096 tokens on `port`.
pub(super) fn set_with_resident(port: u16) -> ResidentSet {
    let set = ResidentSet::new(
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

/// What `wait_for_slot` does before `serve`: a request for `qwen` granted.
pub(super) fn granted(set: &ResidentSet) {
    let ticket = set.queue().enqueue("qwen");
    let decision = set.queue().poll(&ticket, NEVER_FITS);
    assert_eq!(decision, AdmissionDecision::Serve { slot: PRIMARY_SLOT });
}

fn inflight(set: &ResidentSet) -> u32 {
    set.queue().slot(PRIMARY_SLOT).map_or(0, |r| r.inflight)
}

#[tokio::test]
async fn a_request_at_another_context_is_refused_while_the_model_is_held() {
    let port = dead_port();
    let set = set_with_resident(port);
    let hold = set.queue().hold(port, 1).unwrap();

    granted(&set);
    let refused = set
        .serve(PRIMARY_SLOT, (8192, None), &core())
        .await
        .unwrap_err();

    assert!(matches!(refused, ModelRuntimeError::AdmissionTimeout(_)));
    assert_eq!(refused.suggested_status_code(), 503);
    assert!(set.queue().slot(PRIMARY_SLOT).is_some(), "the model stays");
    assert_eq!(inflight(&set), 0, "the refused request's count is released");

    drop(hold);
    granted(&set);
    let recycled = set
        .serve(PRIMARY_SLOT, (8192, None), &core())
        .await
        .unwrap();
    assert!(recycled.is_none());
    assert!(
        set.queue().slot(PRIMARY_SLOT).is_none(),
        "recycled once free"
    );
}

#[tokio::test]
async fn a_failed_health_check_does_not_recycle_a_held_model() {
    let port = dead_port();
    let set = set_with_resident(port);
    let _hold = set.queue().hold(port, 1).unwrap();

    granted(&set);
    let refused = set
        .serve(PRIMARY_SLOT, (4096, None), &core())
        .await
        .unwrap_err();

    assert!(refused.is_retryable());
    assert!(set.queue().slot(PRIMARY_SLOT).is_some(), "the model stays");
}
