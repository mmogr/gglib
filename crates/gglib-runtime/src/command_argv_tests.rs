//! The whole llama-server argument list, for the kinds of launch gglib makes.
//!
//! The tests beside these ask after one flag at a time. These hold every
//! argument and its place, so one that appears, goes or moves shows here
//! whichever layer it came from: the cascade, the translator or the command
//! builder.

use std::path::{Path, PathBuf};

use gglib_core::cache_config::KvCacheType;
use gglib_core::domain::InferenceConfig;

use super::build_command;
use crate::server_config::{ServerConfigOptions, build_server_config};
use crate::unified_server_config::{GlobalDefaults, UnifiedServerConfig};

/// The arguments a model with `tags` launches with under `options`, built as
/// the spawn builds them: the options laid over a template that says nothing,
/// the cache types and the RAM budget written in as resolved, the config
/// translated against the tags, and the projector set last.
fn launched(tags: &[&str], options: &ServerConfigOptions, projector: Option<&str>) -> Vec<String> {
    let mut options = ServerConfigOptions::default().overlay(options);
    options.cache_type_k = Some(KvCacheType::Q8_0);
    options.cache_type_v = Some(KvCacheType::Q8_0);
    options.cache_ram_mb = Some(4096);
    let tags: Vec<String> = tags.iter().map(|tag| (*tag).to_owned()).collect();
    let config = build_server_config(
        7,
        "a-model".to_owned(),
        PathBuf::from("/models/a-model.gguf"),
        0,
        &tags,
        options,
    )
    .with_mmproj(projector.map(PathBuf::from));
    build_command(Path::new("/fake/llama-server"), &config, 5500)
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

/// `line` as an argument list: no argument here holds a space.
fn arguments(line: &str) -> Vec<String> {
    line.split(' ').map(str::to_owned).collect()
}

/// A model with no tags, asked nothing: the context floor and the cache
/// settings. No `-ngl`: how much of the model goes to the GPU is left to
/// llama-server.
#[test]
fn a_model_asked_nothing_launches_on_the_floor_context_with_no_gpu_layer_count() {
    let args = launched(&[], &ServerConfigOptions::default(), None);

    assert_eq!(
        args,
        arguments(
            "-m /models/a-model.gguf --host 127.0.0.1 --port 5500 --metrics --parallel 1 \
             -c 4096 --cache-ram 4096 --cache-type-k q8_0 --cache-type-v q8_0"
        )
    );
    assert!(!args.iter().any(|arg| arg == "-ngl"), "{args:?}");
}

/// The `mtp` tag turns speculative decoding on at its defaults, the `agent`
/// and `reasoning` tags bring jinja and the reasoning format, and a model's
/// projector is passed between them.
#[test]
fn a_tagged_model_launches_with_what_its_tags_turn_on() {
    assert_eq!(
        launched(&["mtp"], &ServerConfigOptions::default(), None),
        arguments(
            "-m /models/a-model.gguf --host 127.0.0.1 --port 5500 --metrics --parallel 1 \
             -c 4096 --cache-ram 4096 --cache-type-k q8_0 --cache-type-v q8_0 \
             --spec-type draft-mtp --spec-draft-n-max 2 --spec-draft-p-min 0.75"
        )
    );
    assert_eq!(
        launched(
            &["agent", "mtp", "reasoning"],
            &ServerConfigOptions::default(),
            Some("/models/a-model.mmproj.gguf"),
        ),
        arguments(
            "-m /models/a-model.gguf --host 127.0.0.1 --port 5500 --metrics --parallel 1 \
             -c 4096 --jinja --mmproj /models/a-model.mmproj.gguf --reasoning-format deepseek \
             --cache-ram 4096 --cache-type-k q8_0 --cache-type-v q8_0 \
             --spec-type draft-mtp --spec-draft-n-max 2 --spec-draft-p-min 0.75"
        )
    );
}

/// A pin's options are the cascade's: what the request named over the proxy's
/// own settings. The sampling the operator stated is one of those settings,
/// and it is no argument: it travels in each request's body.
#[test]
fn a_pinned_launch_passes_the_cascades_options_and_no_sampling() {
    let pin = UnifiedServerConfig {
        explicit: ServerConfigOptions {
            context_size: Some(8192),
            model_server_ctx: Some(32_768),
            port: Some(9345),
            jinja: Some(false),
            reasoning_format: Some("none".to_owned()),
            mtp_draft_n_max: Some(4),
            mtp_draft_p_min: Some(0.6),
            cache_reuse: Some(256),
            mlock: Some(true),
            ..Default::default()
        },
        globals: GlobalDefaults {
            default_ctx: Some(16_384),
            cache_enabled: true,
            slot_dir: Some(PathBuf::from("/slots")),
            inference_override: Some(InferenceConfig {
                temperature: Some(0.2),
                top_p: Some(0.9),
                ..Default::default()
            }),
            ..Default::default()
        },
    };

    assert_eq!(
        launched(&["mtp", "reasoning"], &pin.resolved_options(), None),
        arguments(
            "-m /models/a-model.gguf --host 127.0.0.1 --port 5500 --metrics --parallel 1 \
             -c 8192 --no-jinja --slot-save-path /slots --cache-ram 4096 --cache-reuse 256 \
             --cache-type-k q8_0 --cache-type-v q8_0 \
             --spec-type draft-mtp --spec-draft-n-max 4 --spec-draft-p-min 0.6 \
             --load-mode mmap+mlock"
        )
    );
}

/// The `embedding` tag restricts the server to embeddings, and nothing else
/// about the launch changes.
#[test]
fn an_embedding_model_launches_restricted_to_embeddings() {
    assert_eq!(
        launched(&["embedding"], &ServerConfigOptions::default(), None),
        arguments(
            "-m /models/a-model.gguf --host 127.0.0.1 --port 5500 --metrics --parallel 1 \
             -c 4096 --embeddings --cache-ram 4096 --cache-type-k q8_0 --cache-type-v q8_0"
        )
    );
}
