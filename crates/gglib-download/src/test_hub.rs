//! A repository held in memory as its list of GGUF files, for tests of what
//! is resolved and queued from it.

use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use gglib_core::domain::{ImageFamily, TensorInfo, TensorTable, WeightsFormat};
use gglib_core::download::GgufFileRole;
use gglib_core::ports::huggingface::fake_hub::{PROJECTOR_HEAD, RoleParser};
use gglib_core::ports::huggingface::{
    HfClientPort, HfFileInfo, HfPortError, HfPortResult, HfQuantInfo, HfRepoInfo, HfSearchOptions,
    HfSearchResult, match_variant, variants_of,
};
use gglib_core::ports::{
    GgufCapabilities, GgufMetadata, GgufParseError, GgufParserPort, HeadMetadata,
};

/// One repository, whose files are grouped and labelled into variants as
/// the Hub's listing is. A variant's role, which the Hub's header would
/// give, is read from its first file's name by [`GgufFileRole::classify`];
/// a file's OID is `oid-<path>`.
///
/// An image repository's weights also have a head, which
/// [`ImageHeadParser`] reads as the family's tensor table, and its family's
/// companions are listed in their own repositories, each at `size` bytes.
/// Every repository id asked answers the same files.
pub(crate) struct RepoHub {
    files: Vec<HfFileInfo>,
    /// The head every weights file has, when the repository is an image
    /// model's.
    head: Option<Vec<u8>>,
    /// Files of other repositories, looked up by path.
    elsewhere: Vec<(String, HfFileInfo)>,
    /// Each head read asked of it, as repository and path, oldest first.
    pub(crate) heads_asked: Mutex<Vec<(String, String)>>,
}

impl RepoHub {
    /// A repository holding `files`, each a path and a size.
    pub(crate) fn new(files: &[(&str, u64)]) -> Self {
        let files = files
            .iter()
            .map(|(path, size)| HfFileInfo {
                path: (*path).to_string(),
                size: *size,
                is_gguf: true,
                oid: Some(format!("oid-{path}")),
            })
            .collect();
        Self {
            files,
            head: None,
            elsewhere: Vec::new(),
            heads_asked: Mutex::new(Vec::new()),
        }
    }

    /// A repository of `family`'s weights `files`, whose companions are
    /// listed in their own repositories at `companion_size` bytes each.
    pub(crate) fn image(family: ImageFamily, files: &[(&str, u64)], companion_size: u64) -> Self {
        let elsewhere = family
            .recipe()
            .components
            .iter()
            .map(|spec| {
                let file = HfFileInfo {
                    path: spec.path.to_string(),
                    size: companion_size,
                    is_gguf: Path::new(spec.path)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf")),
                    oid: Some(format!("oid-{}", spec.path)),
                };
                (spec.repo.to_string(), file)
            })
            .collect();
        Self {
            head: Some(family.as_str().as_bytes().to_vec()),
            elsewhere,
            ..Self::new(files)
        }
    }

    /// The repository's variants, each with the role its first file's name
    /// gives.
    fn variants(&self) -> Vec<HfQuantInfo> {
        let mut variants = variants_of(self.files.iter().map(|f| (f.path.as_str(), f.size)));
        for variant in &mut variants {
            variant.role = variant
                .file_paths
                .first()
                .map(|first| GgufFileRole::classify(Path::new(first)));
        }
        variants
    }

    /// The files of one role, by path.
    fn of_role(&self, role: GgufFileRole) -> Vec<HfFileInfo> {
        let mut files: Vec<HfFileInfo> = self
            .files
            .iter()
            .filter(|f| GgufFileRole::classify(Path::new(&f.path)) == role)
            .cloned()
            .collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        files
    }
}

#[async_trait]
impl HfClientPort for RepoHub {
    async fn list_quantizations(&self, _model_id: &str) -> HfPortResult<Vec<HfQuantInfo>> {
        Ok(self.variants())
    }

    async fn get_quantization_files(
        &self,
        model_id: &str,
        quantization: &str,
    ) -> HfPortResult<Vec<HfFileInfo>> {
        let variants = self.variants();
        let chosen = match_variant(&variants, quantization).ok();
        let Some(variant) = chosen else {
            return Err(HfPortError::QuantizationNotFound {
                model_id: model_id.to_string(),
                quantization: quantization.to_string(),
            });
        };
        Ok(variant
            .file_paths
            .iter()
            .filter_map(|path| self.files.iter().find(|f| &f.path == path).cloned())
            .collect())
    }

    async fn search(&self, _options: &HfSearchOptions) -> HfPortResult<HfSearchResult> {
        unimplemented!("a file list is all this hub answers")
    }
    async fn list_gguf_files(&self, _model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        Ok(self.files.clone())
    }
    async fn get_commit_sha(&self, _model_id: &str) -> HfPortResult<String> {
        unimplemented!("a file list is all this hub answers")
    }
    /// The repository's card is not held: a registration goes on without
    /// its tags.
    async fn get_model_info(&self, model_id: &str) -> HfPortResult<HfRepoInfo> {
        Err(HfPortError::ModelNotFound {
            model_id: model_id.to_string(),
        })
    }
    /// An image repository's head, for any of its files; any other file's
    /// cannot be read.
    async fn read_head(
        &self,
        model_id: &str,
        path: &str,
        _max_bytes: u64,
    ) -> HfPortResult<Vec<u8>> {
        let mut asked = self.heads_asked.lock().unwrap();
        asked.push((model_id.to_string(), path.to_string()));
        drop(asked);
        if self
            .of_role(GgufFileRole::Projector)
            .iter()
            .any(|p| p.path == path)
        {
            return Ok(PROJECTOR_HEAD.to_vec());
        }
        let known = self.files.iter().any(|file| file.path == path);
        self.head
            .clone()
            .filter(|_| known)
            .ok_or_else(|| HfPortError::FileNotFound {
                model_id: model_id.to_string(),
                path: path.to_string(),
            })
    }
    async fn file_at(&self, model_id: &str, path: &str) -> HfPortResult<Option<HfFileInfo>> {
        Ok(self
            .elsewhere
            .iter()
            .find(|(repo, file)| repo == model_id && file.path == path)
            .map(|(_, file)| file.clone()))
    }
}

/// Reads a head that names a family, as [`RepoHub::image`] gives one, as
/// the smallest tensor table that family is sniffed from, and any other head
/// as a chat model's, whose metadata runs past it. It reads no file.
pub(crate) struct ImageHeadParser;

/// A tensor of `shape`, outermost first.
fn tensor(name: &str, shape: &[u64]) -> TensorInfo {
    TensorInfo {
        name: name.to_string(),
        shape: shape.to_vec(),
    }
}

/// The smallest table `family` is sniffed from.
fn table_of(family: ImageFamily) -> TensorTable {
    let tensors = match family {
        ImageFamily::Flux1 => vec![
            tensor("double_blocks.0.img_attn.qkv.weight", &[9216, 3072]),
            tensor("single_blocks.0.linear1.weight", &[21504, 3072]),
            tensor("img_in.weight", &[3072, 64]),
            tensor("txt_in.weight", &[3072, 4096]),
        ],
        ImageFamily::Sdxl => vec![
            tensor(
                "model.diffusion_model.input_blocks.0.0.weight",
                &[320, 4, 3, 3],
            ),
            tensor("conditioner.embedders.1.model.ln_final.weight", &[1280]),
            tensor("model.diffusion_model.middle_block.1.norm.weight", &[1280]),
            tensor(
                "model.diffusion_model.output_blocks.3.1.transformer_blocks.1.attn1.to_k.weight",
                &[1280, 1280],
            ),
        ],
        ImageFamily::QwenImage21 => vec![
            tensor("txt_in.text_norm.weight", &[4096]),
            tensor("img_in.weight", &[4096, 64]),
        ],
        ImageFamily::ZImage => vec![
            tensor("cap_embedder.0.weight", &[2560]),
            tensor("cap_embedder.1.weight", &[3840, 2560]),
            tensor("x_embedder.weight", &[3840, 64]),
        ],
        ImageFamily::Flux3Klein => vec![
            tensor("emb_in.image.weight", &[3072, 128]),
            tensor("txt_in.weight", &[3072, 20480]),
            tensor("txt_mode_blocks.0.q_proj.weight", &[3072, 3072]),
        ],
        ImageFamily::Flux2Klein => vec![
            tensor("double_stream_modulation_img.lin.weight", &[18432, 3072]),
            tensor("img_in.weight", &[3072, 128]),
            tensor("txt_in.weight", &[3072, 7680]),
        ],
    };
    TensorTable {
        format: WeightsFormat::Gguf,
        architecture: None,
        tensors,
    }
}

impl GgufParserPort for ImageHeadParser {
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
        let named = std::str::from_utf8(head)
            .ok()
            .and_then(|name| name.parse().ok());
        let table = named.map(table_of).ok_or_else(|| {
            GgufParseError::Io("failed to fill whole buffer: the head ended".to_string())
        })?;
        debug_assert_eq!(ImageFamily::sniff(&table), named);
        Ok(table)
    }

    /// A projector's head is its whole header, as [`RoleParser`] reads it;
    /// a head that names a family holds an image model's whole header, which
    /// states no type; any other is no GGUF header.
    fn parse_head(&self, head: &[u8], file_name: &str) -> Result<HeadMetadata, GgufParseError> {
        if head == PROJECTOR_HEAD {
            return RoleParser.parse_head(head, file_name);
        }
        self.tensor_table_of_head(head)?;
        Ok(HeadMetadata {
            complete: true,
            role: Some(GgufFileRole::Weights),
            ..HeadMetadata::default()
        })
    }
}
