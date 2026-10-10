//! What an image model's download brings beside its weights: its family's
//! companions, found before anything is fetched.
//!
//! An image model's GGUF holds no metadata, only its tensor table, and that
//! table ends well inside the first [`SNIFF_HEAD_BYTES`] of the file. So the
//! head of the first weights file is read, its tensor names say the family,
//! and the family's recipe names each companion and where it is fetched
//! from. Each one is looked up on the Hub, so its size and OID are known and
//! a download's total covers it.

use super::client::{HfClientPort, SNIFF_HEAD_BYTES};
use super::error::{HfPortError, HfPortResult};
use super::types::HfFileInfo;
use crate::domain::{ComponentRole, ImageFamily};
use crate::ports::GgufParserPort;

/// One file an image model draws with beside its weights, as the Hub lists
/// it.
#[derive(Debug, Clone)]
pub struct Companion {
    /// The role it plays.
    pub role: ComponentRole,
    /// The repository it is fetched from.
    pub repo: String,
    /// The file there, with its size and OID.
    pub file: HfFileInfo,
}

/// The family of the image model whose first weights file is `first_weights`
/// in `model_id`, and the companions its recipe draws with; `None` for any
/// other model.
///
/// The head of the file is read and its tensor table sniffed. A chat
/// model's metadata runs past the head, and a head that cannot be read at
/// all is logged: either is `None`, so a download of anything that is not an
/// image model goes on as it always has.
///
/// # Errors
///
/// When the head names a family and a companion its recipe names cannot be
/// looked up: [`HfPortError::FileNotFound`] naming the companion's repository
/// and path when the Hub holds no file there, or what the client answered.
pub async fn image_companions(
    client: &dyn HfClientPort,
    parser: &dyn GgufParserPort,
    model_id: &str,
    first_weights: &HfFileInfo,
) -> HfPortResult<Option<(ImageFamily, Vec<Companion>)>> {
    let head = match client
        .read_head(model_id, &first_weights.path, SNIFF_HEAD_BYTES)
        .await
    {
        Ok(head) => head,
        Err(unread) => {
            tracing::warn!(
                model_id,
                path = %first_weights.path,
                error = %unread,
                "the weights' head could not be read; downloading them as a chat model's"
            );
            return Ok(None);
        }
    };
    let Some(family) = parser
        .tensor_table_of_head(&head)
        .ok()
        .and_then(|table| ImageFamily::sniff(&table))
    else {
        return Ok(None);
    };

    let mut companions = Vec::new();
    for spec in family.recipe().components {
        let file = client.file_at(spec.repo, spec.path).await?.ok_or_else(|| {
            HfPortError::FileNotFound {
                model_id: spec.repo.to_owned(),
                path: spec.path.to_owned(),
            }
        })?;
        companions.push(Companion {
            role: spec.role,
            repo: spec.repo.to_owned(),
            file,
        });
    }
    Ok(Some((family, companions)))
}

#[cfg(test)]
#[path = "image_companions_tests.rs"]
mod tests;
