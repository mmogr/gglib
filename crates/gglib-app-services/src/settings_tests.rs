//! Tests for [`super::SettingsOps`].
//!
//! A `#[path]` sibling rather than an inline `mod tests`, following the split
//! #860 made across the workspace: the tests outweigh the code they cover and
//! every line of them counted against the module's size ratchet, which this
//! module is at.
//!
//! Declared as `mod tests`, not `mod settings_tests`, so every test keeps the
//! path it already had — `settings::tests::…` — and the inventory before and
//! after the move compares name for name.

use std::sync::Arc;

use super::*;
use crate::test_support::{MockDownloadManager, MockSystemProbePort, test_core};

fn make_ops(core: Arc<AppCore>, probe: MockSystemProbePort) -> SettingsOps {
    SettingsOps::new(SettingsDeps {
        core,
        system_probe: Arc::new(probe),
        downloads: Arc::new(MockDownloadManager::new()),
    })
}

#[tokio::test]
async fn get_returns_default_settings() {
    let core = test_core().await;
    let ops = make_ops(core, MockSystemProbePort::default());
    let settings = ops.get().await.expect("get should succeed");
    // Fresh DB: no custom settings – all optional fields are None
    assert!(settings.default_download_path.is_none());
}

fn profile(name: &str, temperature: f32) -> gglib_core::domain::InferenceProfile {
    gglib_core::domain::InferenceProfile {
        name: name.to_owned(),
        description: None,
        config: gglib_core::domain::InferenceConfig {
            temperature: Some(temperature),
            ..Default::default()
        },
        list_in_models: true,
    }
}

/// Profiles must survive the full API round trip: request -> core -> store
/// -> read back. They are only useful to the proxy once persisted.
#[tokio::test]
async fn profiles_round_trip_through_the_settings_api() {
    let core = test_core().await;
    let ops = make_ops(core, MockSystemProbePort::default());

    let updated = ops
        .update(UpdateSettingsRequest {
            inference_profiles: Some(Some(vec![profile("coding", 0.2)])),
            ..Default::default()
        })
        .await
        .expect("update should succeed");
    assert_eq!(updated.inference_profiles.as_deref().unwrap().len(), 1);

    let read_back = ops.get().await.expect("get should succeed");
    let stored = read_back.inference_profiles.expect("profiles persisted");
    assert_eq!(stored[0].name, "coding");
    assert_eq!(stored[0].config.temperature, Some(0.2));
}

/// An omitted key must leave stored profiles alone, so a client updating
/// an unrelated setting cannot drop profiles it never knew about.
#[tokio::test]
async fn an_unrelated_update_leaves_profiles_untouched() {
    let core = test_core().await;
    let ops = make_ops(core, MockSystemProbePort::default());

    ops.update(UpdateSettingsRequest {
        inference_profiles: Some(Some(vec![profile("coding", 0.2)])),
        ..Default::default()
    })
    .await
    .expect("seed should succeed");

    ops.update(UpdateSettingsRequest {
        default_context_size: Some(Some(8192)),
        ..Default::default()
    })
    .await
    .expect("unrelated update should succeed");

    let read_back = ops.get().await.expect("get should succeed");
    assert_eq!(
        read_back.inference_profiles.as_deref().unwrap().len(),
        1,
        "profiles must survive an unrelated update"
    );
}

/// Validation is not bypassed by coming in over the API.
#[tokio::test]
async fn an_invalid_profile_is_rejected_by_the_api() {
    let core = test_core().await;
    let ops = make_ops(core, MockSystemProbePort::default());

    let result = ops
        .update(UpdateSettingsRequest {
            // Uppercase is not a valid slug.
            inference_profiles: Some(Some(vec![profile("Coding", 0.2)])),
            ..Default::default()
        })
        .await;
    assert!(result.is_err(), "expected rejection, got {result:?}");

    let read_back = ops.get().await.expect("get should succeed");
    assert!(
        read_back.inference_profiles.is_none(),
        "a rejected update must not persist anything"
    );
}

/// The HTTP handlers pass these DTOs through verbatim, so their serde
/// shape *is* the wire contract the frontend codes against. Pin it here
/// rather than discovering a rename in the browser.
#[test]
fn profiles_use_camel_case_on_the_wire() {
    let settings = AppSettings {
        default_download_path: None,
        default_context_size: None,
        proxy_port: None,
        llama_base_port: None,
        max_download_queue_size: None,
        show_memory_fit_indicators: None,
        max_tool_iterations: None,
        max_stagnation_steps: None,
        default_model_id: None,
        inference_defaults: None,
        inference_profiles: Some(vec![profile("coding", 0.2)]),
        setup_completed: None,
        title_generation_prompt: None,
        bind_host: None,
        share_lan: None,
        proxy_api_key: None,
        trust_client_sampling: None,
        proxy_loop_detection: None,
        tool_call_repair: None,
        agentic_sampling: Some(false),
        proxy_autostart: None,
        close_to_tray: None,
        start_at_login: None,
    };

    let json = serde_json::to_value(&settings).expect("serializes");
    let entry = &json["inferenceProfiles"][0];
    assert_eq!(entry["name"], "coding");
    assert_eq!(entry["listInModels"], true);
    // Value equality is not the point here — the key name is. `f32` widens
    // to `f64` in JSON, so an exact float compare tests the widening, not
    // the contract.
    assert!(entry["config"]["temperature"].is_number());
    // Wire visibility is the point of adding agentic_sampling to the
    // response DTO — a toggle that saves but cannot read back resets.
    assert_eq!(json["agenticSampling"], false);

    // And the update request accepts the same shape back.
    let request: UpdateSettingsRequest = serde_json::from_value(serde_json::json!({
        "inferenceProfiles": [{
            "name": "chat",
            "description": null,
            "config": {"temperature": 0.7},
            "listInModels": false
        }]
    }))
    .expect("deserializes");
    let parsed = request.inference_profiles.flatten().expect("present");
    assert_eq!(parsed[0].name, "chat");
    assert!(!parsed[0].list_in_models);
}

#[tokio::test]
async fn get_models_directory_info_returns_valid_info() {
    let core = test_core().await;
    let ops = make_ops(core, MockSystemProbePort::default());
    // This calls resolve_models_dir which is pure – should not panic
    let result = ops.get_models_directory_info();
    assert!(result.is_ok(), "expected Ok, got {result:?}");
}

#[tokio::test]
async fn get_system_memory_returns_some_when_probe_reports_enough_ram() {
    let core = test_core().await;
    let probe = MockSystemProbePort {
        total_ram_bytes: 8 * 1024 * 1024 * 1024, // 8 GiB
    };
    let ops = make_ops(core, probe);
    let result = ops.get_system_memory().expect("should not error");
    assert!(result.is_some(), "expected Some for 8 GiB");
}

#[tokio::test]
async fn get_system_memory_returns_none_when_probe_reports_tiny_ram() {
    let core = test_core().await;
    let probe = MockSystemProbePort {
        total_ram_bytes: 1024, // 1 KiB – suspiciously small
    };
    let ops = make_ops(core, probe);
    let result = ops.get_system_memory().expect("should not error");
    assert!(result.is_none(), "expected None for suspiciously small RAM");
}

/// JSON-boundary tests for `UpdateSettingsRequest`'s double-`Option`
/// fields, mirroring the coverage added for
/// `UpdateModelRequest.server_defaults`. Deserializes raw JSON (rather
/// than constructing the struct in Rust) to prove
/// `serde_with::rust::double_option` distinguishes an omitted key from
/// an explicit `null` at the layer that actually matters.
#[test]
fn update_settings_request_omitted_field_is_none() {
    let req: UpdateSettingsRequest = serde_json::from_str("{}").unwrap();
    assert_eq!(req.default_context_size, None, "omitted key must be None");
    assert_eq!(req.default_download_path, None, "omitted key must be None");
}

#[test]
fn update_settings_request_explicit_null_is_some_none() {
    let req: UpdateSettingsRequest =
        serde_json::from_str(r#"{"defaultContextSize": null}"#).unwrap();
    assert_eq!(
        req.default_context_size,
        Some(None),
        "explicit null must clear the setting (Some(None))"
    );
}

#[test]
fn update_settings_request_populated_value_is_some_some() {
    let req: UpdateSettingsRequest =
        serde_json::from_str(r#"{"defaultContextSize": 8192}"#).unwrap();
    assert_eq!(req.default_context_size, Some(Some(8192)));
}

/// End-to-end: drive `SettingsOps::update` with a real
/// `serde_json::from_str` payload proving an explicit JSON `null`
/// actually clears the setting through the full service+DB round trip,
/// not just at deserialization.
#[tokio::test]
async fn update_settings_json_null_clears_default_download_path() {
    let core = test_core().await;
    let ops = make_ops(core, MockSystemProbePort::default());

    let set_req: UpdateSettingsRequest =
        serde_json::from_str(r#"{"defaultDownloadPath": "/custom/path"}"#).unwrap();
    let updated = ops.update(set_req).await.expect("update should succeed");
    assert_eq!(
        updated.default_download_path.as_deref(),
        Some("/custom/path")
    );

    let clear_req: UpdateSettingsRequest =
        serde_json::from_str(r#"{"defaultDownloadPath": null}"#).unwrap();
    let cleared = ops.update(clear_req).await.expect("update should succeed");
    assert!(
        cleared.default_download_path.is_none(),
        "explicit JSON null must clear default_download_path"
    );
}
