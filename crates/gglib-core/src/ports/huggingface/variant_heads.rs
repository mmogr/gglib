//! What a repository's variants are, read from the head of each one's first
//! file.

use futures_util::{StreamExt, stream};

use super::client::HfClientPort;
use super::types::HfQuantInfo;
use crate::domain::HeadMetadata;
use crate::download::GgufFileRole;
use crate::ports::GgufParserPort;

/// How much of a variant's first file is read to learn what it is: 64 KiB.
///
/// A converter writes the keys that settle a file's role (`general.type`,
/// the layer counts, a draft's target layers) near the start: read on the
/// Hub, they end within the first 2 KB of every file but one.
pub const VARIANT_HEAD_BYTES: u64 = 64 * 1024;

/// How many variants' heads are read at once.
const HEADS_AT_ONCE: usize = 4;

/// The head of `variant`'s first file in `model_id`, read as far as
/// [`VARIANT_HEAD_BYTES`]; `None` when it cannot be read or is not a GGUF
/// header.
pub async fn variant_head(
    client: &dyn HfClientPort,
    parser: &dyn GgufParserPort,
    model_id: &str,
    variant: &HfQuantInfo,
) -> Option<HeadMetadata> {
    let first = variant.file_paths.first()?;
    let head = match client.read_head(model_id, first, VARIANT_HEAD_BYTES).await {
        Ok(head) => head,
        Err(unread) => {
            tracing::debug!(model_id, path = %first, error = %unread, "a variant's head could not be read");
            return None;
        }
    };
    let file_name = first.rsplit('/').next().unwrap_or(first);
    parser.parse_head(&head, file_name).ok()
}

/// What `variant` of `model_id` is: its role when known, else what the head
/// of its first file says.
///
/// The head says a draft or helper ([`GgufFileRole::Sidecar`]) or a
/// projector when it settles it, and a model in every other case. A head
/// that cannot be read is a model's.
pub async fn variant_role(
    client: &dyn HfClientPort,
    parser: &dyn GgufParserPort,
    model_id: &str,
    variant: &HfQuantInfo,
) -> GgufFileRole {
    if let Some(role) = variant.role {
        return role;
    }
    variant_head(client, parser, model_id, variant)
        .await
        .and_then(|head| head.role)
        .unwrap_or_default()
}

/// `variants` of `model_id`, each with its role read by [`variant_role`],
/// four heads at once.
pub async fn read_roles(
    client: &dyn HfClientPort,
    parser: &dyn GgufParserPort,
    model_id: &str,
    variants: Vec<HfQuantInfo>,
) -> Vec<HfQuantInfo> {
    stream::iter(variants)
        .map(|mut variant| async move {
            variant.role = Some(variant_role(client, parser, model_id, &variant).await);
            variant
        })
        .buffered(HEADS_AT_ONCE)
        .collect()
        .await
}

#[cfg(test)]
#[path = "variant_heads_tests.rs"]
mod tests;
