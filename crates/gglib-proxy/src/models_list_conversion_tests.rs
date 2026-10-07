use super::*;
use gglib_core::settings::DEFAULT_CONTEXT_SIZE;

// =========================================================================
// ModelInfo conversion tests
// =========================================================================

#[test]
fn model_info_description_includes_arch_and_quant() {
    let summary = ModelSummary {
        param_count: "13B".into(),
        quantization: Some("Q5_K_S".into()),
        architecture: Some("llama".into()),
        ..ModelSummary::bare(1, "test")
    };
    let resp = ModelsResponse::from_summaries(vec![summary], Some(DEFAULT_CONTEXT_SIZE), true);
    let info = &resp.data[0];
    let desc = info.description.as_ref().unwrap();
    assert!(desc.contains("llama"), "description should include arch");
    assert!(desc.contains("13B"), "description should include params");
    assert!(desc.contains("Q5_K_S"), "description should include quant");
}

#[test]
fn model_info_handles_missing_arch_and_quant() {
    let summary = ModelSummary {
        param_count: "1B".into(),
        ..ModelSummary::bare(1, "bare-model")
    };
    let resp = ModelsResponse::from_summaries(vec![summary], Some(DEFAULT_CONTEXT_SIZE), true);
    let info = &resp.data[0];
    let desc = info.description.as_ref().unwrap();
    assert!(
        desc.contains("unknown"),
        "missing fields should show 'unknown'"
    );
}

#[test]
fn model_info_maps_context_length_to_context_window() {
    let summary = ModelSummary {
        context_length: Some(32_768),
        ..ModelSummary::bare(1, "ctx-model")
    };
    let resp = ModelsResponse::from_summaries(vec![summary], Some(DEFAULT_CONTEXT_SIZE), true);
    // With no server_defaults, resolve_context_size returns global default (4096).
    // min(32768, 4096) = 4096.
    assert_eq!(resp.data[0].context_window, Some(4096));
}

#[test]
fn model_info_context_window_none_when_unknown() {
    let summary = ModelSummary::bare(1, "unknown-ctx-model");
    let resp = ModelsResponse::from_summaries(vec![summary], Some(DEFAULT_CONTEXT_SIZE), true);
    assert_eq!(resp.data[0].context_window, None);
}

#[test]
fn models_response_respects_server_defaults_context_length() {
    use gglib_core::domain::ServerConfig;

    let summary = ModelSummary {
        context_length: Some(32_768), // GGUF ceiling is large
        server_defaults: Some(ServerConfig {
            context_length: Some(8192),
        }),
        ..ModelSummary::bare(1, "server-ctx-model")
    };
    // Global default is 4096, but server_defaults (8192) wins.
    // min(32768, 8192) = 8192.
    let resp = ModelsResponse::from_summaries(vec![summary], Some(4096), true);
    assert_eq!(resp.data[0].context_window, Some(8192));
}

#[test]
fn models_response_falls_through_when_server_defaults_context_length_none() {
    use gglib_core::domain::ServerConfig;

    let summary = ModelSummary {
        context_length: Some(32_768),
        server_defaults: Some(ServerConfig {
            context_length: None, // exists but context_length is None
        }),
        ..ModelSummary::bare(1, "fallback-ctx-model")
    };
    // Falls through to global default (4096).
    // min(32768, 4096) = 4096.
    let resp = ModelsResponse::from_summaries(vec![summary], Some(4096), true);
    assert_eq!(resp.data[0].context_window, Some(4096));
}

/// With nothing configured, the advertised window is the model's own trained
/// ceiling — not a floor nobody chose.
///
/// The launch is sized by the daemon in that case, so advertising 4096 would
/// understate what the model is about to be served at. Every other test here
/// passes `Some(..)`, so this is the only cover for the branch.
#[test]
fn an_unconfigured_model_advertises_its_trained_window() {
    let resp = ModelsResponse::from_summaries(
        vec![ModelSummary {
            tags: vec!["chat".into()],
            param_count: "8B".into(),
            quantization: Some("Q4_K_M".into()),
            architecture: Some("qwen".into()),
            created_at: 1_700_000_000,
            file_size: 4_000_000_000,
            context_length: Some(131_072),
            ..ModelSummary::bare(1, "qwen")
        }],
        None,
        true,
    );
    assert_eq!(resp.data[0].context_window, Some(131_072));
}

/// A configured global default is still a cap.
#[test]
fn a_configured_default_still_caps_the_advertised_window() {
    let resp = ModelsResponse::from_summaries(
        vec![ModelSummary {
            tags: vec!["chat".into()],
            param_count: "8B".into(),
            quantization: Some("Q4_K_M".into()),
            architecture: Some("qwen".into()),
            created_at: 1_700_000_000,
            file_size: 4_000_000_000,
            context_length: Some(131_072),
            ..ModelSummary::bare(1, "qwen")
        }],
        Some(8192),
        true,
    );
    assert_eq!(resp.data[0].context_window, Some(8192));
}
