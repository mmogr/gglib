//! Tests for [`super::HubVariants`]: what is read from the Hub, and when.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use gglib_core::domain::{FileType, TensorInfo, TensorTable, WeightsFormat};
use gglib_core::download::{
    Contender, Fit, FitJudge, GgufFileRole, Pick, PickReason, VariantReader, pick_variant,
};
use gglib_core::ports::huggingface::{
    HfClientPort, HfFileInfo, HfPortError, HfPortResult, HfQuantInfo, HfRepoInfo, HfSearchOptions,
    HfSearchResult, SNIFF_HEAD_BYTES,
};
use gglib_core::ports::{
    GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort, HeadMetadata,
};

use super::{HEAD_BYTES, HubVariants};

const MIB: u64 = 1 << 20;
const Q8_0: FileType = FileType(7);

/// One file: its length, where its metadata ends, and where its tensor
/// table ends when it is an image model's.
#[derive(Clone, Copy)]
struct File {
    len: u64,
    metadata_ends: u64,
    image_table_ends: Option<u64>,
    /// Its whole header says it is a draft.
    sidecar: bool,
}

/// Files by path, each read as its first bytes, every read written down.
struct Hub {
    files: HashMap<&'static str, File>,
    reads: Mutex<Vec<(String, u64)>>,
}

impl Hub {
    fn new(files: &[(&'static str, File)]) -> Self {
        Self {
            files: files.iter().copied().collect(),
            reads: Mutex::new(Vec::new()),
        }
    }

    fn reads(&self) -> Vec<(String, u64)> {
        self.reads.lock().unwrap().clone()
    }
}

#[async_trait]
impl HfClientPort for Hub {
    async fn search(&self, _options: &HfSearchOptions) -> HfPortResult<HfSearchResult> {
        unimplemented!()
    }
    async fn list_quantizations(&self, _model_id: &str) -> HfPortResult<Vec<HfQuantInfo>> {
        unimplemented!()
    }
    async fn list_gguf_files(&self, _model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        unimplemented!()
    }
    async fn get_quantization_files(
        &self,
        _model_id: &str,
        _quantization: &str,
    ) -> HfPortResult<Vec<HfFileInfo>> {
        unimplemented!()
    }
    async fn get_commit_sha(&self, _model_id: &str) -> HfPortResult<String> {
        unimplemented!()
    }
    async fn get_model_info(&self, _model_id: &str) -> HfPortResult<HfRepoInfo> {
        unimplemented!()
    }
    async fn file_at(&self, _model_id: &str, _path: &str) -> HfPortResult<Option<HfFileInfo>> {
        unimplemented!()
    }

    /// The file's first `max_bytes`, each byte its offset's low byte, and
    /// the file's own path at the start so the parser can look it up.
    async fn read_head(&self, model_id: &str, path: &str, max_bytes: u64) -> HfPortResult<Vec<u8>> {
        self.reads
            .lock()
            .unwrap()
            .push((path.to_owned(), max_bytes));
        let file = self
            .files
            .get(path)
            .ok_or_else(|| HfPortError::FileNotFound {
                model_id: model_id.to_owned(),
                path: path.to_owned(),
            })?;
        let len = usize::try_from(file.len.min(max_bytes)).unwrap();
        let mut bytes = vec![0; len];
        bytes[..path.len()].copy_from_slice(path.as_bytes());
        Ok(bytes)
    }
}

/// Reads a head as [`Hub`] serves it: whole metadata once it reaches
/// `metadata_ends`, an image's tensor table once it reaches
/// `image_table_ends`.
struct Parser<'a>(&'a Hub);

impl Parser<'_> {
    fn file(&self, head: &[u8]) -> File {
        let end = head.iter().position(|&b| b == 0).unwrap_or(head.len());
        self.0.files[std::str::from_utf8(&head[..end]).unwrap()]
    }
}

impl GgufParserPort for Parser<'_> {
    fn parse(&self, _file_path: &Path) -> Result<GgufMetadata, GgufParseError> {
        unimplemented!()
    }
    fn detect_capabilities(&self, _metadata: &GgufMetadata) -> GgufCapabilities {
        GgufCapabilities::empty()
    }
    fn tensor_table(&self, _path: &Path) -> Result<TensorTable, GgufParseError> {
        unimplemented!()
    }
    fn tensor_table_of_head(&self, head: &[u8]) -> Result<TensorTable, GgufParseError> {
        let file = self.file(head);
        let tensors = match file.image_table_ends {
            Some(end) if head.len() as u64 >= end => flux_tensors(),
            Some(_) => return Err(GgufParseError::Io("the head ended".to_owned())),
            None => Vec::new(),
        };
        Ok(TensorTable {
            format: WeightsFormat::Gguf,
            architecture: None,
            tensors,
        })
    }
    fn parse_head(&self, head: &[u8], _file_name: &str) -> Result<HeadMetadata, GgufParseError> {
        let file = self.file(head);
        let complete = head.len() as u64 >= file.metadata_ends;
        let role = if file.sidecar {
            GgufFileRole::Sidecar
        } else {
            GgufFileRole::Weights
        };
        Ok(HeadMetadata {
            metadata: GgufMetadata {
                file_type: complete.then_some(Q8_0),
                context_length: complete.then_some(40_960),
                role,
                ..GgufMetadata::default()
            },
            complete,
            role: complete.then_some(role),
            ..HeadMetadata::default()
        })
    }
}

/// The smallest table FLUX.1 is sniffed from.
fn flux_tensors() -> Vec<TensorInfo> {
    let tensor = |name: &str, shape: &[u64]| TensorInfo {
        name: name.to_owned(),
        shape: shape.to_vec(),
    };
    vec![
        tensor("double_blocks.0.img_attn.qkv.weight", &[9216, 3072]),
        tensor("single_blocks.0.linear1.weight", &[21504, 3072]),
        tensor("img_in.weight", &[3072, 64]),
        tensor("txt_in.weight", &[3072, 4096]),
    ]
}

const CHAT: File = File {
    len: 8_000 * MIB,
    metadata_ends: 12 * MIB,
    image_table_ends: None,
    sidecar: false,
};

fn variants<'a>(hub: &'a Hub, parser: &'a Parser<'a>, paths: &[&str]) -> HubVariants<'a> {
    HubVariants::new(
        hub,
        parser,
        "owner/repo",
        paths.iter().map(|path| (*path).to_owned()).collect(),
    )
}

#[tokio::test]
async fn a_chat_models_head_is_one_read_and_is_read_once() {
    let hub = Hub::new(&[("chat.gguf", CHAT)]);
    let parser = Parser(&hub);
    let reader = variants(&hub, &parser, &["chat.gguf"]);

    let head = reader.head(0).await.unwrap();
    reader.head(0).await.unwrap();

    assert!(!head.head.complete);
    assert!(!head.image);
    assert_eq!(hub.reads(), [("chat.gguf".to_owned(), HEAD_BYTES)]);
}

#[tokio::test]
async fn an_image_model_whose_table_ends_in_its_head_is_known_from_it() {
    let flux = File {
        len: 12_000 * MIB,
        metadata_ends: 1024,
        image_table_ends: Some(50_000),
        sidecar: false,
    };
    let hub = Hub::new(&[("flux.gguf", flux)]);
    let parser = Parser(&hub);
    let reader = variants(&hub, &parser, &["flux.gguf"]);

    assert!(reader.head(0).await.unwrap().image);
    assert_eq!(hub.reads(), [("flux.gguf".to_owned(), HEAD_BYTES)]);
}

#[tokio::test]
async fn an_image_model_whose_table_runs_past_its_head_is_read_further() {
    let flux = File {
        len: 12_000 * MIB,
        metadata_ends: 1024,
        image_table_ends: Some(200_000),
        sidecar: false,
    };
    let hub = Hub::new(&[("flux.gguf", flux)]);
    let parser = Parser(&hub);
    let reader = variants(&hub, &parser, &["flux.gguf"]);

    assert!(reader.head(0).await.unwrap().image);
    assert_eq!(
        hub.reads(),
        [
            ("flux.gguf".to_owned(), HEAD_BYTES),
            ("flux.gguf".to_owned(), SNIFF_HEAD_BYTES)
        ]
    );
}

#[tokio::test]
async fn a_whole_header_is_read_in_steps_until_one_holds_it() {
    let hub = Hub::new(&[("chat.gguf", CHAT)]);
    let parser = Parser(&hub);
    let reader = variants(&hub, &parser, &["chat.gguf"]);

    let header = reader.header(0).await.unwrap();

    assert_eq!(header.file_type, Some(Q8_0));
    let sizes: Vec<u64> = hub.reads().into_iter().map(|(_, size)| size).collect();
    assert_eq!(sizes, [MIB, 8 * MIB, 32 * MIB]);
}

#[tokio::test]
async fn a_file_shorter_than_a_read_is_read_once() {
    let small = File {
        len: 300_000,
        metadata_ends: 400_000,
        image_table_ends: None,
        sidecar: false,
    };
    let hub = Hub::new(&[("small.gguf", small)]);
    let parser = Parser(&hub);
    let reader = variants(&hub, &parser, &["small.gguf"]);

    reader.header(0).await.unwrap();

    assert_eq!(hub.reads(), [("small.gguf".to_owned(), MIB)]);
}

#[tokio::test]
async fn a_file_that_cannot_be_read_has_no_head_and_no_header() {
    let hub = Hub::new(&[]);
    let parser = Parser(&hub);
    let reader = variants(&hub, &parser, &["gone.gguf"]);

    assert!(reader.head(0).await.is_none());
    assert!(reader.header(0).await.is_none());
    assert!(reader.head(1).await.is_none());
}

/// Fits whatever is at most its bytes, once the trained context is read.
struct AtMost(u64);

impl FitJudge for AtMost {
    fn fit(&self, metadata: &GgufMetadata, _image: bool, weights_bytes: u64) -> Fit {
        match metadata.context_length {
            None => Fit::CannotTell,
            Some(_) if weights_bytes <= self.0 => Fit::Fits,
            Some(_) => Fit::DoesNotFit,
        }
    }
    fn weights_fit(&self, weights_bytes: u64) -> bool {
        weights_bytes <= self.0
    }
}

/// The `FastMTP-32K` draft `HauhauCS` ships writes its tokenizer first: the
/// keys that make it a draft are 4.8 MB in, so its head settles nothing and
/// the listing takes it for a model. Its whole header is read before it is
/// taken, whether it fits or is the smallest, and it never is.
#[tokio::test]
async fn a_draft_whose_keys_lie_past_its_head_is_never_taken() {
    const GB: u64 = 1 << 30;
    let file = |len, sidecar| File {
        len,
        metadata_ends: 11 * MIB,
        image_table_ends: None,
        sidecar,
    };
    let hub = Hub::new(&[
        ("FastMTP-32K.gguf", file(2 * GB, true)),
        ("Q4_K_P.gguf", file(17 * GB, false)),
        ("Q8_0.gguf", file(28 * GB, false)),
    ]);
    let parser = Parser(&hub);
    let paths = ["FastMTP-32K.gguf", "Q4_K_P.gguf", "Q8_0.gguf"];
    let contenders: Vec<Contender> = [2 * GB, 17 * GB, 28 * GB]
        .into_iter()
        .map(|size| Contender { size, role: None })
        .collect();

    // The draft alone fits, then nothing does: either way the smallest
    // model is taken.
    for limit in [5 * GB, GB] {
        let reader = variants(&hub, &parser, &paths);
        let picked = pick_variant(&contenders, &reader, Some(&AtMost(limit)))
            .await
            .unwrap();
        assert_eq!(
            picked,
            Pick {
                index: 1,
                reason: PickReason::NoneFits
            },
            "at {limit}"
        );
    }
}
