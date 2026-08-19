use super::*;

fn model() -> Model {
    Model {
        dialect_spec: None,
        id: 1,
        name: "test-model".to_string(),
        model_key: String::new(),
        file_path: std::path::PathBuf::from("/tmp/model.gguf"),
        param_count_b: 7.0,
        architecture: None,
        quantization: None,
        context_length: Some(131_072),
        expert_count: None,
        expert_used_count: None,
        expert_shared_count: None,
        metadata: std::collections::HashMap::new(),
        added_at: chrono::Utc::now(),
        hf_repo_id: None,
        hf_commit_sha: None,
        hf_filename: None,
        download_date: None,
        last_update_check: None,
        tags: vec![],
        inference_defaults: None,
        defaults_origin: None,
        server_defaults: None,
        capabilities: gglib_core::domain::capabilities::ModelCapabilities::default(),
        template_caps: None,
        benchmark_summary: None,
    }
}

fn request() -> StartServerRequest {
    StartServerRequest::default()
}

/// The explicit tier outranks the model's server defaults, which outrank
/// the settings default — the same precedence `gglib serve` documents.
#[test]
fn context_resolves_explicit_over_model_over_settings() {
    let mut m = model();
    m.server_defaults = Some(gglib_core::domain::ServerConfig {
        context_length: Some(16_384),
    });
    let settings = Settings {
        default_context_size: Some(8192),
        ..Default::default()
    };

    let explicit = plan_pinned_launch(
        &m,
        &settings,
        &StartServerRequest {
            context_length: Some(4096),
            ..request()
        },
        ProxyGlobals::default(),
    );
    assert_eq!(explicit.effective_ctx, 4096);

    let model_tier = plan_pinned_launch(&m, &settings, &request(), ProxyGlobals::default());
    assert_eq!(model_tier.effective_ctx, 16_384);

    m.server_defaults = None;
    let settings_tier = plan_pinned_launch(&m, &settings, &request(), ProxyGlobals::default());
    assert_eq!(settings_tier.effective_ctx, 8192);
}

/// `mlock: false` is the flag's absence, not a request to disable — the
/// invariant the CLI's `--mlock` handling depends on.
#[test]
fn absent_mlock_expresses_no_opinion() {
    let plan = plan_pinned_launch(
        &model(),
        &Settings::default(),
        &request(),
        ProxyGlobals::default(),
    );
    assert_eq!(plan.pinned.launch_overrides.mlock, None);
}

/// Resolved sampling must land on the pin's launch options — sampling
/// rides the pinned model, never a proxy-wide override.
#[test]
fn resolved_sampling_reaches_the_pin() {
    let plan = plan_pinned_launch(
        &model(),
        &Settings::default(),
        &StartServerRequest {
            inference_params: Some(InferenceConfig {
                temperature: Some(0.4),
                ..Default::default()
            }),
            ..request()
        },
        ProxyGlobals::default(),
    );
    let params = plan
        .pinned
        .launch_overrides
        .inference_params
        .as_ref()
        .unwrap();
    assert_eq!(params.temperature, Some(0.4));
}

/// The cache master switch outranks the directory: cache off means no
/// slot path reaches llama-server, byte-for-byte.
#[test]
fn cache_master_switch_gates_the_slot_dir() {
    let off = plan_pinned_launch(
        &model(),
        &Settings::default(),
        &request(),
        ProxyGlobals {
            cache_enabled: false,
            slot_dir: Some(PathBuf::from("/custom/slots")),
            ..Default::default()
        },
    );
    assert_eq!(off.pinned.launch_overrides.slot_save_path, None);

    let on = plan_pinned_launch(
        &model(),
        &Settings::default(),
        &request(),
        ProxyGlobals {
            cache_enabled: true,
            slot_dir: Some(PathBuf::from("/custom/slots")),
            ..Default::default()
        },
    );
    assert_eq!(
        on.pinned.launch_overrides.slot_save_path,
        Some(PathBuf::from("/custom/slots"))
    );
}

/// The request's llama port must reach the pin's launch options — the
/// ServeModal's Port field on the pinned path (review-gate blocker).
#[test]
fn request_port_reaches_the_pin() {
    let plan = plan_pinned_launch(
        &model(),
        &Settings::default(),
        &StartServerRequest {
            port: Some(9345),
            ..request()
        },
        ProxyGlobals::default(),
    );
    assert_eq!(plan.pinned.launch_overrides.port, Some(9345));
}

/// A caller-supplied fallback context overrides the settings default on
/// the third rung without touching the explicit tier.
#[test]
fn caller_default_ctx_overrides_the_settings_rung() {
    let settings = Settings {
        default_context_size: Some(8192),
        ..Default::default()
    };
    let plan = plan_pinned_launch(
        &model(),
        &settings,
        &request(),
        ProxyGlobals {
            default_ctx: Some(16_384),
            ..Default::default()
        },
    );
    assert_eq!(plan.effective_ctx, 16_384);
}

/// Unset proxy globals keep the hardened defaults — loopback bind above all.
#[test]
fn default_globals_bind_loopback() {
    let plan = plan_pinned_launch(
        &model(),
        &Settings::default(),
        &request(),
        ProxyGlobals::default(),
    );
    assert_eq!(plan.unified.to_proxy_config().host, "127.0.0.1");
}
