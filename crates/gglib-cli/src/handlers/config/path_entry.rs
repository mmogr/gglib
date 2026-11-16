//! `gglib config path-entry add|remove`, hidden from `--help`: what the
//! desktop app's Windows installer runs to put its folder on the user's own
//! `PATH`, and its uninstaller to take it off again if the install put it
//! there (`src-tauri/windows/installer-hooks.nsh`).
//!
//! The folder is the one this `gglib` runs from, so the hook passes no path
//! and a folder name with a space or a trailing backslash cannot be misread
//! on the way. It runs before logging is set up
//! ([`crate::dispatch::leaves_nothing_behind`]), so it touches nothing but
//! `PATH` and gglib's record of it. What it prints, one line either way, is
//! what the installer's log shows.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use gglib_core::paths::user_path::{self, Edit};

use crate::config_commands::PathEntryCommand;

/// Add or remove the folder holding this executable.
///
/// # Errors
///
/// One sentence, with no chain of causes under it, saying what could not
/// be done and why; the installer logs it and finishes regardless.
pub(crate) fn execute(command: PathEntryCommand) -> Result<()> {
    let folder = own_folder()?;
    let edit = match command {
        PathEntryCommand::Add => user_path::add(&folder),
        PathEntryCommand::Remove => user_path::remove(&folder),
    }
    .map_err(|error| {
        let folder = folder.display();
        match command {
            PathEntryCommand::Add => anyhow!("could not put {folder} on your PATH: {error}"),
            PathEntryCommand::Remove => {
                anyhow!("could not take {folder} off your PATH: {error}")
            }
        }
    })?;
    println!("{}", sentence(command, edit, &folder));
    Ok(())
}

/// The folder this executable is in.
fn own_folder() -> Result<PathBuf> {
    let exe = std::env::current_exe()
        .map_err(|error| anyhow!("could not find the folder this gglib runs from: {error}"))?;
    exe.parent().map(Path::to_path_buf).ok_or_else(|| {
        anyhow!(
            "could not find the folder this gglib runs from: {} is in none",
            exe.display()
        )
    })
}

/// What the edit did, in words.
fn sentence(command: PathEntryCommand, edit: Edit, folder: &Path) -> String {
    let folder = folder.display();
    match (command, edit) {
        (PathEntryCommand::Add, Edit::Changed) => {
            format!("Added {folder} to your PATH. A terminal opened from now on runs gglib.")
        }
        (PathEntryCommand::Add, Edit::Unchanged | Edit::NotOurs) => {
            format!("{folder} is already on your PATH.")
        }
        (PathEntryCommand::Remove, Edit::Changed) => format!("Took {folder} off your PATH."),
        (PathEntryCommand::Remove, Edit::Unchanged) => format!("{folder} was not on your PATH."),
        (PathEntryCommand::Remove, Edit::NotOurs) => {
            format!("{folder} stays on your PATH: gglib did not put it there.")
        }
        (_, Edit::NotKept) => {
            "This system keeps no PATH of your own for gglib to edit; nothing changed.".to_owned()
        }
    }
}
