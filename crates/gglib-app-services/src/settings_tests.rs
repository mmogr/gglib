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
    assert!(settings.title_generation_prompt.is_none());
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

/// The settings page's install: the nine starter profiles, and a profile the
/// user already has under one of their names left exactly as it was.
#[tokio::test]
async fn installing_the_templates_adds_the_nine_and_keeps_a_profile_of_the_same_name() {
    let core = test_core().await;
    let ops = make_ops(core, MockSystemProbePort::default());
    let mine = UpdateSettingsRequest {
        inference_profiles: Some(Some(vec![profile("chat", 0.123)])),
        ..Default::default()
    };
    ops.update(mine).await.expect("update should succeed");

    let done = ops.install_profile_templates().await.expect("installs");

    assert_eq!(done.kept, ["chat"]);
    assert_eq!(
        done.installed,
        [
            "coding", "creative", "minimal", "low", "medium", "high", "xhigh", "max"
        ]
    );
    let stored = ops.get().await.unwrap().inference_profiles.unwrap();
    assert_eq!(stored, done.settings.inference_profiles.unwrap());
    assert_eq!(stored.len(), 9);
    assert_eq!(stored[0], profile("chat", 0.123), "the stored chat changed");
}

/// The HTTP handlers pass these DTOs through verbatim, so their serde
/// shape *is* the wire contract the frontend codes against. Pin it here
/// rather than discovering a rename in the browser.
#[test]
fn profiles_use_camel_case_on_the_wire() {
    let settings = AppSettings {
        default_context_size: None,
        proxy_port: None,
        llama_base_port: None,
        max_download_queue_size: None,
        max_tool_iterations: None,
        max_stagnation_steps: None,
        max_parallel_tools: None,
        tool_timeout_ms: None,
        default_model_id: None,
        default_image_model_id: None,
        inference_defaults: None,
        inference_profiles: Some(vec![profile("coding", 0.2)]),
        setup_completed: None,
        title_generation_prompt: None,
        proxy_host: None,
        proxy_allowed_hosts: None,
        proxy_cache_on_disk: None,
        proxy_cache_disk_gb: None,
        has_proxy_api_key: false,
        trust_client_sampling: None,
        loop_guard: None,
        tool_call_repair: None,
        mcp_drawing: None,
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

/// The probe's reading is kept when it is plausible and refused when it is
/// suspiciously small; `download_budget` and the setup status read it.
#[test]
fn system_memory_refuses_a_suspiciously_small_reading() {
    let enough = MockSystemProbePort {
        total_ram_bytes: 8 * 1024 * 1024 * 1024,
    };
    assert!(system_memory(&enough).is_some(), "8 GiB is a reading");
    let tiny = MockSystemProbePort {
        total_ram_bytes: 1024,
    };
    assert!(system_memory(&tiny).is_none(), "1 KiB is not");
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
    assert_eq!(
        req.title_generation_prompt, None,
        "omitted key must be None"
    );
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
async fn update_settings_json_null_clears_title_generation_prompt() {
    let core = test_core().await;
    let ops = make_ops(core, MockSystemProbePort::default());

    let set_req: UpdateSettingsRequest =
        serde_json::from_str(r#"{"titleGenerationPrompt": "a prompt"}"#).unwrap();
    let updated = ops.update(set_req).await.expect("update should succeed");
    assert_eq!(updated.title_generation_prompt.as_deref(), Some("a prompt"));

    let clear_req: UpdateSettingsRequest =
        serde_json::from_str(r#"{"titleGenerationPrompt": null}"#).unwrap();
    let cleared = ops.update(clear_req).await.expect("update should succeed");
    assert!(
        cleared.title_generation_prompt.is_none(),
        "explicit JSON null must clear title_generation_prompt"
    );
}

/// The two tool limits Settings › Tools writes are stored and read back
/// under their own names, through the service and the database, and a
/// `null` clears each back to the run's default.
#[tokio::test]
async fn the_tool_limits_round_trip_through_the_settings_api() {
    let core = test_core().await;
    let ops = make_ops(core, MockSystemProbePort::default());

    let set: UpdateSettingsRequest =
        serde_json::from_str(r#"{"maxParallelTools": 4, "toolTimeoutMs": 5000}"#).unwrap();
    let updated = ops.update(set).await.expect("update should succeed");
    assert_eq!(
        (updated.max_parallel_tools, updated.tool_timeout_ms),
        (Some(4), Some(5_000))
    );
    let read = ops.get().await.expect("get should succeed");
    let wire = serde_json::to_value(&read).unwrap();
    assert_eq!(
        (&wire["maxParallelTools"], &wire["toolTimeoutMs"]),
        (&serde_json::json!(4), &serde_json::json!(5000))
    );

    let clear: UpdateSettingsRequest =
        serde_json::from_str(r#"{"maxParallelTools": null, "toolTimeoutMs": null}"#).unwrap();
    let cleared = ops.update(clear).await.expect("update should succeed");
    assert_eq!(
        (cleared.max_parallel_tools, cleared.tool_timeout_ms),
        (None, None)
    );
}

/// A write that names the queue size hands the running download manager the
/// size now stored: the one set, or the default once it is cleared. A write
/// that leaves the field alone tells the manager nothing.
#[tokio::test]
async fn a_queue_size_write_reaches_the_running_download_manager() {
    let core = test_core().await;
    let downloads = Arc::new(MockDownloadManager::new());
    let ops = SettingsOps::new(SettingsDeps {
        core,
        system_probe: Arc::new(MockSystemProbePort::default()),
        downloads: Arc::clone(&downloads) as Arc<dyn DownloadManagerPort>,
    });

    for json in [
        r#"{"maxDownloadQueueSize": 3}"#,
        r#"{"defaultContextSize": 8192}"#,
        r#"{"maxDownloadQueueSize": null}"#,
    ] {
        let request: UpdateSettingsRequest = serde_json::from_str(json).unwrap();
        ops.update(request).await.expect("update should succeed");
    }

    let calls = downloads.calls.lock().unwrap().clone();
    assert_eq!(calls, ["set_max_queue_size 3", "set_max_queue_size 10"]);
}

/// The folder's state is what a write to the disk finds: a folder that is
/// not there will be created where its parent can be written, one there is
/// writable.
#[test]
fn the_folder_state_is_read_from_the_disk() {
    let root = tempfile::tempdir().expect("a temp folder");
    let missing = directory_info_at(&root.path().join("models"), "default", String::new());
    assert_eq!(missing.state, ModelsDirState::Absent);
    assert!(!missing.exists && missing.writable);

    let there = directory_info_at(root.path(), "explicit", String::new());
    assert_eq!(there.state, ModelsDirState::Writable);
    assert!(there.exists && there.writable);
    assert_eq!(there.source, "explicit");
}

/// A folder whose mode refuses a write is not writable, and a missing
/// folder under one will not be created there: both are Not writable, as
/// the first download's write would find.
#[cfg(unix)]
#[test]
fn a_folder_that_refuses_a_write_or_sits_under_one_is_not_writable() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().expect("a temp folder");
    let locked = root.path().join("locked");
    std::fs::create_dir(&locked).expect("a folder to lock");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).expect("0555");
    let folder = directory_info_at(&locked, "explicit", String::new());
    let under = directory_info_at(&locked.join("models"), "explicit", String::new());
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755))
        .expect("writable again, to clean up");
    assert_eq!(folder.state, ModelsDirState::NotWritable);
    assert!(folder.exists && !folder.writable);
    assert_eq!(under.state, ModelsDirState::NotWritable);
    assert!(!under.exists && !under.writable);
}
