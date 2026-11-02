//! `sd-server`'s command line, from an [`SdServerConfig`], its family's
//! recipe and its variant's sampling.
//!
//! Pure: nothing is spawned or read, so each variant's whole argv is pinned
//! by a golden in `args_tests.rs`. The flags are stable-diffusion.cpp's own,
//! as `sd-server --help` lists them at the pinned release; a flag given at
//! launch becomes the default for every request the server answers.

use std::ffi::OsString;

use gglib_core::domain::{ComponentRole, Placement};

use super::config::SdServerConfig;

/// The arguments `sd-server` is started with, in this order: the main file
/// (`--diffusion-model` for a diffusion-only file, `-m` for an all-in-one
/// checkpoint), each component in role order (`--vae`, `--clip_l`,
/// `--t5xxl`, `--llm`) whatever order it was given in, the loopback address
/// and `port`, then the variant's steps, guidance scale and sampler (the
/// variant read from the main file's name, [`ImageFamily::sampling`]), its
/// `--sigmas` preset and its `--linear-scale` when it has them, and
/// `--flash-attn` with the recipe's mode, and `--conv-direct off`. Both are
/// given though stable-diffusion.cpp defaults them to `auto`: that default
/// came with master-995, and no recipe has drawn with it.
///
/// [`ImageFamily::sampling`]: gglib_core::domain::ImageFamily::sampling
pub(crate) fn sd_server_args(config: &SdServerConfig, port: u16) -> Vec<OsString> {
    let recipe = config.family.recipe();
    let file_name = config.model_path.file_name().unwrap_or_default();
    let sampling = config.family.sampling(&file_name.to_string_lossy());
    let mut args: Vec<OsString> = Vec::new();
    let mut flag = |name: &str, value: OsString| {
        args.push(name.into());
        args.push(value);
    };

    let main_flag = match recipe.placement {
        Placement::DiffusionOnly => "--diffusion-model",
        Placement::AllInOne => "-m",
    };
    flag(main_flag, config.model_path.clone().into_os_string());

    for role in ComponentRole::ALL {
        for component in config.components.iter().filter(|c| c.role == role) {
            flag(
                &format!("--{}", role.as_str()),
                component.path.clone().into_os_string(),
            );
        }
    }

    flag("--listen-ip", "127.0.0.1".into());
    flag("--listen-port", port.to_string().into());
    flag("--steps", sampling.steps.to_string().into());
    flag("--cfg-scale", sampling.cfg.to_string().into());
    flag("--sampling-method", sampling.sampler.into());
    if let Some(sigmas) = sampling.sigmas {
        flag("--sigmas", sigmas.into());
    }
    if let Some(scale) = sampling.linear_scale {
        flag("--linear-scale", scale.to_string().into());
    }
    flag("--flash-attn", recipe.flash_attention.mode().into());
    flag("--conv-direct", "off".into());
    args
}
