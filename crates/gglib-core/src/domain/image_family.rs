//! An image model's family, read from its tensor names, and the recipe that
//! family is drawn with.
//!
//! An image model's GGUF has no metadata (the measured Flux schnell and
//! Qwen-Image 2.1 files hold no key-value pairs), so a family is known only
//! from the names and shapes of its tensors. [`ImageFamily::sniff`] reads
//! them with the rules stable-diffusion.cpp loads by (`src/model_loader.cpp`),
//! and [`ComponentRole::fits`] checks a file offered as one of a family's
//! components the same way. [`ImageFamily::recipe`] is the one table of what
//! each family needs beside its main file and how it is drawn.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use super::tensor_table::{TensorTable, WeightsFormat};

/// A family of image models gglib can draw with.
///
/// On the wire `flux1`, `sdxl` and `qwen-image-2.1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub enum ImageFamily {
    /// Flux.1 (schnell; dev sniffs the same).
    #[serde(rename = "flux1")]
    Flux1,
    /// Stable Diffusion XL.
    #[serde(rename = "sdxl")]
    Sdxl,
    /// Qwen-Image 2.1.
    #[serde(rename = "qwen-image-2.1")]
    QwenImage21,
}

/// A file a family draws with beside its main weights, named by the
/// stable-diffusion.cpp flag that loads it.
///
/// On the wire `vae`, `clip_l`, `t5xxl` and `llm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentRole {
    /// The VAE that decodes latents to pixels (`--vae`).
    Vae,
    /// The CLIP-L text encoder (`--clip_l`).
    ClipL,
    /// The T5-XXL text encoder (`--t5xxl`).
    T5xxl,
    /// A language model used as the text encoder (`--llm`).
    Llm,
}

/// How a family's main file holds the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// One file holds the diffusion model, the text encoders and the VAE,
    /// loaded with `-m`.
    AllInOne,
    /// The main file holds the diffusion model only, loaded with
    /// `--diffusion-model`; the rest are components.
    DiffusionOnly,
}

/// Where a component is fetched from by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComponentSpec {
    /// The role the file plays.
    pub role: ComponentRole,
    /// The Hugging Face repository that serves it.
    pub repo: &'static str,
    /// Its path in that repository.
    pub path: &'static str,
    /// Its size in bytes.
    pub size: u64,
}

/// The image sizes a family draws: multiples of `step`, from `min` to `max`
/// on each side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeRule {
    /// What each side must be a multiple of.
    pub step: u32,
    /// The shortest side.
    pub min: u32,
    /// The longest side.
    pub max: u32,
}

/// How a family is drawn: what it needs beside its main file, and its
/// defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Recipe {
    /// How the main file holds the model.
    pub placement: Placement,
    /// The components it needs, each with its default source.
    pub components: &'static [ComponentSpec],
    /// Sampling steps.
    pub steps: u32,
    /// Classifier-free guidance scale.
    pub cfg: f32,
    /// The stable-diffusion.cpp sampler name.
    pub sampler: &'static str,
    /// Whether flash attention is on.
    pub flash_attention: bool,
    /// The sizes it draws.
    pub size: SizeRule,
}

/// The one size rule's bounds; the step differs by family.
const fn sizes(step: u32) -> SizeRule {
    SizeRule {
        step,
        min: 256,
        max: 1536,
    }
}

/// Flux.1: sources checked against the Hub on 2026-10-09. The VAE comes from
/// `unsloth/FLUX.1-schnell`, an ungated copy byte-identical to the gated
/// `black-forest-labs` file the measurement used.
const FLUX1: Recipe = Recipe {
    placement: Placement::DiffusionOnly,
    components: &[
        ComponentSpec {
            role: ComponentRole::Vae,
            repo: "unsloth/FLUX.1-schnell",
            path: "ae.safetensors",
            size: 335_304_388,
        },
        ComponentSpec {
            role: ComponentRole::ClipL,
            repo: "comfyanonymous/flux_text_encoders",
            path: "clip_l.safetensors",
            size: 246_144_152,
        },
        ComponentSpec {
            role: ComponentRole::T5xxl,
            repo: "comfyanonymous/flux_text_encoders",
            path: "t5xxl_fp16.safetensors",
            size: 9_787_841_024,
        },
    ],
    steps: 4,
    cfg: 1.0,
    sampler: "euler",
    flash_attention: false,
    size: sizes(64),
};

/// SDXL: the checkpoint holds everything; `euler_a` is what the measured run
/// logged (stable-diffusion.cpp's default).
const SDXL: Recipe = Recipe {
    placement: Placement::AllInOne,
    components: &[],
    steps: 20,
    cfg: 7.0,
    sampler: "euler_a",
    flash_attention: false,
    size: sizes(64),
};

/// Qwen-Image 2.1: sizes step by 32, per stable-diffusion.cpp's guide.
const QWEN_IMAGE_21: Recipe = Recipe {
    placement: Placement::DiffusionOnly,
    components: &[
        ComponentSpec {
            role: ComponentRole::Vae,
            repo: "Comfy-Org/Qwen-Image-2.1",
            path: "vae/qwen_image_2.1_vae_bf16.safetensors",
            size: 675_509_688,
        },
        ComponentSpec {
            role: ComponentRole::Llm,
            repo: "Qwen/Qwen3-VL-8B-Instruct-GGUF",
            path: "Qwen3VL-8B-Instruct-Q8_0.gguf",
            size: 8_709_519_456,
        },
    ],
    steps: 20,
    cfg: 6.0,
    sampler: "euler",
    flash_attention: true,
    size: sizes(32),
};

impl ImageFamily {
    /// Every family, in sniff order.
    pub const ALL: [Self; 3] = [Self::QwenImage21, Self::Flux1, Self::Sdxl];

    /// The name a person reads: "Flux.1", "SDXL", "Qwen-Image 2.1".
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Flux1 => "Flux.1",
            Self::Sdxl => "SDXL",
            Self::QwenImage21 => "Qwen-Image 2.1",
        }
    }

    /// The wire name: `flux1`, `sdxl`, `qwen-image-2.1`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Flux1 => "flux1",
            Self::Sdxl => "sdxl",
            Self::QwenImage21 => "qwen-image-2.1",
        }
    }

    /// How this family is drawn.
    #[must_use]
    pub const fn recipe(self) -> &'static Recipe {
        match self {
            Self::Flux1 => &FLUX1,
            Self::Sdxl => &SDXL,
            Self::QwenImage21 => &QWEN_IMAGE_21,
        }
    }

    /// The family a main weights file belongs to, from its tensor names and
    /// shapes; `None` for anything else, a chat model's weights or a
    /// component included.
    ///
    /// Names are compared after one leading `model.diffusion_model.` or
    /// `diffusion_model.` is stripped, as stable-diffusion.cpp renames a
    /// diffusion-only file before it sniffs. The rules are its own, checked
    /// in this order:
    ///
    /// - Qwen-Image 2.1: `txt_in.text_norm.weight`, and `img_in.weight` with
    ///   64 input features.
    /// - Flux.1: `double_blocks.0.img_attn.qkv.weight` and
    ///   `single_blocks.0.linear1.weight`; `img_in.weight` with 64 input
    ///   features (384 is Fill, 128 Controls, 196 Flex.2); `txt_in.weight`
    ///   with 4096 (3584 is `LongCat`); and none of Flux.2's, Ovis's or Chroma
    ///   Radiance's own tensors.
    /// - SDXL: `input_blocks.`, `conditioner.embedders.1.`, `middle_block.1.`
    ///   and `output_blocks.3.1.transformer_blocks.1.` (Vega and SSD-1B lack
    ///   the last two), and `input_blocks.0.0.weight` with 4 input channels
    ///   (9 is inpainting, 8 pix2pix).
    #[must_use]
    pub fn sniff(table: &TensorTable) -> Option<Self> {
        let names = Normalised::of(table);
        if names.has("txt_in.text_norm.weight") && names.dim("img_in.weight", 1) == Some(64) {
            return Some(Self::QwenImage21);
        }
        if names.has("double_blocks.0.img_attn.qkv.weight")
            && names.has("single_blocks.0.linear1.weight")
            && names.dim("img_in.weight", 1) == Some(64)
            && names.dim("txt_in.weight", 1) == Some(4096)
            && !names.has_prefix("double_stream_modulation_img.")
            && !names.has_prefix("double_blocks.0.img_mlp.gate_proj.")
            && !names.has_prefix("nerf_final_layer_conv.")
        {
            return Some(Self::Flux1);
        }
        if names.has_prefix("input_blocks.")
            && names.has_prefix("conditioner.embedders.1.")
            && names.has_prefix("middle_block.1.")
            && names.has_prefix("output_blocks.3.1.transformer_blocks.1.")
            && names.dim("input_blocks.0.0.weight", 1) == Some(4)
        {
            return Some(Self::Sdxl);
        }
        None
    }
}

impl ComponentRole {
    /// Every role.
    pub const ALL: [Self; 4] = [Self::Vae, Self::ClipL, Self::T5xxl, Self::Llm];

    /// The name a person reads: "VAE", "CLIP-L", "T5-XXL", "LLM".
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Vae => "VAE",
            Self::ClipL => "CLIP-L",
            Self::T5xxl => "T5-XXL",
            Self::Llm => "LLM",
        }
    }

    /// The wire name, which is stable-diffusion.cpp's flag without `--`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vae => "vae",
            Self::ClipL => "clip_l",
            Self::T5xxl => "t5xxl",
            Self::Llm => "llm",
        }
    }

    /// Whether `table` is this role's file for `family`; the refusal says
    /// what was expected.
    ///
    /// Names are compared exactly, never normalised, so an all-in-one
    /// checkpoint's own VAE (`first_stage_model.decoder.…`) or text encoder
    /// (`conditioner.embedders.0.transformer.text_model.…`) never passes for
    /// a separate file. A role the family's recipe does not name is refused.
    ///
    /// # Errors
    ///
    /// The sentence naming what the role's file must hold.
    pub fn fits(self, family: ImageFamily, table: &TensorTable) -> Result<(), &'static str> {
        let in_recipe = family
            .recipe()
            .components
            .iter()
            .any(|spec| spec.role == self);
        if !in_recipe {
            return Err("a role this family's recipe does not use");
        }
        let dim =
            |name: &str, axis: usize| table.get(name).and_then(|t| t.shape.get(axis).copied());
        let (fits, expected) = match (self, family) {
            (Self::Vae, ImageFamily::Flux1) => (
                dim("decoder.conv_in.weight", 1) == Some(16)
                    && table.get("encoder.conv_in.weight").is_some(),
                "a Flux.1 VAE: decoder.conv_in.weight with 16 input channels, and encoder.conv_in.weight",
            ),
            (Self::Vae, ImageFamily::QwenImage21) => (
                table
                    .get("decoder.conv1.weight")
                    .is_some_and(|t| t.shape.len() == 5 && t.shape[1] == 64),
                "a Qwen-Image 2.1 VAE: decoder.conv1.weight of 5 dimensions with 64 input channels",
            ),
            (Self::ClipL, _) => (
                dim("text_model.embeddings.token_embedding.weight", 1) == Some(768)
                    && table.has_prefix("text_model.encoder.layers.11.")
                    && !table.has_prefix("text_model.encoder.layers.12."),
                "a CLIP-L text encoder: text_model.embeddings.token_embedding.weight of width 768, and 12 layers",
            ),
            (Self::T5xxl, _) => (
                dim("shared.weight", 1) == Some(4096)
                    && table
                        .get("encoder.block.23.layer.0.SelfAttention.q.weight")
                        .is_some(),
                "a T5-XXL text encoder: shared.weight of width 4096, and 24 encoder blocks",
            ),
            (Self::Llm, ImageFamily::QwenImage21) => (
                table.format == WeightsFormat::Gguf
                    && table.architecture.as_deref() == Some("qwen3vl")
                    && dim("token_embd.weight", 1) == Some(4096),
                "a Qwen3-VL 8B GGUF: architecture qwen3vl, token_embd.weight of width 4096",
            ),
            _ => (false, "a role this family's recipe does not use"),
        };
        if fits { Ok(()) } else { Err(expected) }
    }
}

/// A table's names as stable-diffusion.cpp sniffs them: one leading
/// `model.diffusion_model.` or `diffusion_model.` stripped, nothing else.
struct Normalised<'a> {
    tensors: Vec<(&'a str, &'a [u64])>,
}

impl<'a> Normalised<'a> {
    fn of(table: &'a TensorTable) -> Self {
        let tensors = table
            .tensors
            .iter()
            .map(|t| {
                let name = t
                    .name
                    .strip_prefix("model.diffusion_model.")
                    .or_else(|| t.name.strip_prefix("diffusion_model."))
                    .unwrap_or(&t.name);
                (name, t.shape.as_slice())
            })
            .collect();
        Self { tensors }
    }

    fn shape(&self, name: &str) -> Option<&'a [u64]> {
        self.tensors
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, shape)| *shape)
    }

    fn has(&self, name: &str) -> bool {
        self.shape(name).is_some()
    }

    fn dim(&self, name: &str, axis: usize) -> Option<u64> {
        self.shape(name).and_then(|shape| shape.get(axis).copied())
    }

    fn has_prefix(&self, prefix: &str) -> bool {
        self.tensors.iter().any(|(n, _)| n.starts_with(prefix))
    }
}

impl fmt::Display for ImageFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Display for ComponentRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A name that is no family's or no role's, as given.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown {what} {given:?}; expected one of {expected}")]
pub struct UnknownName {
    what: &'static str,
    given: String,
    expected: &'static str,
}

impl FromStr for ImageFamily {
    type Err = UnknownName;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|family| family.as_str() == s)
            .ok_or_else(|| UnknownName {
                what: "image family",
                given: s.to_owned(),
                expected: "flux1, sdxl, qwen-image-2.1",
            })
    }
}

impl FromStr for ComponentRole {
    type Err = UnknownName;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|role| role.as_str() == s)
            .ok_or_else(|| UnknownName {
                what: "component role",
                given: s.to_owned(),
                expected: "vae, clip_l, t5xxl, llm",
            })
    }
}

#[cfg(test)]
#[path = "image_family_tests.rs"]
mod tests;
