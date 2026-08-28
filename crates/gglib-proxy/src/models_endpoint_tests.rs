//! What `/v1/models` advertises when nothing is configured.
//!
//! The interesting cases are all about the *unconfigured* chain, which is the
//! ordinary state since #926 stopped `Settings::with_defaults` writing a floor.

use crate::models::ModelsResponse;
use gglib_core::domain::ModelCapabilities;
use gglib_core::ports::ModelSummary;

fn summary(trained: Option<u64>) -> ModelSummary {
    ModelSummary {
        dialect: None,
        template_caps: None,
        id: 1,
        name: "qwen3-8b".into(),
        tags: Vec::new(),
        capabilities: ModelCapabilities::empty(),
        param_count: "8B".into(),
        quantization: None,
        architecture: None,
        created_at: 0,
        file_size: 0,
        context_length: trained,
        inference_defaults: None,
        defaults_origin: None,
        server_defaults: None,
    }
}

/// The regression this module exists to close.
///
/// Three separately-reviewed changes composed into it. #926 left
/// `default_context_size` unset, so a fresh consumer configures nothing;
/// `total_device_memory_bytes` returned `None` on every AMD, Intel, Vulkan and
/// CPU-only host — #946 later narrowed that to CPU-only and unreadable devices
/// — so `fit_context` refuses and the chain lands on the floor;
/// and #925 then read that floor as "no cap, advertise the trained window".
///
/// The result on a Ryzen laptop with an AMD iGPU: `context_window` 131072
/// advertised, `-c 4096` served. A Copilot BYOK picker reads this endpoint once
/// and budgets against it for the whole session, so it then sends a request the
/// server cannot hold. Before the arc that host advertised 4096, which was
/// honest.
#[test]
fn a_host_that_cannot_fit_advertises_the_floor_it_will_serve() {
    let resp = ModelsResponse::from_summaries(vec![summary(Some(131_072))], None, false);
    assert_eq!(
        resp.data[0].context_window,
        Some(gglib_core::settings::DEFAULT_CONTEXT_SIZE),
        "an unfittable host must advertise what a launch actually gets"
    );
}

/// The control, and the behaviour #925 was right to introduce. Where the fit is
/// reachable the launch really will exceed the floor, so advertising 4096 would
/// understate it — and the trained window is a true upper bound, because
/// `fit_context` caps at it before snapping to a rung.
#[test]
fn a_host_that_can_fit_still_advertises_the_trained_window() {
    let resp = ModelsResponse::from_summaries(vec![summary(Some(131_072))], None, true);
    assert_eq!(resp.data[0].context_window, Some(131_072));
}

/// A number someone chose outranks both. The flag only decides what happens
/// when the chain reaches the floor, so it must not disturb the rungs above it.
#[test]
fn a_configured_default_is_unaffected_by_the_flag() {
    for fit in [true, false] {
        let resp = ModelsResponse::from_summaries(vec![summary(Some(131_072))], Some(8192), fit);
        assert_eq!(
            resp.data[0].context_window,
            Some(8192),
            "fit_available = {fit} must not move a configured cap"
        );
    }
}

/// An unknown trained window stays unknown. There is nothing to cap, and
/// inventing the floor here would advertise a ceiling the file never claimed.
#[test]
fn an_unknown_trained_window_is_still_unknown() {
    for fit in [true, false] {
        let resp = ModelsResponse::from_summaries(vec![summary(None)], None, fit);
        assert_eq!(resp.data[0].context_window, None, "fit_available = {fit}");
    }
}
