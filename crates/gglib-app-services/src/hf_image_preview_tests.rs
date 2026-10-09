//! Tests for [`image_preview`]: an image repository's companions, read from
//! one head before anything is downloaded, with what is already here and
//! what a download would fetch.

use std::path::Path;
use std::sync::Arc;

use gglib_core::domain::{ComponentRole, ImageFamily, TensorInfo, WeightsFormat};
use gglib_core::paths::repository_dir;
use gglib_core::ports::huggingface::fake_hub::{FakeHub, hub_file};
use gglib_core::ports::{
    GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort, HfQuantInfo, TensorTable,
};

use super::*;

/// Reads the head `flux` as a Flux.1 model's tensor table, and any other
/// head as a chat model's, whose metadata runs past it.
struct HeadParser;

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
        if head != b"flux" {
            return Err(GgufParseError::Io("the head ended".to_owned()));
        }
        let tensor = |name: &str, shape: &[u64]| TensorInfo {
            name: name.to_owned(),
            shape: shape.to_vec(),
        };
        Ok(TensorTable {
            format: WeightsFormat::Gguf,
            architecture: None,
            tensors: vec![
                tensor("double_blocks.0.img_attn.qkv.weight", &[9216, 3072]),
                tensor("single_blocks.0.linear1.weight", &[21504, 3072]),
                tensor("img_in.weight", &[3072, 64]),
                tensor("txt_in.weight", &[3072, 4096]),
            ],
        })
    }
}

const REPO: &str = "leejet/FLUX.1-schnell-gguf";

fn quant(name: &str, total_size: u64) -> HfQuantInfo {
    HfQuantInfo {
        name: name.to_owned(),
        shard_count: 1,
        total_size,
        file_paths: vec![format!("flux1-schnell-{name}.gguf")],
    }
}

/// A Hub where `head_of`'s first file reads as a Flux head and every
/// companion of the Flux recipe is listed at the recipe's size.
fn hub(head_of: &str) -> FakeHub {
    FakeHub {
        weights: vec![hub_file(head_of, 12_000, "w")],
        heads: vec![(head_of.to_owned(), b"flux".to_vec())],
        files_at: ImageFamily::Flux1
            .recipe()
            .components
            .iter()
            .map(|spec| (spec.repo.to_owned(), hub_file(spec.path, spec.size, "c")))
            .collect(),
        ..FakeHub::default()
    }
}

/// The Flux recipe's size of `role`.
fn size_of(role: ComponentRole) -> u64 {
    ImageFamily::Flux1
        .recipe()
        .components
        .iter()
        .find(|spec| spec.role == role)
        .unwrap()
        .size
}

/// A Flux head names the family and its three companions in the recipe's
/// order; one already in its repository's folder is present, and the bytes
/// to fetch are the other two.
#[tokio::test]
async fn a_flux_repository_lists_its_companions_and_what_a_download_fetches() {
    let models = tempfile::tempdir().unwrap();
    let vae_dir = repository_dir(models.path(), "unsloth/FLUX.1-schnell");
    std::fs::create_dir_all(&vae_dir).unwrap();
    std::fs::write(vae_dir.join("ae.safetensors"), b"here").unwrap();
    let hub = hub("flux1-schnell-q8_0.gguf");

    let preview = image_preview(
        &hub,
        &HeadParser,
        REPO,
        &[quant("q8_0", 12_000)],
        Some(models.path()),
    )
    .await
    .expect("a Flux head is an image model");

    assert_eq!(preview.family, ImageFamily::Flux1);
    let shown: Vec<_> = preview
        .companions
        .iter()
        .map(|c| (c.role, c.repo.as_str(), c.file_path.as_str(), c.present))
        .collect();
    assert_eq!(
        shown,
        [
            (
                ComponentRole::Vae,
                "unsloth/FLUX.1-schnell",
                "ae.safetensors",
                true
            ),
            (
                ComponentRole::ClipL,
                "comfyanonymous/flux_text_encoders",
                "clip_l.safetensors",
                false
            ),
            (
                ComponentRole::T5xxl,
                "comfyanonymous/flux_text_encoders",
                "t5xxl_fp16.safetensors",
                false
            ),
        ]
    );
    assert_eq!(
        preview.companions[0].size_bytes,
        size_of(ComponentRole::Vae)
    );
    assert_eq!(
        preview.fetch_bytes,
        size_of(ComponentRole::ClipL) + size_of(ComponentRole::T5xxl)
    );
}

/// With no models directory to look in, nothing is here and every
/// companion is fetched.
#[tokio::test]
async fn with_no_models_directory_every_companion_is_fetched() {
    let hub = hub("flux1-schnell-q8_0.gguf");

    let preview = image_preview(&hub, &HeadParser, REPO, &[quant("q8_0", 1)], None)
        .await
        .unwrap();

    assert!(preview.companions.iter().all(|c| !c.present));
    let all: u64 = ImageFamily::Flux1
        .recipe()
        .components
        .iter()
        .map(|s| s.size)
        .sum();
    assert_eq!(preview.fetch_bytes, all);
}

/// The head read is `Q8_0`'s first file when the repository has it, however
/// large, and the smallest quantization's otherwise.
#[tokio::test]
async fn the_head_read_is_q8_0s_and_else_the_smallests() {
    let q8 = hub("flux1-schnell-q8_0.gguf");
    image_preview(
        &q8,
        &HeadParser,
        REPO,
        &[
            quant("q4_k_m", 6_000),
            quant("q8_0", 12_000),
            quant("q2_k", 3_000),
        ],
        None,
    )
    .await
    .unwrap();

    let smallest = hub("flux1-schnell-q2_k.gguf");
    image_preview(
        &smallest,
        &HeadParser,
        REPO,
        &[
            quant("q4_k_m", 6_000),
            quant("q2_k", 3_000),
            quant("f16", 24_000),
        ],
        None,
    )
    .await
    .unwrap();

    let asked = |hub: &FakeHub| {
        hub.heads_asked
            .lock()
            .unwrap()
            .iter()
            .map(|(repo, path, _)| format!("{repo}/{path}"))
            .collect::<Vec<_>>()
    };
    assert_eq!(asked(&q8), [format!("{REPO}/flux1-schnell-q8_0.gguf")]);
    assert_eq!(
        asked(&smallest),
        [format!("{REPO}/flux1-schnell-q2_k.gguf")]
    );
}

/// A chat model's head names no family, and a family whose companion the
/// Hub does not list is answered as no preview, never as an error.
#[tokio::test]
async fn a_chat_head_and_an_unlisted_companion_answer_no_preview() {
    let chat = FakeHub {
        heads: vec![("flux1-schnell-q8_0.gguf".to_owned(), b"GGUF...".to_vec())],
        ..FakeHub::default()
    };
    assert!(
        image_preview(&chat, &HeadParser, REPO, &[quant("q8_0", 1)], None)
            .await
            .is_none()
    );

    let mut unlisted = hub("flux1-schnell-q8_0.gguf");
    unlisted.files_at.pop();
    assert!(
        image_preview(&unlisted, &HeadParser, REPO, &[quant("q8_0", 1)], None)
            .await
            .is_none()
    );
    assert!(
        image_preview(&unlisted, &HeadParser, REPO, &[], None)
            .await
            .is_none(),
        "a repository with no quantization has no head to read"
    );
}

/// The browser's listing carries the preview, its companions looked for in
/// the models directory the handler was given.
#[tokio::test]
async fn the_listing_served_carries_the_image_preview() {
    use crate::downloads::{DownloadDeps, DownloadOps};
    use crate::test_support::{MockDownloadManager, MockToolSupportDetector};

    let models = tempfile::tempdir().unwrap();
    let ops = DownloadOps::new(DownloadDeps {
        downloads: Arc::new(MockDownloadManager::new()),
        hf: Arc::new(hub("flux1-schnell-q8_0.gguf")),
        tool_detector: Arc::new(MockToolSupportDetector),
        gguf_parser: Arc::new(HeadParser),
        models_directory: Some(models.path().to_path_buf()),
    });

    let listed = ops.get_model_quantizations(REPO).await.unwrap();

    let image = listed.image.expect("the Flux repository's preview");
    assert_eq!(image.family, ImageFamily::Flux1);
    assert_eq!(image.companions.len(), 3);
    assert_eq!(listed.quantizations.len(), 1);
}
