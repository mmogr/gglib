//! Tests for [`super::translate`].
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

use super::*;
use crate::unified_server_config::{GlobalDefaults, UnifiedServerConfig};
use gglib_core::domain::InferenceConfig;

const BASE_PORT: u16 = 9000;

fn model_path() -> PathBuf {
    PathBuf::from("/models/test.gguf")
}

/// Flatten `explicit`/`globals` through the cascade, then translate — the
/// same two calls every real launch surface makes: `resolved_options()`
/// followed by `build_server_config` at spawn (see `ResidentSet`).
fn build_via_cascade(
    tags: &[String],
    explicit: ServerConfigOptions,
    globals: GlobalDefaults,
) -> ServerConfig {
    let base_port = globals.llama_base_port;
    let opts = UnifiedServerConfig { explicit, globals }.resolved_options();
    build_server_config(
        7,
        "cascade-model".to_string(),
        model_path(),
        base_port,
        tags,
        opts,
    )
}

#[test]
fn cascade_reaches_the_built_config_with_default_options() {
    let config = build_via_cascade(
        &[],
        ServerConfigOptions::default(),
        GlobalDefaults {
            llama_base_port: BASE_PORT,
            ..Default::default()
        },
    );
    assert_eq!(config.base_port, BASE_PORT);
    assert!(!config.mlock);
    assert_eq!(config.slot_save_path, None);
}

#[test]
fn cascade_reaches_the_built_config_with_fully_specified_options() {
    let opts = ServerConfigOptions {
        context_size: Some(32_768),
        model_server_ctx: Some(16_384),
        global_default_ctx: Some(8192),
        port: Some(5501),
        jinja: Some(true),
        reasoning_format: Some("deepseek".to_string()),
        mtp_draft_n_max: Some(4),
        mtp_draft_p_min: Some(0.8),
        cache_ram_mb: Some(4096),
        cache_reuse: Some(256),
        inference_params: Some(InferenceConfig {
            temperature: Some(0.7),
            ..Default::default()
        }),
        mlock: Some(true),
        ..Default::default()
    };

    let config = build_via_cascade(
        &["mtp".to_string(), "agent".to_string()],
        opts,
        GlobalDefaults {
            llama_base_port: BASE_PORT,
            ..Default::default()
        },
    );

    assert_eq!(config.context_size, Some(32_768));
    assert_eq!(config.port, Some(5501));
    assert!(config.jinja);
    assert_eq!(config.reasoning_format.as_deref(), Some("deepseek"));
    assert_eq!(config.spec_draft_n_max, Some(4));
    assert_eq!(config.cache_ram_mb, Some(4096));
    assert_eq!(config.cache_reuse, Some(256));
    assert!(config.mlock);
}

/// Tag-driven auto-detection has to survive the cascade — this is the
/// capability drift the epic exists to prevent.
#[test]
fn cascade_preserves_tag_driven_detection() {
    let config = build_via_cascade(
        &[
            "mtp".to_string(),
            "agent".to_string(),
            "reasoning".to_string(),
        ],
        ServerConfigOptions::default(),
        GlobalDefaults {
            llama_base_port: BASE_PORT,
            ..Default::default()
        },
    );

    assert!(config.jinja, "agent tag should auto-enable jinja");
    assert!(
        config.reasoning_format.is_some(),
        "reasoning tag should auto-detect a reasoning format"
    );
    assert!(
        config.spec_draft_n_max.is_some(),
        "mtp tag should auto-enable speculative decoding"
    );
    assert!(
        !config.embeddings,
        "a chat model's tags must never reach --embeddings"
    );
}

/// The embedding tag is the only route to `--embeddings`; there is no
/// `ServerConfigOptions` field that could carry it instead.
#[test]
fn cascade_preserves_embedding_tag_detection() {
    let config = build_via_cascade(
        &["embedding".to_string()],
        ServerConfigOptions::default(),
        GlobalDefaults::default(),
    );
    assert!(config.embeddings);
}

/// With caching on, the slot directory reaches the built config the same
/// way whether it arrived as an explicit option or a global default.
#[test]
fn cascade_reaches_the_built_config_with_cache_enabled() {
    let slot_dir = PathBuf::from("/slots/parity");

    let config = build_via_cascade(
        &[],
        ServerConfigOptions {
            slot_save_path: Some(slot_dir.clone()),
            ..Default::default()
        },
        GlobalDefaults {
            llama_base_port: BASE_PORT,
            cache_enabled: true,
            slot_dir: Some(slot_dir.clone()),
            ..Default::default()
        },
    );

    assert_eq!(config.slot_save_path, Some(slot_dir));
}

// ---------------------------------------------------------------
// The cascade is actually applied (not just passed through)
// ---------------------------------------------------------------

#[test]
fn cascade_applies_global_context_when_nothing_explicit() {
    let config = build_via_cascade(
        &[],
        ServerConfigOptions::default(),
        GlobalDefaults {
            default_ctx: Some(8192),
            ..Default::default()
        },
    );
    assert_eq!(config.context_size, Some(8192));
}

#[test]
fn cascade_lets_explicit_context_beat_global() {
    let config = build_via_cascade(
        &[],
        ServerConfigOptions {
            context_size: Some(32_768),
            ..Default::default()
        },
        GlobalDefaults {
            default_ctx: Some(8192),
            ..Default::default()
        },
    );
    assert_eq!(config.context_size, Some(32_768));
}

#[test]
fn cascade_carries_the_llama_base_port_from_globals() {
    let config = build_via_cascade(
        &[],
        ServerConfigOptions::default(),
        GlobalDefaults {
            llama_base_port: 5500,
            ..Default::default()
        },
    );
    assert_eq!(config.base_port, 5500);
}

/// mlock reaching the built config is what #631 plumbed; this asserts the
/// cascade did not sever it.
#[test]
fn cascade_carries_mlock_through_to_the_built_config() {
    let config = build_via_cascade(
        &[],
        ServerConfigOptions {
            mlock: Some(true),
            ..Default::default()
        },
        GlobalDefaults::default(),
    );
    assert!(config.mlock);
}

#[test]
fn cascade_suppresses_slot_path_when_cache_disabled() {
    let config = build_via_cascade(
        &[],
        ServerConfigOptions {
            slot_save_path: Some(PathBuf::from("/slots/ignored")),
            ..Default::default()
        },
        GlobalDefaults {
            cache_enabled: false,
            ..Default::default()
        },
    );
    assert_eq!(config.slot_save_path, None);
}
