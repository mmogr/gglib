//! The one rule a file passes to be linked to a model: as its projector, or
//! as one of an image model's components.
//!
//! Every path is resolved first, so two spellings of one file are one link.
//! A projector is then checked by its GGUF header alone, exactly as ADR 0015
//! decided ("one header check links it"). A component is checked by its
//! tensor names against the role and family it is offered for, since a VAE
//! or a text encoder has no header that says what it is.

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::domain::{ComponentRole, ImageFamily};
use crate::ports::{CoreError, GgufParserPort, RepositoryError};

/// What a file is offered to a model as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkRole {
    /// The projector the model loads beside its weights (`--mmproj`).
    Projector,
    /// One of an image model's components.
    Component {
        /// The model's family.
        family: ImageFamily,
        /// The role the file is to play.
        role: ComponentRole,
    },
}

impl LinkRole {
    /// The word a refusal names the file by.
    const fn noun(self) -> &'static str {
        match self {
            Self::Projector => "projector",
            Self::Component { .. } => "component",
        }
    }
}

/// Why a file was not linked to a model.
#[derive(Debug, Error)]
pub enum LinkError {
    /// The path does not resolve to a file.
    #[error("no {what} file at {}: {reason}", .path.display())]
    Missing {
        /// The path as the caller gave it.
        path: PathBuf,
        /// What resolving it reported.
        reason: String,
        /// What the file was offered as: `projector` or `component`.
        what: &'static str,
    },

    /// The file's header could not be read.
    #[error("{} is not a readable {formats} file: {reason}", .path.display())]
    Unreadable {
        /// The resolved path.
        path: PathBuf,
        /// What the parser reported.
        reason: String,
        /// The formats the file could have been: `GGUF` for a projector,
        /// `GGUF or safetensors` for a component.
        formats: &'static str,
    },

    /// The file is a GGUF, and its header says it holds a model's weights.
    #[error(
        "{} holds a model's weights, not a projector (a projector's header says general.type = mmproj)",
        .0.display()
    )]
    Weights(PathBuf),

    /// The file's tensors are not what the role needs.
    #[error("{} is not a {role} for this model: expected {expected}", .path.display())]
    WrongRole {
        /// The resolved path.
        path: PathBuf,
        /// The role it was offered for.
        role: ComponentRole,
        /// What the role's file must hold.
        expected: &'static str,
    },

    /// The model draws no images, so it takes no components.
    #[error("{0} is not an image model, so it takes no components")]
    NotAnImageModel(String),

    /// The model's family draws without a file in this role.
    #[error("the {} recipe has no {role} component", .family.label())]
    NotInRecipe {
        /// The model's family.
        family: ImageFamily,
        /// The role asked for.
        role: ComponentRole,
    },

    /// The model could not be read or written.
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl From<LinkError> for CoreError {
    fn from(error: LinkError) -> Self {
        match error {
            LinkError::Repository(repository) => Self::Repository(repository),
            refused => Self::Validation(refused.to_string()),
        }
    }
}

/// The canonical path of `path`, once it has passed the check for `role`.
///
/// # Errors
///
/// [`LinkError::Missing`] when the path resolves to no file; for a projector,
/// [`LinkError::Unreadable`] when its header is not GGUF and
/// [`LinkError::Weights`] when the header says it is a model; for a
/// component, [`LinkError::NotInRecipe`] when the family draws without the
/// role, [`LinkError::Unreadable`] when its tensor table cannot be read, and
/// [`LinkError::WrongRole`] when its tensors are not the role's.
pub(super) fn checked_link(
    path: &Path,
    role: LinkRole,
    gguf_parser: &dyn GgufParserPort,
) -> Result<PathBuf, LinkError> {
    let resolved = crate::paths::canonical_model_path(path).map_err(|e| LinkError::Missing {
        path: path.to_path_buf(),
        reason: e.to_string(),
        what: role.noun(),
    })?;
    match role {
        LinkRole::Projector => {
            let header = gguf_parser
                .parse(&resolved)
                .map_err(|e| LinkError::Unreadable {
                    path: resolved.clone(),
                    reason: e.to_string(),
                    formats: "GGUF",
                })?;
            if header.role.is_projector() {
                Ok(resolved)
            } else {
                Err(LinkError::Weights(resolved))
            }
        }
        LinkRole::Component { family, role } => {
            if !in_recipe(family, role) {
                return Err(LinkError::NotInRecipe { family, role });
            }
            let table = gguf_parser
                .tensor_table(&resolved)
                .map_err(|e| LinkError::Unreadable {
                    path: resolved.clone(),
                    reason: e.to_string(),
                    formats: "GGUF or safetensors",
                })?;
            match role.fits(family, &table) {
                Ok(()) => Ok(resolved),
                Err(expected) => Err(LinkError::WrongRole {
                    path: resolved,
                    role,
                    expected,
                }),
            }
        }
    }
}

/// Whether `family`'s recipe draws with a file in `role`.
pub(super) fn in_recipe(family: ImageFamily, role: ComponentRole) -> bool {
    family
        .recipe()
        .components
        .iter()
        .any(|spec| spec.role == role)
}

/// The canonical path of `path`, or `path` itself when it resolves to no file.
pub(super) fn resolved_or_literal(path: &Path) -> PathBuf {
    crate::paths::canonical_model_path(path).unwrap_or_else(|_| path.to_path_buf())
}
