//! `sd-server`'s argv, one exact golden per variant.
//!
//! Each golden is the whole command line, not a set of contains checks: a
//! flag added, dropped or moved fails here. The values are the recipes' and
//! samplings' own (`gglib_core::domain::ImageFamily::recipe` and
//! `::sampling`), which match the measured 2026-10-09 runs; Z-Image's and
//! Qwen-Image 2.1 Turbo's are their guides', and no run through gglib has
//! used them.

use std::ffi::OsString;
use std::path::PathBuf;

use gglib_core::domain::{ComponentRole, ImageFamily, ModelComponent};

use super::args::sd_server_args;
use super::config::SdServerConfig;

const PORT: u16 = 9123;

fn component(role: ComponentRole, path: &str) -> ModelComponent {
    ModelComponent {
        role,
        path: PathBuf::from(path),
    }
}

fn config(family: ImageFamily, components: Vec<ModelComponent>) -> SdServerConfig {
    SdServerConfig {
        model_id: 1,
        model_name: "m".to_owned(),
        model_path: PathBuf::from("M"),
        family,
        components,
        port: None,
    }
}

fn argv(config: &SdServerConfig) -> Vec<String> {
    sd_server_args(config, PORT)
        .into_iter()
        .map(|a: OsString| a.into_string().unwrap())
        .collect()
}

fn words(golden: &str) -> Vec<String> {
    golden.split_whitespace().map(str::to_owned).collect()
}

#[test]
fn flux_loads_a_diffusion_model_with_its_vae_and_both_text_encoders() {
    let flux = config(
        ImageFamily::Flux1,
        vec![
            component(ComponentRole::Vae, "V"),
            component(ComponentRole::ClipL, "C"),
            component(ComponentRole::T5xxl, "T"),
        ],
    );
    assert_eq!(
        argv(&flux),
        words(
            "--diffusion-model M --vae V --clip_l C --t5xxl T \
             --listen-ip 127.0.0.1 --listen-port 9123 \
             --steps 4 --cfg-scale 1 --sampling-method euler --flash-attn off --conv-direct off"
        )
    );
}

#[test]
fn sdxl_loads_one_checkpoint_with_m() {
    let sdxl = config(ImageFamily::Sdxl, Vec::new());
    assert_eq!(
        argv(&sdxl),
        words(
            "-m M --listen-ip 127.0.0.1 --listen-port 9123 \
             --steps 20 --cfg-scale 7 --sampling-method euler_a --flash-attn off --conv-direct off"
        )
    );
}

#[test]
fn qwen_image_loads_its_vae_and_language_model_with_flash_attention() {
    let qwen = config(
        ImageFamily::QwenImage21,
        vec![
            component(ComponentRole::Vae, "V"),
            component(ComponentRole::Llm, "L"),
        ],
    );
    assert_eq!(
        argv(&qwen),
        words(
            "--diffusion-model M --vae V --llm L \
             --listen-ip 127.0.0.1 --listen-port 9123 \
             --steps 20 --cfg-scale 6 --sampling-method euler --flash-attn on --conv-direct off"
        )
    );
}

/// Qwen-Image 2.1 Turbo, known by "turbo" in its file name, as
/// stable-diffusion.cpp's guide runs it: base's files, then its sigma preset
/// at 8 steps and guidance 1.
#[test]
fn qwen_image_turbo_adds_its_sigma_preset_at_turbos_steps() {
    let mut turbo = config(
        ImageFamily::QwenImage21,
        vec![
            component(ComponentRole::Vae, "V"),
            component(ComponentRole::Llm, "L"),
        ],
    );
    turbo.model_path = PathBuf::from("/models/qwen-image-2.1-turbo-Q8_0.gguf");
    assert_eq!(
        argv(&turbo),
        words(
            "--diffusion-model /models/qwen-image-2.1-turbo-Q8_0.gguf --vae V --llm L \
             --listen-ip 127.0.0.1 --listen-port 9123 \
             --steps 8 --cfg-scale 1 --sampling-method euler --sigmas qwen-image-2.1-turbo \
             --flash-attn on --conv-direct off"
        )
    );
}

/// Z-Image Turbo, known by "turbo" in its file name, as stable-diffusion.cpp's
/// guide runs it: the diffusion model, the Flux.1 VAE, Qwen3 4B as `--llm`,
/// 8 steps at guidance 1, and flash attention in the diffusion model alone.
#[test]
fn z_image_loads_its_vae_and_language_model_at_turbos_steps() {
    let mut z_image = config(
        ImageFamily::ZImage,
        vec![
            component(ComponentRole::Llm, "L"),
            component(ComponentRole::Vae, "V"),
        ],
    );
    z_image.model_path = PathBuf::from("z_image_turbo-Q8_0.gguf");
    assert_eq!(
        argv(&z_image),
        words(
            "--diffusion-model z_image_turbo-Q8_0.gguf --vae V --llm L \
             --listen-ip 127.0.0.1 --listen-port 9123 \
             --steps 8 --cfg-scale 1 --sampling-method euler --flash-attn off,diffusion=on --conv-direct off"
        )
    );
}

/// A Z-Image file whose name lacks "turbo" is base: the same files, 28 steps
/// at guidance 5, and the linear scale that drew its `Q8_0` file in `sd-cli`,
/// written as the number it is.
#[test]
fn base_z_image_draws_at_bases_steps_with_its_linear_scale() {
    let mut base = config(
        ImageFamily::ZImage,
        vec![
            component(ComponentRole::Vae, "V"),
            component(ComponentRole::Llm, "L"),
        ],
    );
    base.model_path = PathBuf::from("/models/z-image-Q8_0.gguf");
    assert_eq!(
        argv(&base),
        words(
            "--diffusion-model /models/z-image-Q8_0.gguf --vae V --llm L \
             --listen-ip 127.0.0.1 --listen-port 9123 \
             --steps 28 --cfg-scale 5 --sampling-method euler --linear-scale 0.000244140625 \
             --flash-attn off,diffusion=on --conv-direct off"
        )
    );
}

/// The database hands components back in whatever order they were linked;
/// the command line is the same for every order.
#[test]
fn components_come_out_in_role_order_whatever_order_they_were_given() {
    let in_order = config(
        ImageFamily::Flux1,
        vec![
            component(ComponentRole::Vae, "V"),
            component(ComponentRole::ClipL, "C"),
            component(ComponentRole::T5xxl, "T"),
        ],
    );
    let expected = argv(&in_order);
    for shuffled in [
        [
            ComponentRole::T5xxl,
            ComponentRole::Vae,
            ComponentRole::ClipL,
        ],
        [
            ComponentRole::ClipL,
            ComponentRole::T5xxl,
            ComponentRole::Vae,
        ],
        [
            ComponentRole::T5xxl,
            ComponentRole::ClipL,
            ComponentRole::Vae,
        ],
    ] {
        let components = shuffled
            .iter()
            .map(|role| {
                let path = match role {
                    ComponentRole::Vae => "V",
                    ComponentRole::ClipL => "C",
                    ComponentRole::T5xxl => "T",
                    ComponentRole::Llm => "L",
                };
                component(*role, path)
            })
            .collect();
        assert_eq!(
            argv(&config(ImageFamily::Flux1, components)),
            expected,
            "given as {shuffled:?}"
        );
    }

    let qwen_reversed = config(
        ImageFamily::QwenImage21,
        vec![
            component(ComponentRole::Llm, "L"),
            component(ComponentRole::Vae, "V"),
        ],
    );
    assert_eq!(
        &argv(&qwen_reversed)[..6],
        words("--diffusion-model M --vae V --llm L")
    );
}

/// A path with spaces stays one argument: the argv is never a shell string.
#[test]
fn a_path_with_spaces_is_one_argument() {
    let mut sdxl = config(ImageFamily::Sdxl, Vec::new());
    sdxl.model_path = PathBuf::from("/Models Folder/sd xl.safetensors");
    let args = argv(&sdxl);
    assert_eq!(args[0], "-m");
    assert_eq!(args[1], "/Models Folder/sd xl.safetensors");
}
