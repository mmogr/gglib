//! The two flags of `gglib model add` and `gglib model update` that link an
//! image model's components and unlink them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr as _;

use anyhow::{Result, bail};
use clap::Args;
use gglib_core::domain::ComponentRole;

/// `--component <ROLE>=<PATH>` and `--no-component <ROLE>`, each repeatable.
#[derive(Args, Debug, Clone, Default)]
pub struct ComponentArgs {
    /// Link one of an image model's components, as `<ROLE>=<PATH>`, the role
    /// one of `vae`, `clip_l`, `t5xxl` or `llm`. Repeatable. The file's
    /// tensors must be that role's for the model's family.
    #[arg(
        long = "component",
        value_name = "ROLE=PATH",
        value_parser = parse_component,
        action = clap::ArgAction::Append
    )]
    pub component: Vec<(ComponentRole, PathBuf)>,
    /// Unlink one of an image model's components, by role. Repeatable.
    #[arg(
        long = "no-component",
        value_name = "ROLE",
        value_parser = parse_role,
        action = clap::ArgAction::Append
    )]
    pub no_component: Vec<ComponentRole>,
}

/// What the flags ask to do to one role: link it to a file, or unlink it.
pub(crate) type ComponentChanges<'a> = BTreeMap<ComponentRole, Option<&'a Path>>;

impl ComponentArgs {
    /// What the flags ask for, by role: the path to link, or `None` to
    /// unlink. Empty when neither flag was passed.
    ///
    /// # Errors
    ///
    /// When one role is named twice, in either flag or both: that says two
    /// things about one link.
    pub(crate) fn changes(&self) -> Result<ComponentChanges<'_>> {
        let mut changes = BTreeMap::new();
        let asked = (self.component.iter())
            .map(|(role, path)| (*role, Some(path.as_path())))
            .chain(self.no_component.iter().map(|role| (*role, None)));
        for (role, change) in asked {
            if changes.insert(role, change).is_some() {
                bail!("the component {role} is named twice; name each role once");
            }
        }
        Ok(changes)
    }
}

/// The request's `components` for `changes`: each path as a string, an
/// unlink as `null`, and no field at all when there is nothing to change.
pub(crate) fn request_components(
    changes: &ComponentChanges<'_>,
) -> Option<BTreeMap<ComponentRole, Option<String>>> {
    (!changes.is_empty()).then(|| {
        changes
            .iter()
            .map(|(role, path)| {
                let path = path.map(|path| path.to_string_lossy().into_owned());
                (*role, path)
            })
            .collect()
    })
}

/// `<ROLE>=<PATH>`, the role by its name.
fn parse_component(value: &str) -> Result<(ComponentRole, PathBuf), String> {
    let Some((role, path)) = value.split_once('=') else {
        return Err("expected <ROLE>=<PATH>, such as vae=ae.safetensors".to_owned());
    };
    if path.is_empty() {
        return Err(format!("no path after {role}="));
    }
    Ok((parse_role(role)?, PathBuf::from(path)))
}

/// A role by its name, refused with the names there are.
fn parse_role(value: &str) -> Result<ComponentRole, String> {
    ComponentRole::from_str(value).map_err(|unknown| unknown.to_string())
}

#[cfg(test)]
#[path = "component_args_tests.rs"]
mod tests;
