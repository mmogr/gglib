//! Tests for [`image_companions`]: a head read to know the family, and each
//! companion of the recipe looked up before anything is fetched.

use std::path::Path;

use super::*;
use crate::domain::image_family_goldens::Golden;
use crate::ports::huggingface::fake_hub::{FakeHub, hub_file};
use crate::ports::{GgufCapabilities, GgufMetadata, GgufParseError, TensorTable};

/// Reads a head that is a measured file's name as that file's tensor table,
/// and any other head as a chat model's, whose metadata runs past it.
pub(crate) struct HeadParser;

/// The golden a head names, when it is one's file name.
fn golden_named(head: &[u8]) -> Option<Golden> {
    [
        Golden::FluxSchnellQ8,
        Golden::QwenImage21Q8,
        Golden::SdxlBase,
        Golden::FluxVae,
        Golden::ClipL,
        Golden::T5xxl,
        Golden::QwenImage21Vae,
        Golden::Qwen3Vl8bQ8,
    ]
    .into_iter()
    .find(|golden| golden.file_name().as_bytes() == head)
}

impl GgufParserPort for HeadParser {
    fn parse(&self, _file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        unimplemented!("only heads are read here")
    }
    fn detect_capabilities(&self, _metadata: &GgufMetadata) -> GgufCapabilities {
        GgufCapabilities::empty()
    }
    fn tensor_table(&self, _path: &Path) -> Result<TensorTable, GgufParseError> {
        unimplemented!("only heads are read here")
    }
    fn tensor_table_of_head(&self, head: &[u8]) -> Result<TensorTable, GgufParseError> {
        golden_named(head).map(Golden::table).ok_or_else(|| {
            GgufParseError::Io("failed to fill whole buffer: the head ended".to_owned())
        })
    }
}

const FLUX_REPO: &str = "leejet/FLUX.1-schnell-gguf";
const FLUX_WEIGHTS: &str = "flux1-schnell-q8_0.gguf";

/// Every companion file of `family`'s recipe, as the Hub lists it: the
/// recipe's own size and an OID made from its path.
fn listed(family: ImageFamily) -> Vec<(String, HfFileInfo)> {
    family
        .recipe()
        .components
        .iter()
        .map(|spec| {
            let mut file = hub_file(spec.path, spec.size, &format!("oid-{}", spec.path));
            file.is_gguf = Path::new(spec.path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"));
            (spec.repo.to_owned(), file)
        })
        .collect()
}

/// A Hub whose weights file `path` has `head`, and which lists `files`.
fn hub(path: &str, head: &[u8], files: Vec<(String, HfFileInfo)>) -> FakeHub {
    FakeHub {
        heads: vec![(path.to_owned(), head.to_vec())],
        files_at: files,
        ..FakeHub::default()
    }
}

/// A Flux.1 head names the family, and its three companions come back in
/// the recipe's order, each from its own repository with the Hub's size and
/// OID.
#[tokio::test]
async fn a_flux_head_brings_its_three_companions_from_their_own_repositories() {
    let hub = hub(
        FLUX_WEIGHTS,
        FLUX_WEIGHTS.as_bytes(),
        listed(ImageFamily::Flux1),
    );
    let weights = hub_file(FLUX_WEIGHTS, 12_000, "w");

    let (family, companions) = image_companions(&hub, &HeadParser, FLUX_REPO, &weights)
        .await
        .unwrap()
        .expect("an image model");

    assert_eq!(family, ImageFamily::Flux1);
    let found: Vec<_> = companions
        .iter()
        .map(|c| (c.role, c.repo.as_str(), c.file.path.as_str(), c.file.size))
        .collect();
    assert_eq!(
        found,
        [
            (
                ComponentRole::Vae,
                "unsloth/FLUX.1-schnell",
                "ae.safetensors",
                335_304_388
            ),
            (
                ComponentRole::ClipL,
                "comfyanonymous/flux_text_encoders",
                "clip_l.safetensors",
                246_144_152
            ),
            (
                ComponentRole::T5xxl,
                "comfyanonymous/flux_text_encoders",
                "t5xxl_fp16.safetensors",
                9_787_841_024
            ),
        ]
    );
    assert_eq!(
        companions[0].file.oid.as_deref(),
        Some("oid-ae.safetensors")
    );
}

/// The head asked for is the first [`SNIFF_HEAD_BYTES`] of the first
/// weights file, in the model's own repository, and it is asked once.
#[tokio::test]
async fn the_head_read_is_the_first_mebibyte_of_the_first_weights_file() {
    let hub = hub(
        FLUX_WEIGHTS,
        FLUX_WEIGHTS.as_bytes(),
        listed(ImageFamily::Flux1),
    );

    image_companions(
        &hub,
        &HeadParser,
        FLUX_REPO,
        &hub_file(FLUX_WEIGHTS, 1, "w"),
    )
    .await
    .unwrap();

    assert_eq!(
        *hub.heads_asked.lock().unwrap(),
        [(FLUX_REPO.to_owned(), FLUX_WEIGHTS.to_owned(), 1_048_576)]
    );
    assert_eq!(SNIFF_HEAD_BYTES, 1 << 20);
}

/// Qwen-Image 2.1 brings its VAE and its language model, the latter a GGUF
/// of its own repository.
#[tokio::test]
async fn a_qwen_image_head_brings_its_vae_and_language_model() {
    let path = "qwen_image_2.1-Q8_0.gguf";
    let hub = hub(path, path.as_bytes(), listed(ImageFamily::QwenImage21));

    let (family, companions) = image_companions(
        &hub,
        &HeadParser,
        "leejet/Qwen-Image-2.1-GGUF",
        &hub_file(path, 1, "w"),
    )
    .await
    .unwrap()
    .expect("an image model");

    assert_eq!(family, ImageFamily::QwenImage21);
    let roles: Vec<_> = companions
        .iter()
        .map(|c| (c.role, c.repo.as_str()))
        .collect();
    assert_eq!(
        roles,
        [
            (ComponentRole::Vae, "Comfy-Org/Qwen-Image-2.1"),
            (ComponentRole::Llm, "Qwen/Qwen3-VL-8B-Instruct-GGUF"),
        ]
    );
}

/// SDXL's checkpoint holds everything: the family, and nothing to look up.
#[tokio::test]
async fn an_sdxl_head_names_its_family_and_no_companion() {
    let path = "sd_xl_base_1.0.safetensors";
    let hub = hub(path, path.as_bytes(), Vec::new());

    let found = image_companions(&hub, &HeadParser, "o/sdxl", &hub_file(path, 1, "w"))
        .await
        .unwrap();

    let (family, companions) = found.expect("an image model");
    assert_eq!(family, ImageFamily::Sdxl);
    assert!(companions.is_empty());
}

/// A chat model's head is not a table, and is no image model and no error.
#[tokio::test]
async fn a_chat_models_head_is_no_image_model() {
    let hub = hub("chat.Q8_0.gguf", b"GGUF and a tokenizer", Vec::new());

    let found = image_companions(
        &hub,
        &HeadParser,
        "o/chat-GGUF",
        &hub_file("chat.Q8_0.gguf", 1, "w"),
    )
    .await
    .unwrap();

    assert!(found.is_none());
}

/// A head the Hub will not serve leaves the download a chat model's: it is
/// not an error that stops the download.
#[tokio::test]
async fn a_head_that_cannot_be_read_is_no_image_model() {
    let hub = FakeHub::default();

    let found = image_companions(
        &hub,
        &HeadParser,
        FLUX_REPO,
        &hub_file(FLUX_WEIGHTS, 1, "w"),
    )
    .await
    .unwrap();

    assert!(found.is_none());
}

/// A component's file not on the Hub fails the lookup, and the error names
/// the repository and the path it was looked for at.
#[tokio::test]
async fn a_companion_the_hub_does_not_hold_is_named_in_the_error() {
    let mut files = listed(ImageFamily::Flux1);
    files.retain(|(_, file)| file.path != "t5xxl_fp16.safetensors");
    let hub = hub(FLUX_WEIGHTS, FLUX_WEIGHTS.as_bytes(), files);

    let refused = image_companions(
        &hub,
        &HeadParser,
        FLUX_REPO,
        &hub_file(FLUX_WEIGHTS, 1, "w"),
    )
    .await
    .expect_err("a companion is missing");

    assert_eq!(
        refused.to_string(),
        "No file t5xxl_fp16.safetensors in comfyanonymous/flux_text_encoders"
    );
}

/// A companion is looked up in its own repository: the same path in
/// another repository is not it.
#[tokio::test]
async fn a_companion_is_looked_up_in_its_own_repository() {
    let mut files = listed(ImageFamily::Flux1);
    for (repo, _) in &mut files {
        if repo == "unsloth/FLUX.1-schnell" {
            *repo = FLUX_REPO.to_owned();
        }
    }
    let hub = hub(FLUX_WEIGHTS, FLUX_WEIGHTS.as_bytes(), files);

    let refused = image_companions(
        &hub,
        &HeadParser,
        FLUX_REPO,
        &hub_file(FLUX_WEIGHTS, 1, "w"),
    )
    .await
    .expect_err("the VAE is not in its repository");

    assert!(matches!(
        refused,
        HfPortError::FileNotFound { ref model_id, .. } if model_id == "unsloth/FLUX.1-schnell"
    ));
}
