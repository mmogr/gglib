//! A repository's variants as the `HuggingFace` browser shows them.

use gglib_core::download::GgufFileRole;
use gglib_core::ports::HfQuantInfo;
use gglib_core::ports::huggingface::largest_projector;

use crate::types::{HfImagePreview, HfProjector, HfQuantization, HfQuantizationsResponse};

/// The listing of `model_id`: each of `quants`, its role read, with its
/// label, its files' size and shard count, and for a model the projector
/// its download fetches, the largest of `quants` whose header says it is
/// one; and `image`, what it fetches beside them when it is an image model.
pub(crate) fn quantizations_response(
    model_id: &str,
    quants: Vec<HfQuantInfo>,
    image: Option<HfImagePreview>,
) -> HfQuantizationsResponse {
    let projector = largest_projector(&quants).map(|p| HfProjector {
        file_path: p.file_paths.first().cloned().unwrap_or_default(),
        size_bytes: p.total_size,
    });
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
                is_sharded: q.is_sharded(),
                shard_count: q
                    .is_sharded()
                    .then(|| u32::try_from(q.shard_count()).unwrap_or(u32::MAX)),
                projector: q
                    .role
                    .is_none_or(GgufFileRole::is_weights)
                    .then(|| projector.clone())
                    .flatten(),
                role: q.role.unwrap_or_default(),
                label: q.label,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quant(label: &str, shard_count: usize, total_size: u64) -> HfQuantInfo {
        HfQuantInfo {
            label: label.to_owned(),
            role: Some(GgufFileRole::Weights),
            total_size,
            file_paths: (0..shard_count)
                .map(|i| format!("m-{label}-{i}.gguf"))
                .collect(),
        }
    }

    fn projector(path: &str, total_size: u64) -> HfQuantInfo {
        HfQuantInfo {
            label: path.trim_end_matches(".gguf").to_owned(),
            role: Some(GgufFileRole::Projector),
            total_size,
            file_paths: vec![path.to_owned()],
        }
    }

    /// Each model carries the projector every download fetches, the largest
    /// whose header says so, and its size stays the weights alone.
    #[test]
    fn each_model_names_the_largest_projector() {
        let response = quantizations_response(
            "o/r",
            vec![
                quant("Q4_K_M", 1, 4_000),
                quant("Q8_0", 2, 8_000),
                projector("X.mmproj-Q8_0.gguf", 600),
                projector("mmproj-F16.gguf", 900),
            ],
            None,
        );

        assert_eq!(response.model_id, "o/r");
        let [q4, q8, small, large] = response.quantizations.as_slice() else {
            panic!("four variants were listed");
        };
        let fetched = Some(HfProjector {
            file_path: "mmproj-F16.gguf".to_owned(),
            size_bytes: 900,
        });
        assert_eq!(q4.projector, fetched);
        assert_eq!(q8.projector, fetched);
        assert_eq!(
            (small.projector.clone(), large.projector.clone()),
            (None, None)
        );
        assert_eq!(
            (q4.size_bytes, q4.is_sharded, q4.shard_count),
            (4_000, false, None)
        );
        assert_eq!(
            (q8.size_bytes, q8.is_sharded, q8.shard_count),
            (8_000, true, Some(2))
        );
        assert_eq!(q8.file_path, "m-Q8_0-0.gguf");
    }

    #[test]
    fn a_repository_without_projectors_names_none() {
        let response = quantizations_response("o/r", vec![quant("Q8_0", 1, 8_000)], None);

        assert_eq!(response.quantizations[0].projector, None);
    }

    /// The browser's listing finds the projector among the variants by the
    /// head of each.
    #[tokio::test]
    async fn the_listing_served_reads_the_repository_projectors() {
        use crate::downloads::{DownloadDeps, DownloadOps};
        use crate::test_support::{MockDownloadManager, MockHfClient, MockToolSupportDetector};
        use std::sync::Arc;

        let ops = DownloadOps::new(DownloadDeps {
            downloads: Arc::new(MockDownloadManager::new()),
            hf: Arc::new(MockHfClient),
            tool_detector: Arc::new(MockToolSupportDetector),
            gguf_parser: Arc::new(gglib_core::ports::huggingface::fake_hub::RoleParser),
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
