//! `GET /v1/models/{name}/detail` over real HTTP: one model read in full, by
//! whatever a chat request could name it with, and nothing about this
//! machine's disk.

mod fixtures;

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde_json::Value;

use gglib_core::domain::{AdmissionSnapshot, ResidentSlotSnapshot};
use gglib_core::ports::{
    Admission, LaunchOverrides, ModelRuntimeError, ModelRuntimePort, PinnedSpec, RunningTarget,
};

use fixtures::common::{settings_listing, spawn_proxy_with_settings};
use fixtures::pinned::{StaticCatalog, pin};

/// The profile the settings configure, so `qwen:coding` routes.
const PROFILE: &str = "coding";

/// A runtime with models resident by id, optionally pinned. Admits nothing:
/// reading a model must not need a launch.
#[derive(Debug, Default)]
struct Resident {
    /// `(slot, model id)` for each resident model; slot 0 is the primary.
    slots: Vec<(usize, u32)>,
    pinned: Option<(u32, &'static str)>,
}

#[async_trait]
impl ModelRuntimePort for Resident {
    async fn admit(
        &self,
        model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: Option<u64>,
        _overrides: LaunchOverrides,
    ) -> Result<Admission, ModelRuntimeError> {
        Err(ModelRuntimeError::ModelNotFound(model_name.to_owned()))
    }

    fn admission_snapshot(&self) -> AdmissionSnapshot {
        AdmissionSnapshot {
            slots: self
                .slots
                .iter()
                .map(|&(slot, model_id)| ResidentSlotSnapshot {
                    slot,
                    model_name: "qwen".to_owned(),
                    model_id,
                    port: 9000,
                    inflight: 0,
                    is_primary: slot == 0,
                    resident_for_secs: 1,
                    runtime: gglib_core::domain::RuntimeKind::Llama,
                })
                .collect(),
            ..AdmissionSnapshot::default()
        }
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }

    fn pinned(&self) -> Option<PinnedSpec> {
        self.pinned.map(|(id, name)| pin(id, name))
    }
}

/// A proxy over `qwen` (1), a second `qwen` (2), `org/qwen` (3),
/// `qwen-vision` (4), which has a projector, and `flux-draws` (5), a Flux.1
/// model linked to a VAE, with the `coding` profile configured.
async fn proxy(runtime: Resident) -> (String, tokio_util::sync::CancellationToken) {
    let catalog = StaticCatalog::numbered(&[
        (1, "qwen"),
        (2, "qwen"),
        (3, "org/qwen"),
        (4, "qwen-vision"),
        (5, "flux-draws"),
    ]);
    spawn_proxy_with_settings(
        Arc::new(runtime),
        Arc::new(catalog),
        Arc::new(settings_listing(PROFILE)),
    )
    .await
}

/// `GET {base}/v1/models/{path}/detail`, as status and JSON body.
async fn detail(base: &str, path: &str) -> (StatusCode, Value) {
    let response = Client::new()
        .get(format!("{base}/v1/models/{path}/detail"))
        .send()
        .await
        .expect("the proxy answers");
    let status = response.status();
    (status, response.json().await.expect("json"))
}

/// An id reaches exactly that model, even where another has its name.
#[tokio::test]
async fn an_id_reads_that_model() {
    let (base, cancel) = proxy(Resident::default()).await;
    let (status, body) = detail(&base, "2").await;
    cancel.cancel();

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["detail"]["id"], 2);
    assert_eq!(body["detail"]["name"], "qwen");
    assert_eq!(body["detail"]["quantization"], "Q4_K_M");
    assert!(body.get("profile").is_none(), "{body}");
}

/// A name reaches the model a chat request with that name would.
#[tokio::test]
async fn a_name_reads_the_model_it_resolves_to() {
    let (base, cancel) = proxy(Resident::default()).await;
    let (status, body) = detail(&base, "qwen").await;
    cancel.cancel();

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["detail"]["id"], 1);
}

/// `name:profile` is routed as on a chat request, and the profile is echoed
/// so the reader knows what it named.
#[tokio::test]
async fn a_profile_suffix_reads_the_base_model_and_names_the_profile() {
    let (base, cancel) = proxy(Resident::default()).await;
    let (status, body) = detail(&base, "2:coding").await;
    let (unknown, refused) = detail(&base, "2:nope").await;
    cancel.cancel();

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["profile"], PROFILE);
    assert_eq!(body["detail"]["id"], 2);
    assert_eq!(unknown, StatusCode::NOT_FOUND, "{refused}");
    assert_eq!(refused["error"]["code"], "profile_not_found");
}

/// A name with a `/` in it travels percent-encoded as one path segment.
#[tokio::test]
async fn a_name_with_a_slash_is_read_percent_encoded() {
    let (base, cancel) = proxy(Resident::default()).await;
    let (status, body) = detail(&base, "org%2Fqwen").await;
    cancel.cancel();

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["detail"]["id"], 3);
    assert_eq!(body["detail"]["name"], "org/qwen");
}

/// Serving is residency in either slot, by id: the other `qwen` is not
/// serving because one named `qwen` is. Neither the file path nor a port
/// reaches the reader.
#[tokio::test]
async fn serving_is_by_id_in_either_slot_and_no_path_or_port_is_sent() {
    let (base, cancel) = proxy(Resident {
        slots: vec![(1, 2)],
        ..Resident::default()
    })
    .await;
    let (_, resident) = detail(&base, "2").await;
    let (_, idle) = detail(&base, "1").await;
    cancel.cancel();

    assert_eq!(resident["detail"]["isServing"], true, "{resident}");
    assert_eq!(idle["detail"]["isServing"], false, "{idle}");
    for key in ["filePath", "port"] {
        assert!(resident["detail"].get(key).is_none(), "{key}: {resident}");
    }
}

/// Whether a model reads images reaches the reader; where its projector sits
/// on this machine's disk does not.
#[tokio::test]
async fn image_input_is_sent_and_the_projector_path_is_not() {
    let (base, cancel) = proxy(Resident::default()).await;
    let (_, linked) = detail(&base, "4").await;
    let (_, unlinked) = detail(&base, "1").await;
    cancel.cancel();

    assert_eq!(linked["detail"]["imageInput"], true, "{linked}");
    assert!(linked["detail"].get("projectorPath").is_none(), "{linked}");
    assert_eq!(unlinked["detail"]["imageInput"], false, "{unlinked}");
}

/// An image model's family, its links' roles and the roles it still needs
/// reach the reader; where a linked file sits on this machine's disk does
/// not.
#[tokio::test]
async fn an_image_models_family_and_links_are_sent_and_their_paths_are_not() {
    let (base, cancel) = proxy(Resident::default()).await;
    let (status, draws) = detail(&base, "5").await;
    let (_, chats) = detail(&base, "1").await;
    cancel.cancel();

    assert_eq!(status, StatusCode::OK, "{draws}");
    let read = &draws["detail"];
    assert_eq!(read["imageFamily"], "flux1", "{draws}");
    assert_eq!(read["components"][0]["role"], "vae", "{draws}");
    assert!(read["components"][0].get("path").is_none(), "{draws}");
    assert_eq!(read["components"][0]["present"], false, "{draws}");
    assert_eq!(
        read["missingComponents"],
        serde_json::json!(["clip_l", "t5xxl"])
    );
    assert!(chats["detail"].get("imageFamily").is_none(), "{chats}");
}

/// Unknown is the code a chat request for it gets.
#[tokio::test]
async fn an_unknown_model_is_404_model_not_found() {
    let (base, cancel) = proxy(Resident::default()).await;
    let (status, body) = detail(&base, "ghost").await;
    cancel.cancel();

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "model_not_found");
}

/// A pinned endpoint answers for its pin and for nothing `/v1/models` does
/// not list — including a model that shares the pin's name, and one named
/// with a suffix that is no profile, which would otherwise tell the reader
/// that the hidden model exists.
#[tokio::test]
async fn a_pinned_endpoint_reads_only_its_pin() {
    let (base, cancel) = proxy(Resident {
        pinned: Some((2, "qwen")),
        ..Resident::default()
    })
    .await;
    let (pinned, _) = detail(&base, "2").await;
    let (other, body) = detail(&base, "1").await;
    let (other_suffixed, suffixed_body) = detail(&base, "1:nope").await;
    let (pin_suffixed, pin_suffixed_body) = detail(&base, "2:nope").await;
    cancel.cancel();

    assert_eq!(pinned, StatusCode::OK);
    assert_eq!(other, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "model_not_found");
    assert_eq!(other_suffixed, StatusCode::NOT_FOUND, "{suffixed_body}");
    assert_eq!(suffixed_body["error"]["code"], "model_not_found");
    assert_eq!(pin_suffixed, StatusCode::NOT_FOUND, "{pin_suffixed_body}");
    assert_eq!(pin_suffixed_body["error"]["code"], "profile_not_found");
}

/// Behind the same gate as `/v1/models`: a request that claims the tunnel and
/// names no device reaches nothing.
#[tokio::test]
async fn a_tunnelled_request_with_no_device_is_refused() {
    let (base, cancel) = proxy(Resident::default()).await;
    let response = Client::new()
        .get(format!("{base}/v1/models/1/detail"))
        .header("via", "1.1 modelpipe")
        .header("x-modelpipe-peer", "3ca82708b995")
        .send()
        .await
        .expect("the proxy answers");
    let status = response.status();
    let body: Value = response.json().await.expect("json");
    cancel.cancel();

    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "device_not_paired");
}

/// The route sits beside `/load` under the same `{name}` parameter, and the
/// router builds with both: each answers from its handler, not the fallback.
#[tokio::test]
async fn the_detail_and_load_routes_are_served_side_by_side() {
    let (base, cancel) = proxy(Resident::default()).await;
    let (read, _) = detail(&base, "1").await;
    let load: Value = Client::new()
        .post(format!("{base}/v1/models/ghost/load"))
        .send()
        .await
        .expect("the proxy answers")
        .json()
        .await
        .expect("a handler's JSON, not the fallback's empty 404");
    cancel.cancel();

    assert_eq!(read, StatusCode::OK);
    assert_eq!(load["error"]["code"], "model_not_found", "{load}");
}
