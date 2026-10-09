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

    /// The server program's name, as log lines and errors say it:
    /// "llama-server" or "sd-server".
    #[must_use]
    pub const fn server_name(self) -> &'static str {
        match self {
            Self::Llama => "llama-server",
            Self::StableDiffusion => "sd-server",
        }
    }

    /// The path a readiness probe asks for.
    ///
    /// llama-server answers `/health`. `sd-server` has no health route; its
    /// `/v1/models` answers without taking the lock a render holds, so it is
    /// the one route that says the server is up even while it draws.
    #[must_use]
    pub const fn health_path(self) -> &'static str {
        match self {
            Self::Llama => "/health",
            Self::StableDiffusion => "/v1/models",
        }
    }

    /// Whether a 2xx body from [`Self::health_path`] comes from this runtime's
    /// server and not from something else listening on the port.
    ///
    /// llama-server's `/health` is a small JSON object naming a status, its
    /// slots or an error, or nothing at all. `sd-server` lists exactly one
    /// model, `sd-cpp-local`, whatever it has loaded; a 200 without that id
    /// is some other server.
    #[must_use]
    pub fn is_ready_body(self, body: &str) -> bool {
        match self {
            Self::Llama => {
                body.contains("status")
                    || body.contains("slots")
                    || body.contains("error")
                    || body.is_empty()
            }
            Self::StableDiffusion => body.contains(SD_SERVER_MODEL_ID),
        }
    }
}

/// The one model id `sd-server`'s `/v1/models` lists, whatever it loaded.
const SD_SERVER_MODEL_ID: &str = "sd-cpp-local";

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
    fn each_runtime_is_probed_at_its_own_path() {
        assert_eq!(RuntimeKind::Llama.health_path(), "/health");
        assert_eq!(RuntimeKind::StableDiffusion.health_path(), "/v1/models");
        assert_eq!(RuntimeKind::Llama.server_name(), "llama-server");
        assert_eq!(RuntimeKind::StableDiffusion.server_name(), "sd-server");
    }

    #[test]
    fn llama_keeps_its_health_body_test() {
        for body in [
            r#"{"status":"ok"}"#,
            r#"{"slots":[]}"#,
            r#"{"error":"x"}"#,
            "",
        ] {
            assert!(RuntimeKind::Llama.is_ready_body(body), "{body:?}");
        }
        assert!(!RuntimeKind::Llama.is_ready_body("<html>nginx</html>"));
    }

    #[test]
    fn sd_is_ready_only_when_it_lists_its_model() {
        let sd = RuntimeKind::StableDiffusion;
        assert!(sd.is_ready_body(
            r#"{"object":"list","data":[{"id":"sd-cpp-local","object":"model","owned_by":"local"}]}"#
        ));
        // Another OpenAI-shaped server on the port, a llama-server among them.
        assert!(!sd.is_ready_body(r#"{"object":"list","data":[]}"#));
        assert!(!sd.is_ready_body(r#"{"data":[{"id":"qwen3-8b"}]}"#));
        assert!(!sd.is_ready_body(r#"{"status":"ok"}"#));
        assert!(!sd.is_ready_body(""));
    }

    #[test]
    fn labels_name_the_projects() {
        assert_eq!(RuntimeKind::Llama.label(), "llama.cpp");
        assert_eq!(RuntimeKind::StableDiffusion.label(), "stable-diffusion.cpp");
    }
}
