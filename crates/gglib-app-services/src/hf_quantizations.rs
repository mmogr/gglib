//! A repository's quantizations as the `HuggingFace` browser shows them.

use gglib_core::download::Quantization;
use gglib_core::ports::huggingface::projector_fetched_with;
use gglib_core::ports::{HfFileInfo, HfQuantInfo};

use crate::types::{HfImagePreview, HfProjector, HfQuantization, HfQuantizationsResponse};

/// The listing of `model_id`: each of `quants` with its weights' size and
/// shard count, and the one of `projectors` its download fetches; and
/// `image`, what it fetches beside them when it is an image model.
pub(crate) fn quantizations_response(
    model_id: &str,
    quants: Vec<HfQuantInfo>,
    projectors: &[HfFileInfo],
    image: Option<HfImagePreview>,
) -> HfQuantizationsResponse {
    HfQuantizationsResponse {
        image,
        model_id: model_id.to_string(),
        quantizations: quants
            .into_iter()
            .map(|q| HfQuantization {
                file_path: q.file_paths.first().cloned().unwrap_or_default(),
                size_bytes: q.total_size,
                #[allow(clippy::cast_precision_loss, reason = "a size in MiB for display")]
                size_mb: q.total_size as f64 / 1_048_576.0,
                is_sharded: q.shard_count > 1,
                shard_count: (q.shard_count > 1)
                    .then(|| u32::try_from(q.shard_count).unwrap_or(u32::MAX)),
                projector: projector_fetched_with(Quantization::from_filename(&q.name), projectors)
                    .map(|p| HfProjector {
                        file_path: p.path.clone(),
                        size_bytes: p.size,
                    }),
                name: q.name,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quant(name: &str, shard_count: usize, total_size: u64) -> HfQuantInfo {
        HfQuantInfo {
            name: name.to_owned(),
            shard_count,
            total_size,
            file_paths: vec![format!("m-{name}.gguf")],
        }
    }

    fn projector(path: &str, size: u64) -> HfFileInfo {
        HfFileInfo {
            path: path.to_owned(),
            size,
            is_gguf: true,
            oid: None,
        }
    }

    /// Each quantization carries the projector its own download fetches, and
    /// its size stays the weights alone.
    #[test]
    fn each_quantization_names_the_projector_its_download_fetches() {
        let response = quantizations_response(
            "o/r",
            vec![quant("Q4_K_M", 1, 4_000), quant("Q8_0", 2, 8_000)],
            &[
                projector("X.mmproj-Q8_0.gguf", 600),
                projector("mmproj-F16.gguf", 900),
            ],
            None,
        );

        assert_eq!(response.model_id, "o/r");
        let [q4, q8] = response.quantizations.as_slice() else {
            panic!("two quantizations were listed");
        };
        assert_eq!(
            q4.projector,
            Some(HfProjector {
                file_path: "mmproj-F16.gguf".to_owned(),
                size_bytes: 900
            })
        );
        assert_eq!(
            (q4.size_bytes, q4.is_sharded, q4.shard_count),
            (4_000, false, None)
        );
        assert_eq!(
            q8.projector,
            Some(HfProjector {
                file_path: "X.mmproj-Q8_0.gguf".to_owned(),
                size_bytes: 600
            })
        );
        assert_eq!(
            (q8.size_bytes, q8.is_sharded, q8.shard_count),
            (8_000, true, Some(2))
        );
        assert_eq!(q8.file_path, "m-Q8_0.gguf");
    }

    #[test]
    fn a_repository_without_projectors_names_none() {
        let response = quantizations_response("o/r", vec![quant("Q8_0", 1, 8_000)], &[], None);

        assert_eq!(response.quantizations[0].projector, None);
    }

    /// The browser's listing reads the repository's projectors, not the
    /// quantizations alone.
    #[tokio::test]
    async fn the_listing_served_reads_the_repository_projectors() {
        use crate::downloads::{DownloadDeps, DownloadOps};
        use crate::test_support::{MockDownloadManager, MockHfClient, MockToolSupportDetector};
        use std::sync::Arc;

        let ops = DownloadOps::new(DownloadDeps {
            downloads: Arc::new(MockDownloadManager::new()),
            hf: Arc::new(MockHfClient),
            tool_detector: Arc::new(MockToolSupportDetector),
            gguf_parser: Arc::new(gglib_core::ports::NoopGgufParser),
            models_directory: None,
        });

        let listed = ops.get_model_quantizations("o/r").await.unwrap();

        assert_eq!(
            listed.quantizations[0].projector,
            Some(HfProjector {
                file_path: "mmproj-F16.gguf".to_owned(),
                size_bytes: 7
            })
        );
    }
}
