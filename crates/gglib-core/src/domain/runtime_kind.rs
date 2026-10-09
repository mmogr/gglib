//! Which program serves a model: llama.cpp's `llama-server` for a model that
//! chats, stable-diffusion.cpp's `sd-server` for one that draws.
//!
//! Never stored. A model's runtime follows from its image family, which is
//! read from its tensors at import, so the two cannot disagree.

use serde::{Deserialize, Serialize};

use super::image_family::ImageFamily;

/// The program a model is served by.
///
/// On the wire `llama` and `stable_diffusion`. A record written before the
/// field existed reads as [`Self::Llama`], which is what every server was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    /// llama.cpp's `llama-server`, for a model that chats.
    #[default]
    Llama,
    /// stable-diffusion.cpp's `sd-server`, for a model that draws.
    StableDiffusion,
}

impl RuntimeKind {
    /// The runtime of a model with this image family: stable-diffusion.cpp
    /// for any family, llama.cpp for none.
    #[must_use]
    pub const fn of(image_family: Option<ImageFamily>) -> Self {
        match image_family {
            Some(_) => Self::StableDiffusion,
            None => Self::Llama,
        }
    }

    /// The project's name, as a person reads it: "llama.cpp" or
    /// "stable-diffusion.cpp".
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Llama => "llama.cpp",
            Self::StableDiffusion => "stable-diffusion.cpp",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_names_round_trip() {
        for (kind, wire) in [
            (RuntimeKind::Llama, "\"llama\""),
            (RuntimeKind::StableDiffusion, "\"stable_diffusion\""),
        ] {
            assert_eq!(serde_json::to_string(&kind).unwrap(), wire);
            assert_eq!(serde_json::from_str::<RuntimeKind>(wire).unwrap(), kind);
        }
    }

    #[test]
    fn a_record_without_the_field_is_llama() {
        assert_eq!(RuntimeKind::default(), RuntimeKind::Llama);
    }

    #[test]
    fn every_image_family_is_stable_diffusion_and_none_is_llama() {
        assert_eq!(RuntimeKind::of(None), RuntimeKind::Llama);
        for family in ImageFamily::ALL {
            assert_eq!(RuntimeKind::of(Some(family)), RuntimeKind::StableDiffusion);
        }
    }

    #[test]
    fn labels_name_the_projects() {
        assert_eq!(RuntimeKind::Llama.label(), "llama.cpp");
        assert_eq!(RuntimeKind::StableDiffusion.label(), "stable-diffusion.cpp");
    }
}
