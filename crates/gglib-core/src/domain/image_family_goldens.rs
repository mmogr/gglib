//! The measured files' tensor tables, from `testdata/image_families/`, for
//! the tests of whatever reads a family or a component from one.

use super::tensor_table::{TensorInfo, TensorTable, WeightsFormat};

/// One measured file whose table is a golden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Golden {
    FluxSchnellQ8,
    QwenImage21Q8,
    SdxlBase,
    FluxVae,
    ClipL,
    T5xxl,
    QwenImage21Vae,
    Qwen3Vl8bQ8,
}

impl Golden {
    /// The measured file's name.
    pub(crate) const fn file_name(self) -> &'static str {
        match self {
            Self::FluxSchnellQ8 => "flux1-schnell-q8_0.gguf",
            Self::QwenImage21Q8 => "qwen_image_2.1-Q8_0.gguf",
            Self::SdxlBase => "sd_xl_base_1.0.safetensors",
            Self::FluxVae => "ae.safetensors",
            Self::ClipL => "clip_l.safetensors",
            Self::T5xxl => "t5xxl_fp16.safetensors",
            Self::QwenImage21Vae => "qwen_image_2.1_vae_bf16.safetensors",
            Self::Qwen3Vl8bQ8 => "Qwen3VL-8B-Instruct-Q8_0.gguf",
        }
    }

    /// The golden's text, as the folder holds it.
    pub(crate) const fn text(self) -> &'static str {
        match self {
            Self::FluxSchnellQ8 => {
                include_str!("testdata/image_families/flux1-schnell-q8_0.gguf.tsv")
            }
            Self::QwenImage21Q8 => {
                include_str!("testdata/image_families/qwen_image_2.1-Q8_0.gguf.tsv")
            }
            Self::SdxlBase => {
                include_str!("testdata/image_families/sd_xl_base_1.0.safetensors.tsv")
            }
            Self::FluxVae => include_str!("testdata/image_families/ae.safetensors.tsv"),
            Self::ClipL => include_str!("testdata/image_families/clip_l.safetensors.tsv"),
            Self::T5xxl => include_str!("testdata/image_families/t5xxl_fp16.safetensors.tsv"),
            Self::QwenImage21Vae => {
                include_str!("testdata/image_families/qwen_image_2.1_vae_bf16.safetensors.tsv")
            }
            Self::Qwen3Vl8bQ8 => {
                include_str!("testdata/image_families/Qwen3VL-8B-Instruct-Q8_0.gguf.tsv")
            }
        }
    }

    /// The file's tensor table, as the parser reads it from the file.
    pub(crate) fn table(self) -> TensorTable {
        parse(self.file_name(), self.text())
    }
}

/// A golden's text read back as a table, its format taken from `file_name`'s
/// extension.
///
/// # Panics
///
/// On a line that is not `name<TAB>shape`.
pub(crate) fn parse(file_name: &str, text: &str) -> TensorTable {
    let gguf = std::path::Path::new(file_name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"));
    let format = if gguf {
        WeightsFormat::Gguf
    } else {
        WeightsFormat::Safetensors
    };
    let mut architecture = None;
    let mut tensors = Vec::new();
    for line in text.lines() {
        if let Some(arch) = line.strip_prefix("architecture: ") {
            architecture = Some(arch.to_owned());
            continue;
        }
        let (name, shape) = line.split_once('\t').expect("a golden line has a tab");
        let shape = if shape == "-" {
            Vec::new()
        } else {
            shape
                .split('x')
                .map(|dim| dim.parse().expect("a golden dimension is a number"))
                .collect()
        };
        tensors.push(TensorInfo {
            name: name.to_owned(),
            shape,
        });
    }
    TensorTable {
        format,
        architecture,
        tensors,
    }
}
