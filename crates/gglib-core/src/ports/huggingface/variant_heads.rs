//! What a repository's variants are, read from the head of each one's first
//! file, and the variant a stored model came as.

use futures_util::{StreamExt, stream};

use super::client::HfClientPort;
use super::error::HfPortResult;
use super::types::HfQuantInfo;
use super::variant_match::match_variant;
use crate::domain::{HeadMetadata, Model};
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

/// `variants` of `model_id`, each with its head read and kept beside it.
///
/// The head of each first file is read by [`variant_head`], four at once,
/// and the role set from it. The head stays beside the variant: a listing
/// judges whether a model fits from its trained context and KV shape, the
/// same read its role came from.
pub async fn read_heads(
    client: &dyn HfClientPort,
    parser: &dyn GgufParserPort,
    model_id: &str,
    variants: Vec<HfQuantInfo>,
) -> Vec<(HfQuantInfo, Option<HeadMetadata>)> {
    stream::iter(variants)
        .map(|mut variant| async move {
            let head = variant_head(client, parser, model_id, &variant).await;
            if variant.role.is_none() {
                variant.role = Some(head.as_ref().and_then(|h| h.role).unwrap_or_default());
            }
            (variant, head)
        })
        .buffered(HEADS_AT_ONCE)
        .collect()
        .await
}

/// `variants` of `model_id`, each with its role read by [`variant_role`],
/// four heads at once.
pub async fn read_roles(
    client: &dyn HfClientPort,
    parser: &dyn GgufParserPort,
    model_id: &str,
    variants: Vec<HfQuantInfo>,
) -> Vec<HfQuantInfo> {
    read_heads(client, parser, model_id, variants)
        .await
        .into_iter()
        .map(|(variant, _)| variant)
        .collect()
}

/// The label a Hub lookup asks `repo_id` for to find `model`'s files again.
///
/// That is its stored label while it names a variant ([`match_variant`]).
/// When it has none, or the listing no longer has it, it is the label of the
/// one variant that holds its file: the longest file path its stored path
/// ends with, folder and name, else one named as its `hf_filename`. `None`
/// when the model has neither a label nor an `hf_filename`, and when no
/// variant, or more than one, holds its file.
///
/// # Errors
///
/// What the client answers when the repository is listed.
pub async fn stored_label(
    client: &dyn HfClientPort,
    repo_id: &str,
    model: &Model,
) -> HfPortResult<Option<String>> {
    if model.variant_label.is_none() && model.hf_filename.is_none() {
        return Ok(None);
    }
    let variants = client.list_quantizations(repo_id).await?;
    if let Some(label) = &model.variant_label
        && match_variant(&variants, label).is_ok()
    {
        return Ok(Some(label.clone()));
    }
    // The variants whose file is the longest end of the stored path: a
    // nested `Q8_0/model.gguf` over a root `model.gguf`.
    let suffix = |variant: &HfQuantInfo| {
        variant
            .file_paths
            .iter()
            .filter(|path| model.file_path.ends_with(path))
            .map(|path| path.split('/').count())
            .max()
    };
    let longest = variants.iter().filter_map(suffix).max();
    let mut found: Vec<&HfQuantInfo> = variants
        .iter()
        .filter(|variant| longest.is_some() && suffix(variant) == longest)
        .collect();
    if found.is_empty()
        && let Some(file_name) = model.hf_filename.as_deref()
    {
        found = variants
            .iter()
            .filter(|variant| {
                variant
                    .file_paths
                    .iter()
                    .any(|path| path.rsplit('/').next() == Some(file_name))
            })
            .collect();
    }
    Ok(match found.as_slice() {
        [one] => Some(one.label.clone()),
        _ => None,
    })
}

#[cfg(test)]
#[path = "variant_heads_tests.rs"]
mod tests;
