//! Tests for [`super`]: the family sniffed from each measured file, the
//! tables edited to be what the rules exclude, and the role each component
//! file fits.

use super::*;
use crate::domain::image_family_goldens::Golden;
use crate::domain::tensor_table::TensorInfo;

fn shape(table: &TensorTable, name: &str) -> Vec<u64> {
    table
        .get(name)
        .unwrap_or_else(|| panic!("{name} is in the table"))
        .shape
        .clone()
}

/// Whether `prefix{i}.` names a tensor for each `i` below `count` and not
/// for `count`.
fn numbered(table: &TensorTable, prefix: &str, count: usize) -> bool {
    (0..count).all(|i| table.has_prefix(&format!("{prefix}{i}.")))
        && !table.has_prefix(&format!("{prefix}{count}."))
}

fn renamed(table: &TensorTable, from: &str, to: &str) -> TensorTable {
    let mut table = table.clone();
    for tensor in &mut table.tensors {
        if tensor.name == from {
            to.clone_into(&mut tensor.name);
        }
    }
    table
}

fn reshaped(table: &TensorTable, name: &str, shape: &[u64]) -> TensorTable {
    let mut table = table.clone();
    for tensor in &mut table.tensors {
        if tensor.name == name {
            tensor.shape = shape.to_vec();
        }
    }
    table
}

fn with(table: &TensorTable, name: &str, shape: &[u64]) -> TensorTable {
    let mut table = table.clone();
    table.tensors.push(TensorInfo {
        name: name.to_owned(),
        shape: shape.to_vec(),
    });
    table
}

fn without_prefix(table: &TensorTable, prefix: &str) -> TensorTable {
    let mut table = table.clone();
    table.tensors.retain(|t| !t.name.starts_with(prefix));
    table
}

// ── The goldens hold what the rules read ────────────────────────────────────

#[test]
fn the_flux_golden_holds_the_tensors_the_rules_read() {
    let table = Golden::FluxSchnellQ8.table();
    assert_eq!(table.tensors.len(), 776);
    assert_eq!(table.architecture, None);
    assert_eq!(
        shape(&table, "double_blocks.0.img_attn.qkv.weight"),
        [9216, 3072]
    );
    assert_eq!(
        shape(&table, "single_blocks.0.linear1.weight"),
        [21504, 3072]
    );
    assert_eq!(shape(&table, "img_in.weight"), [3072, 64]);
    assert_eq!(shape(&table, "txt_in.weight"), [3072, 4096]);
    assert!(
        table.get("guidance_in.in_layer.weight").is_none(),
        "schnell has no guidance"
    );
    assert!(numbered(&table, "double_blocks.", 19));
    assert!(numbered(&table, "single_blocks.", 38));
}

#[test]
fn the_qwen_image_golden_holds_the_tensors_the_rules_read() {
    let table = Golden::QwenImage21Q8.table();
    assert_eq!(table.tensors.len(), 265);
    assert_eq!(shape(&table, "txt_in.text_norm.weight"), [4096]);
    assert_eq!(shape(&table, "img_in.weight"), [4096, 64]);
    assert!(numbered(&table, "transformer_blocks.", 32));
}

#[test]
fn the_sdxl_golden_holds_the_tensors_the_rules_read() {
    let table = Golden::SdxlBase.table();
    assert_eq!(table.tensors.len(), 2515);
    assert_eq!(
        shape(&table, "model.diffusion_model.input_blocks.0.0.weight"),
        [320, 4, 3, 3]
    );
    for name in [
        "conditioner.embedders.1.model.ln_final.weight",
        "model.diffusion_model.middle_block.1.norm.weight",
        "model.diffusion_model.output_blocks.3.1.transformer_blocks.1.attn1.to_k.weight",
    ] {
        assert!(table.get(name).is_some(), "{name}");
    }
}

#[test]
fn the_component_goldens_hold_the_tensors_the_roles_read() {
    let ae = Golden::FluxVae.table();
    assert_eq!(shape(&ae, "decoder.conv_in.weight"), [512, 16, 3, 3]);

    let qwen_vae = Golden::QwenImage21Vae.table();
    assert_eq!(
        shape(&qwen_vae, "decoder.conv1.weight"),
        [1152, 64, 1, 3, 3]
    );

    let clip = Golden::ClipL.table();
    assert_eq!(
        shape(&clip, "text_model.embeddings.token_embedding.weight"),
        [49408, 768]
    );
    assert!(numbered(&clip, "text_model.encoder.layers.", 12));

    let t5 = Golden::T5xxl.table();
    assert_eq!(shape(&t5, "shared.weight"), [32128, 4096]);
    assert!(numbered(&t5, "encoder.block.", 24));

    let llm = Golden::Qwen3Vl8bQ8.table();
    assert_eq!(llm.format, WeightsFormat::Gguf);
    assert_eq!(llm.architecture.as_deref(), Some("qwen3vl"));
    assert_eq!(shape(&llm, "token_embd.weight"), [151_936, 4096]);
    assert!(numbered(&llm, "blk.", 36));
}

// ── Sniff ───────────────────────────────────────────────────────────────────

#[test]
fn each_main_file_sniffs_as_its_family() {
    assert_eq!(
        ImageFamily::sniff(&Golden::FluxSchnellQ8.table()),
        Some(ImageFamily::Flux1)
    );
    assert_eq!(
        ImageFamily::sniff(&Golden::QwenImage21Q8.table()),
        Some(ImageFamily::QwenImage21)
    );
    assert_eq!(
        ImageFamily::sniff(&Golden::SdxlBase.table()),
        Some(ImageFamily::Sdxl)
    );
}

#[test]
fn no_component_file_sniffs_as_a_family() {
    for golden in [
        Golden::FluxVae,
        Golden::ClipL,
        Golden::T5xxl,
        Golden::QwenImage21Vae,
        Golden::Qwen3Vl8bQ8,
    ] {
        assert_eq!(ImageFamily::sniff(&golden.table()), None, "{golden:?}");
    }
}

/// stable-diffusion.cpp prepends `model.diffusion_model.` to a
/// diffusion-only file before it sniffs, so a Flux file written that way is
/// the same family.
#[test]
fn a_flux_table_under_the_diffusion_model_prefix_is_flux() {
    let mut table = Golden::FluxSchnellQ8.table();
    for tensor in &mut table.tensors {
        tensor.name = format!("model.diffusion_model.{}", tensor.name);
    }
    assert_eq!(ImageFamily::sniff(&table), Some(ImageFamily::Flux1));

    let mut table = Golden::FluxSchnellQ8.table();
    for tensor in &mut table.tensors {
        tensor.name = format!("diffusion_model.{}", tensor.name);
    }
    assert_eq!(ImageFamily::sniff(&table), Some(ImageFamily::Flux1));
}

/// One prefix is stripped, not any number of them, and nothing else is.
#[test]
fn only_one_leading_prefix_is_stripped() {
    let mut table = Golden::FluxSchnellQ8.table();
    for tensor in &mut table.tensors {
        tensor.name = format!(
            "model.diffusion_model.model.diffusion_model.{}",
            tensor.name
        );
    }
    assert_eq!(ImageFamily::sniff(&table), None);

    let mut table = Golden::FluxSchnellQ8.table();
    for tensor in &mut table.tensors {
        tensor.name = format!("transformer.{}", tensor.name);
    }
    assert_eq!(ImageFamily::sniff(&table), None);
}

#[test]
fn flux_variants_the_rules_exclude_are_not_flux() {
    let flux = Golden::FluxSchnellQ8.table();
    for (what, table) in [
        (
            "Fill (img_in 384)",
            reshaped(&flux, "img_in.weight", &[3072, 384]),
        ),
        (
            "Controls (img_in 128)",
            reshaped(&flux, "img_in.weight", &[3072, 128]),
        ),
        (
            "LongCat (txt_in 3584)",
            reshaped(&flux, "txt_in.weight", &[3072, 3584]),
        ),
        (
            "Flux.2",
            with(
                &flux,
                "double_stream_modulation_img.lin.weight",
                &[18432, 3072],
            ),
        ),
        (
            "Ovis",
            with(
                &flux,
                "double_blocks.0.img_mlp.gate_proj.weight",
                &[12288, 3072],
            ),
        ),
        (
            "Chroma Radiance",
            with(&flux, "nerf_final_layer_conv.weight", &[3, 64, 3, 3]),
        ),
    ] {
        assert_eq!(ImageFamily::sniff(&table), None, "{what}");
    }
}

#[test]
fn sdxl_variants_the_rules_exclude_are_not_sdxl() {
    let sdxl = Golden::SdxlBase.table();
    let inpaint = reshaped(
        &sdxl,
        "model.diffusion_model.input_blocks.0.0.weight",
        &[320, 9, 3, 3],
    );
    assert_eq!(ImageFamily::sniff(&inpaint), None, "inpainting");
    let pix2pix = reshaped(
        &sdxl,
        "model.diffusion_model.input_blocks.0.0.weight",
        &[320, 8, 3, 3],
    );
    assert_eq!(ImageFamily::sniff(&pix2pix), None, "pix2pix");
    let no_middle = without_prefix(&sdxl, "model.diffusion_model.middle_block.1.");
    assert_eq!(ImageFamily::sniff(&no_middle), None, "no middle_block.1");
    let vega = without_prefix(
        &sdxl,
        "model.diffusion_model.output_blocks.3.1.transformer_blocks.1.",
    );
    assert_eq!(ImageFamily::sniff(&vega), None, "Vega");
}

/// A name that only ends in Qwen's norm tensor is not it.
#[test]
fn a_name_ending_in_the_qwen_norm_is_not_qwen() {
    let qwen = Golden::QwenImage21Q8.table();
    let table = renamed(
        &qwen,
        "txt_in.text_norm.weight",
        "foo.txt_in.text_norm.weight",
    );
    assert_eq!(ImageFamily::sniff(&table), None);
}

#[test]
fn qwen_with_another_img_in_width_is_not_qwen() {
    let qwen = Golden::QwenImage21Q8.table();
    let table = reshaped(&qwen, "img_in.weight", &[4096, 128]);
    assert_eq!(ImageFamily::sniff(&table), None);
}

/// A prefix rule reads the start of a name: Flux.2's own tensor under
/// another path is no exclusion, and SDXL's `middle_block.1.` under another
/// path is not there.
#[test]
fn a_prefix_only_mid_name_is_not_the_prefix() {
    let flux = with(
        &Golden::FluxSchnellQ8.table(),
        "extra.double_stream_modulation_img.lin.weight",
        &[18432, 3072],
    );
    assert_eq!(ImageFamily::sniff(&flux), Some(ImageFamily::Flux1));

    let mut sdxl = Golden::SdxlBase.table();
    for tensor in &mut sdxl.tensors {
        if let Some(rest) = tensor
            .name
            .strip_prefix("model.diffusion_model.middle_block.1.")
        {
            tensor.name = format!("model.diffusion_model.other.middle_block.1.{rest}");
        }
    }
    assert_eq!(ImageFamily::sniff(&sdxl), None);
}

/// SDXL's second text encoder is part of the rule: without it, no SDXL.
#[test]
fn sdxl_without_its_second_encoder_is_not_sdxl() {
    let table = without_prefix(&Golden::SdxlBase.table(), "conditioner.embedders.1.");
    assert_eq!(ImageFamily::sniff(&table), None);
}

// ── Role fits ───────────────────────────────────────────────────────────────

#[test]
fn every_recipe_component_fits_its_role() {
    let golden = |role, family| match (role, family) {
        (ComponentRole::Vae, ImageFamily::Flux1) => Golden::FluxVae,
        (ComponentRole::ClipL, _) => Golden::ClipL,
        (ComponentRole::T5xxl, _) => Golden::T5xxl,
        (ComponentRole::Vae, ImageFamily::QwenImage21) => Golden::QwenImage21Vae,
        (ComponentRole::Llm, _) => Golden::Qwen3Vl8bQ8,
        other => panic!("no golden for {other:?}"),
    };
    let mut checked = 0;
    for family in ImageFamily::ALL {
        for spec in family.recipe().components {
            let table = golden(spec.role, family).table();
            assert_eq!(
                spec.role.fits(family, &table),
                Ok(()),
                "{family} {}",
                spec.role
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 5);
}

#[test]
fn a_vae_of_one_family_does_not_fit_the_other() {
    let flux_vae = Golden::FluxVae.table();
    let qwen_vae = Golden::QwenImage21Vae.table();
    assert_eq!(
        ComponentRole::Vae.fits(ImageFamily::QwenImage21, &flux_vae),
        Err("a Qwen-Image 2.1 VAE: decoder.conv1.weight of 5 dimensions with 64 input channels")
    );
    assert_eq!(
        ComponentRole::Vae.fits(ImageFamily::Flux1, &qwen_vae),
        Err(
            "a Flux.1 VAE: decoder.conv_in.weight with 16 input channels, and encoder.conv_in.weight"
        )
    );
}

/// Qwen-Image 2.1's VAE is a video VAE of 64 latent channels; an older
/// one of 16, or a 2D convolution of the same width, is not it.
#[test]
fn a_qwen_vae_of_another_shape_does_not_fit() {
    let vae = Golden::QwenImage21Vae.table();
    for shape in [
        &[384, 16, 3, 3, 3][..],
        &[1152, 32, 1, 3, 3],
        &[1152, 64, 3, 3],
    ] {
        let table = reshaped(&vae, "decoder.conv1.weight", shape);
        assert!(
            ComponentRole::Vae
                .fits(ImageFamily::QwenImage21, &table)
                .is_err(),
            "{shape:?}"
        );
    }
}

/// The all-in-one checkpoint carries a VAE and a CLIP-L of its own under
/// longer names; names are matched whole, so it is neither.
#[test]
fn the_sdxl_checkpoint_is_not_a_vae_or_a_clip_l() {
    let sdxl = Golden::SdxlBase.table();
    assert!(ComponentRole::Vae.fits(ImageFamily::Flux1, &sdxl).is_err());
    assert!(
        ComponentRole::ClipL
            .fits(ImageFamily::Flux1, &sdxl)
            .is_err()
    );
}

#[test]
fn clip_l_is_not_a_t5xxl_and_t5xxl_is_not_a_clip_l() {
    assert_eq!(
        ComponentRole::T5xxl.fits(ImageFamily::Flux1, &Golden::ClipL.table()),
        Err("a T5-XXL text encoder: shared.weight of width 4096, and 24 encoder blocks")
    );
    assert!(
        ComponentRole::ClipL
            .fits(ImageFamily::Flux1, &Golden::T5xxl.table())
            .is_err()
    );
}

/// A component's names are its own whole names: the same tensors under a
/// longer name (as an all-in-one checkpoint carries its VAE and encoders) do
/// not fit.
#[test]
fn a_component_under_longer_names_does_not_fit() {
    let prefixed = |golden: Golden| {
        let mut table = golden.table();
        for tensor in &mut table.tensors {
            tensor.name = format!("first_stage_model.{}", tensor.name);
        }
        table
    };
    for (role, family, golden) in [
        (ComponentRole::Vae, ImageFamily::Flux1, Golden::FluxVae),
        (
            ComponentRole::Vae,
            ImageFamily::QwenImage21,
            Golden::QwenImage21Vae,
        ),
        (ComponentRole::ClipL, ImageFamily::Flux1, Golden::ClipL),
        (ComponentRole::T5xxl, ImageFamily::Flux1, Golden::T5xxl),
        (
            ComponentRole::Llm,
            ImageFamily::QwenImage21,
            Golden::Qwen3Vl8bQ8,
        ),
    ] {
        assert!(role.fits(family, &prefixed(golden)).is_err(), "{role}");
    }
}

/// The layer count is read from names that start with the encoder's path,
/// not from names that only contain it.
#[test]
fn a_clip_l_whose_layers_sit_under_another_path_does_not_fit() {
    let mut clip = Golden::ClipL.table();
    for tensor in &mut clip.tensors {
        if tensor.name.starts_with("text_model.encoder.") {
            tensor.name = format!("other.{}", tensor.name);
        }
    }
    assert!(
        ComponentRole::ClipL
            .fits(ImageFamily::Flux1, &clip)
            .is_err()
    );
}

/// Each shape and name a role reads is part of its rule.
#[test]
fn a_component_missing_one_part_of_its_rule_does_not_fit() {
    let ae = Golden::FluxVae.table();
    for (what, table) in [
        (
            "SD VAE in-channels",
            reshaped(&ae, "decoder.conv_in.weight", &[512, 4, 3, 3]),
        ),
        ("no encoder", without_prefix(&ae, "encoder.conv_in.")),
    ] {
        assert!(
            ComponentRole::Vae.fits(ImageFamily::Flux1, &table).is_err(),
            "{what}"
        );
    }

    let clip = reshaped(
        &Golden::ClipL.table(),
        "text_model.embeddings.token_embedding.weight",
        &[49408, 512],
    );
    assert!(
        ComponentRole::ClipL
            .fits(ImageFamily::Flux1, &clip)
            .is_err()
    );

    let t5 = without_prefix(&Golden::T5xxl.table(), "encoder.block.23.");
    assert!(ComponentRole::T5xxl.fits(ImageFamily::Flux1, &t5).is_err());
}

/// A CLIP-L with a thirteenth layer is some other CLIP.
#[test]
fn a_clip_with_more_than_twelve_layers_is_not_a_clip_l() {
    let clip = with(
        &Golden::ClipL.table(),
        "text_model.encoder.layers.12.mlp.fc1.weight",
        &[3072, 768],
    );
    assert!(
        ComponentRole::ClipL
            .fits(ImageFamily::Flux1, &clip)
            .is_err()
    );
}

/// A projector GGUF (architecture `clip`) is not the language model, nor is
/// the same table read from a safetensors file.
#[test]
fn a_projector_or_a_safetensors_copy_is_not_the_llm() {
    let llm = Golden::Qwen3Vl8bQ8.table();
    let mut projector = llm.clone();
    projector.architecture = Some("clip".to_owned());
    let expected = Err("a Qwen3-VL 8B GGUF: architecture qwen3vl, token_embd.weight of width 4096");
    assert_eq!(
        ComponentRole::Llm.fits(ImageFamily::QwenImage21, &projector),
        expected
    );

    let mut safetensors = llm;
    safetensors.format = WeightsFormat::Safetensors;
    assert_eq!(
        ComponentRole::Llm.fits(ImageFamily::QwenImage21, &safetensors),
        expected
    );
}

#[test]
fn a_role_outside_the_recipe_is_refused() {
    let refused = Err("a role this family's recipe does not use");
    assert_eq!(
        ComponentRole::Llm.fits(ImageFamily::Flux1, &Golden::Qwen3Vl8bQ8.table()),
        refused
    );
    assert_eq!(
        ComponentRole::T5xxl.fits(ImageFamily::QwenImage21, &Golden::T5xxl.table()),
        refused
    );
    assert_eq!(
        ComponentRole::Vae.fits(ImageFamily::Sdxl, &Golden::FluxVae.table()),
        refused
    );
}

// ── Names and the recipe table ──────────────────────────────────────────────

#[test]
fn names_round_trip_through_serde_from_str_and_display() {
    for family in ImageFamily::ALL {
        let wire = serde_json::to_string(&family).unwrap();
        assert_eq!(wire, format!("\"{family}\""));
        assert_eq!(family.to_string().parse::<ImageFamily>(), Ok(family));
    }
    assert_eq!(
        ImageFamily::ALL.map(ImageFamily::as_str),
        ["qwen-image-2.1", "flux1", "sdxl"]
    );
    assert_eq!(
        ImageFamily::ALL.map(ImageFamily::label),
        ["Qwen-Image 2.1", "Flux.1", "SDXL"]
    );
    for role in ComponentRole::ALL {
        let wire = serde_json::to_string(&role).unwrap();
        assert_eq!(wire, format!("\"{role}\""));
        assert_eq!(role.to_string().parse::<ComponentRole>(), Ok(role));
    }
    assert_eq!(
        ComponentRole::ALL.map(ComponentRole::as_str),
        ["vae", "clip_l", "t5xxl", "llm"]
    );
    assert_eq!(
        ComponentRole::ALL.map(ComponentRole::label),
        ["VAE", "CLIP-L", "T5-XXL", "LLM"]
    );
    let unknown = "clip".parse::<ComponentRole>().unwrap_err();
    assert_eq!(
        unknown.to_string(),
        "unknown component role \"clip\"; expected one of vae, clip_l, t5xxl, llm"
    );
}

#[test]
fn the_recipes_are_the_measured_ones() {
    let flux = ImageFamily::Flux1.recipe();
    assert_eq!(flux.placement, Placement::DiffusionOnly);
    assert_eq!(
        (flux.steps, flux.sampler, flux.flash_attention),
        (4, "euler", false)
    );
    assert_eq!(
        flux.size,
        SizeRule {
            step: 64,
            min: 256,
            max: 1536
        }
    );
    let sdxl = ImageFamily::Sdxl.recipe();
    assert_eq!(sdxl.placement, Placement::AllInOne);
    assert!(sdxl.components.is_empty());
    assert_eq!((sdxl.steps, sdxl.sampler), (20, "euler_a"));
    let qwen = ImageFamily::QwenImage21.recipe();
    assert_eq!(
        (qwen.steps, qwen.sampler, qwen.flash_attention),
        (20, "euler", true)
    );
    assert_eq!(qwen.size.step, 32);
    let roles = |r: &Recipe| r.components.iter().map(|c| c.role).collect::<Vec<_>>();
    assert_eq!(
        roles(flux),
        [
            ComponentRole::Vae,
            ComponentRole::ClipL,
            ComponentRole::T5xxl
        ]
    );
    assert_eq!(roles(qwen), [ComponentRole::Vae, ComponentRole::Llm]);
}
