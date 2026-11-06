//! Every argument a plan lists has a reason, and the reason names whose
//! choice it was: a launch with every optional flag on explains each; the
//! context's source follows the rung that chose it; an sd-server launch
//! explains its files, its address and its recipe.

use std::path::PathBuf;

use gglib_core::cache_config::{CacheRamSetting, KvCacheType};
use gglib_core::domain::{ChoiceSource, ComponentRole, ImageFamily, ModelComponent};
use gglib_core::ports::{LaunchOverrides, ModelLaunchSpec};
use gglib_core::server_config::{ContextSizeSource, ServerConfigOptions};
use std::collections::BTreeMap;

use super::super::explain::plan_launch;
use super::super::residency_tests::launch_spec;

/// A chat model tagged for everything a tag can turn on.
fn tagged() -> ModelLaunchSpec {
    ModelLaunchSpec {
        tags: ["agent", "reasoning", "mtp", "embedding"]
            .map(str::to_owned)
            .to_vec(),
        projector: Some(PathBuf::from("/models/qwen-mmproj.gguf")),
        file_size_bytes: 4 << 30,
        context_length: Some(32_768),
        ..launch_spec(3, "qwen")
    }
}

/// A plan of `spec` with `options` asked for on this launch.
fn planned(spec: ModelLaunchSpec, options: ServerConfigOptions) -> gglib_core::domain::LaunchPlan {
    planned_with(spec, options, None)
}

/// [`planned`], with the launch's own cache setting too.
fn planned_with(
    spec: ModelLaunchSpec,
    options: ServerConfigOptions,
    cache_ram: Option<CacheRamSetting>,
) -> gglib_core::domain::LaunchPlan {
    let overrides = LaunchOverrides { options, cache_ram };
    plan_launch(
        spec,
        &ServerConfigOptions::default(),
        CacheRamSetting::Auto,
        None,
        None,
        &overrides,
        &[],
    )
    .expect("a chat model plans")
}

fn flags(plan: &gglib_core::domain::LaunchPlan) -> Vec<&str> {
    plan.args.iter().map(|a| a.flag.as_str()).collect()
}

/// Each flag's source, by flag.
fn sources(plan: &gglib_core::domain::LaunchPlan) -> BTreeMap<&str, ChoiceSource> {
    plan.args
        .iter()
        .map(|a| (a.flag.as_str(), a.source))
        .collect()
}

fn unexplained(plan: &gglib_core::domain::LaunchPlan) -> Vec<&str> {
    plan.args
        .iter()
        .filter(|a| a.decision == "unexplained")
        .map(|a| a.flag.as_str())
        .collect()
}

/// Every flag the token list can emit is sent here, and each has a reason
/// of its own: none falls through to the `unexplained` arm.
#[test]
fn every_llama_flag_has_a_reason() {
    use ChoiceSource::{Gglib, ModelAuthor, You};
    let plan = planned(
        tagged(),
        ServerConfigOptions {
            context_size: Some(8192),
            slot_save_path: Some(PathBuf::from("/cache/slots")),
            cache_reuse: Some(256),
            mlock: Some(true),
            ..Default::default()
        },
    );

    assert_eq!(unexplained(&plan), Vec::<&str>::new(), "{:?}", flags(&plan));
    // Whose choice each was, on a launch where the tags decide: the files
    // the author's, the context, the cache dir and mlock yours, the rest
    // gglib's.
    let expected: BTreeMap<&str, ChoiceSource> = [
        ("-m", ModelAuthor),
        ("--host", Gglib),
        ("--port", Gglib),
        ("--metrics", Gglib),
        ("--parallel", Gglib),
        ("-c", You),
        ("--jinja", Gglib),
        ("--embeddings", Gglib),
        ("--mmproj", You),
        ("--reasoning-format", Gglib),
        ("--slot-save-path", You),
        ("--cache-ram", Gglib),
        ("--cache-reuse", Gglib),
        ("--cache-type-k", Gglib),
        ("--cache-type-v", Gglib),
        ("--spec-type", Gglib),
        ("--spec-draft-n-max", Gglib),
        ("--spec-draft-p-min", Gglib),
        ("--load-mode", You),
    ]
    .into_iter()
    .collect();
    assert_eq!(sources(&plan), expected);
    for expected in [
        "-m",
        "--host",
        "--port",
        "--metrics",
        "--parallel",
        "-c",
        "--jinja",
        "--embeddings",
        "--mmproj",
        "--reasoning-format",
        "--slot-save-path",
        "--cache-ram",
        "--cache-reuse",
        "--cache-type-k",
        "--cache-type-v",
        "--spec-type",
        "--spec-draft-n-max",
        "--spec-draft-p-min",
        "--load-mode",
    ] {
        assert!(flags(&plan).contains(&expected), "{expected} is sent");
    }
    assert_eq!(plan.program, "llama-server");
}

/// The port is taken when the launch happens, so the plan spells it `auto`
/// and the command line shows it so.
#[test]
fn the_port_is_auto_and_the_line_is_the_tokens_in_order() {
    let plan = planned(launch_spec(3, "qwen"), ServerConfigOptions::default());
    let port = plan.args.iter().find(|a| a.flag == "--port").unwrap();
    assert_eq!(port.value.as_deref(), Some("auto"));
    assert!(
        plan.command_line()
            .starts_with("llama-server -m /nonexistent/qwen.gguf --host 127.0.0.1 --port auto --metrics --parallel 1"),
        "{}",
        plan.command_line()
    );
}

/// The context flag's source is the rung that chose it: asked for, this
/// model's default, your default, or the floor when nothing was set.
#[test]
fn the_context_flags_source_is_the_rung_that_chose_it() {
    let ctx = |plan: &gglib_core::domain::LaunchPlan| {
        let arg = plan.args.iter().find(|a| a.flag == "-c").unwrap().clone();
        (
            arg.value.unwrap(),
            arg.source,
            plan.fit.clone().unwrap().source,
        )
    };

    let asked = planned(
        launch_spec(3, "qwen"),
        ServerConfigOptions {
            context_size: Some(8192),
            ..Default::default()
        },
    );
    assert_eq!(
        ctx(&asked),
        (
            "8192".to_owned(),
            ChoiceSource::You,
            ContextSizeSource::Explicit
        )
    );

    let models_own = planned(
        ModelLaunchSpec {
            server_defaults: Some(gglib_core::domain::ServerConfig {
                context_length: Some(16_384),
            }),
            ..launch_spec(3, "qwen")
        },
        ServerConfigOptions::default(),
    );
    assert_eq!(
        ctx(&models_own),
        (
            "16384".to_owned(),
            ChoiceSource::You,
            ContextSizeSource::ModelServerDefaults
        )
    );

    // Nothing set and nothing to fit (no trained window): the floor.
    let floor = planned(launch_spec(3, "qwen"), ServerConfigOptions::default());
    let (value, source, rung) = ctx(&floor);
    assert_eq!(
        value,
        gglib_core::settings::DEFAULT_CONTEXT_SIZE.to_string()
    );
    assert_eq!(
        (source, rung),
        (ChoiceSource::Gglib, ContextSizeSource::BuiltInDefault)
    );
    let fit = floor.fit.unwrap();
    assert_eq!(fit.fitted, None, "no trained window, so no fit");
    assert_eq!(fit.trained_ctx, None);
}

/// KV types asked for on the launch are yours; the default is gglib's.
#[test]
fn an_explicit_kv_type_is_yours_and_the_default_is_gglibs() {
    let default = planned(launch_spec(3, "qwen"), ServerConfigOptions::default());
    let k = |plan: &gglib_core::domain::LaunchPlan| {
        plan.args
            .iter()
            .find(|a| a.flag == "--cache-type-k")
            .unwrap()
            .clone()
    };
    assert_eq!(k(&default).source, ChoiceSource::Gglib);
    assert_eq!(k(&default).value.as_deref(), Some("q8_0"));

    let asked = planned(
        launch_spec(3, "qwen"),
        ServerConfigOptions {
            cache_type_k: Some(KvCacheType::F16),
            cache_type_v: Some(KvCacheType::F16),
            ..Default::default()
        },
    );
    assert_eq!(k(&asked).source, ChoiceSource::You);
    assert_eq!(k(&asked).value.as_deref(), Some("f16"));
}

/// An image model's plan is sd-server's command: its weights, each linked
/// file, the loopback address with the port still to be taken, and the
/// family's recipe, every flag explained; it has no context fit.
#[test]
fn an_image_models_plan_is_sd_servers_command_with_no_fit() {
    let component = |role, name: &str| ModelComponent {
        role,
        path: PathBuf::from(format!("/models/{name}")),
    };
    let spec = ModelLaunchSpec {
        name: "flux".to_owned(),
        file_path: PathBuf::from("/models/flux1-schnell-q8_0.gguf"),
        image_family: Some(ImageFamily::Flux1),
        components: vec![
            component(ComponentRole::T5xxl, "t5xxl_fp16.safetensors"),
            component(ComponentRole::Vae, "ae.safetensors"),
            component(ComponentRole::ClipL, "clip_l.safetensors"),
        ],
        file_size_bytes: 12 << 30,
        ..launch_spec(9, "flux")
    };

    let plan = planned(spec, ServerConfigOptions::default());

    assert_eq!(plan.program, "sd-server");
    assert!(plan.fit.is_none(), "an image model has no context");
    let unexplained: Vec<&str> = plan
        .args
        .iter()
        .filter(|a| a.decision == "unexplained")
        .map(|a| a.flag.as_str())
        .collect();
    assert_eq!(unexplained, Vec::<&str>::new(), "{:?}", flags(&plan));
    let by_flag = |flag: &str| plan.args.iter().find(|a| a.flag == flag).unwrap().clone();
    assert_eq!(
        by_flag("--diffusion-model").source,
        ChoiceSource::ModelAuthor
    );
    assert_eq!(by_flag("--vae").source, ChoiceSource::You);
    assert_eq!(by_flag("--vae").decision, "parts");
    assert_eq!(by_flag("--listen-port").value.as_deref(), Some("auto"));
    assert_eq!(by_flag("--steps").decision, "recipe");
    assert!(
        plan.narration.decisions.iter().any(|d| d.label == "family"),
        "narrated as a launch is"
    );
}

/// The same flags asked for on the launch are yours, `--no-jinja` included,
/// and an explicit cache size is yours where the auto size was gglib's.
#[test]
fn flags_asked_for_on_the_launch_are_yours() {
    let plan = planned_with(
        tagged(),
        ServerConfigOptions {
            jinja: Some(false),
            reasoning_format: Some("deepseek-legacy".to_owned()),
            mtp_draft_n_max: Some(4),
            ..Default::default()
        },
        Some(CacheRamSetting::ExplicitMb(2048)),
    );

    assert_eq!(unexplained(&plan), Vec::<&str>::new(), "{:?}", flags(&plan));
    let got = sources(&plan);
    assert_eq!(got.get("--no-jinja"), Some(&ChoiceSource::You));
    assert_eq!(got.get("--jinja"), None);
    assert_eq!(got.get("--reasoning-format"), Some(&ChoiceSource::You));
    assert_eq!(got.get("--spec-draft-n-max"), Some(&ChoiceSource::You));
    assert_eq!(got.get("--cache-ram"), Some(&ChoiceSource::You));
    assert_eq!(
        plan.args
            .iter()
            .find(|a| a.flag == "--cache-ram")
            .unwrap()
            .value
            .as_deref(),
        Some("2048")
    );
}

/// Your default context is yours.
#[test]
fn a_global_default_context_is_yours() {
    let overrides = LaunchOverrides::default();
    let plan = plan_launch(
        launch_spec(3, "qwen"),
        &ServerConfigOptions::default(),
        CacheRamSetting::Auto,
        None,
        Some(12_288),
        &overrides,
        &[],
    )
    .unwrap();
    let ctx = plan.args.iter().find(|a| a.flag == "-c").unwrap();
    assert_eq!(
        (ctx.value.as_deref(), ctx.source),
        (Some("12288"), ChoiceSource::You)
    );
    assert_eq!(plan.fit.unwrap().source, ContextSizeSource::GlobalDefault);
}

/// Every image family's command, under a turbo and a base file name, with
/// a file in every role, is explained flag by flag: between them the
/// all-in-one `-m`, `--llm`, `--sigmas` and `--linear-scale` are emitted
/// and explained.
#[test]
fn every_image_familys_command_is_explained() {
    let mut emitted = std::collections::BTreeSet::new();
    for family in ImageFamily::ALL {
        for file in ["image-turbo.gguf", "image.gguf"] {
            let spec = ModelLaunchSpec {
                name: format!("{family:?}"),
                file_path: PathBuf::from(format!("/models/{file}")),
                image_family: Some(family),
                components: ComponentRole::ALL
                    .into_iter()
                    .map(|role| ModelComponent {
                        role,
                        path: PathBuf::from(format!("/models/{}.safetensors", role.as_str())),
                    })
                    .collect(),
                file_size_bytes: 4 << 30,
                ..launch_spec(11, "image")
            };
            let plan = planned(spec, ServerConfigOptions::default());
            assert_eq!(plan.program, "sd-server", "{family:?} {file}");
            assert_eq!(
                unexplained(&plan),
                Vec::<&str>::new(),
                "{family:?} {file}: {:?}",
                flags(&plan)
            );
            for component in ["--vae", "--clip_l", "--t5xxl", "--llm"] {
                let arg = plan.args.iter().find(|a| a.flag == component);
                assert_eq!(
                    arg.map(|a| a.source),
                    Some(ChoiceSource::You),
                    "{family:?} {file} {component}"
                );
            }
            emitted.extend(plan.args.iter().map(|a| a.flag.clone()));
        }
    }
    for flag in [
        "-m",
        "--diffusion-model",
        "--llm",
        "--sigmas",
        "--linear-scale",
    ] {
        assert!(
            emitted.contains(flag),
            "{flag} was emitted by no family: {emitted:?}"
        );
    }
}
