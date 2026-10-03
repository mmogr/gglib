//! The two flags of `gglib model update` that link a model to its projector
//! and unlink it.

use std::path::{Path, PathBuf};

use clap::Args;

/// `--projector <PATH>` and `--no-projector`. One or neither: clap refuses
/// both together.
#[derive(Args, Debug, Clone, Default)]
pub struct ProjectorArgs {
    /// Link the model to this projector file (an mmproj GGUF), which gives it
    /// image input. The file's header must say it is a projector.
    #[arg(long, value_name = "PATH", conflicts_with = "no_projector")]
    pub projector: Option<PathBuf>,
    /// Unlink the model's projector; it stops reading images.
    #[arg(long = "no-projector")]
    pub no_projector: bool,
}

/// What the flags ask to do to a model's projector link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectorChange<'a> {
    /// Link the model to the projector at this path.
    Link(&'a Path),
    /// Unlink the model's projector.
    Unlink,
}

impl<'a> ProjectorChange<'a> {
    /// The path the link will name, `None` once unlinked: the argument
    /// `ModelService::set_projector` takes.
    pub(crate) const fn path(self) -> Option<&'a Path> {
        match self {
            Self::Link(path) => Some(path),
            Self::Unlink => None,
        }
    }
}

impl ProjectorArgs {
    /// What the flags ask for, or `None` when neither was passed and the link
    /// is left alone.
    pub(crate) fn change(&self) -> Option<ProjectorChange<'_>> {
        self.projector.as_deref().map_or_else(
            || self.no_projector.then_some(ProjectorChange::Unlink),
            |path| Some(ProjectorChange::Link(path)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_asks_to_link_it() {
        let args = ProjectorArgs {
            projector: Some(PathBuf::from("mmproj-F16.gguf")),
            no_projector: false,
        };
        assert_eq!(
            args.change(),
            Some(ProjectorChange::Link(Path::new("mmproj-F16.gguf")))
        );
    }

    #[test]
    fn no_projector_asks_to_unlink() {
        let args = ProjectorArgs {
            projector: None,
            no_projector: true,
        };
        assert_eq!(args.change(), Some(ProjectorChange::Unlink));
    }

    #[test]
    fn neither_flag_asks_for_nothing() {
        assert_eq!(ProjectorArgs::default().change(), None);
    }
}
