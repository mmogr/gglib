//! `sd-server`'s command line, from an [`SdServerConfig`] and its family's
//! recipe.
//!
//! Pure: nothing is spawned or read, so each family's whole argv is pinned
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
/// and `port`, then the recipe's steps, guidance scale and sampler, and
/// `--fa` when the recipe turns flash attention on.
pub(crate) fn sd_server_args(config: &SdServerConfig, port: u16) -> Vec<OsString> {
    let recipe = config.family.recipe();
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
    flag("--steps", recipe.steps.to_string().into());
    flag("--cfg-scale", recipe.cfg.to_string().into());
    flag("--sampling-method", recipe.sampler.into());

    if recipe.flash_attention {
        args.push("--fa".into());
    }
    args
}
