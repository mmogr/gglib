//! A folder put on, and taken off, the user's own `PATH`.
//!
//! The desktop app's Windows installer puts the folder it installs to,
//! which holds `gglib.exe`, on this user's `PATH`, so `gglib` runs in a
//! terminal opened afterwards, and its uninstaller takes it off again (#1959).
//! Both run `gglib config path-entry`, which calls [`add`] and [`remove`]
//! with the folder of the running `gglib.exe`. gglib edits the variable, not
//! the installer: an NSIS string holds 1024 characters in the build Tauri
//! uses, and a longer `PATH` read into one and written back would be cut
//! short.
//!
//! # Only what gglib added is taken off
//!
//! The folder may be on `PATH` already: the user can install into a folder
//! of their own tools, or have put it there by hand. Then [`add`] changes
//! nothing, and [`remove`] must not take it off either. So [`add`] records
//! each folder it puts on `PATH`, and [`remove`] takes a folder off only
//! when the record names it, then drops it from the record whether or not
//! the entry was still there. The record is a list, so an install moved to
//! another folder does not lose the first folder's entry.
//!
//! The writes are ordered so that a crash between two of them can never
//! make a later [`remove`] take off an entry gglib did not add, except one
//! the user typed in that same moment. [`add`] writes the record first,
//! then `PATH`: a crash between leaves a record naming a folder not on
//! `PATH`, which the next install completes and the next uninstall clears,
//! taking nothing off. The other order would leave an entry with no record,
//! which no uninstall would ever take off. [`remove`] writes `PATH` first,
//! then the record, for the same reason: a crash between leaves a record
//! with no entry, cleared by the next uninstall. A failed `PATH` write in
//! [`add`] puts the record back as it was.
//!
//! # The text
//!
//! [`with_folder`] and [`without_folder`] are the edit, as functions of the
//! variable's text, so they are tested on every OS. Entries are separated
//! by `;` outside double quotes: a `;` inside `"…"` is part of the entry,
//! and an unclosed quote runs to the end, so the rest is one entry that is
//! never cut. An entry names the folder when the two are the same text once
//! surrounding quotes and trailing separators are dropped, `/` is read as
//! `\` and case is ignored, as Windows ignores it. Every other entry is kept
//! as it is written: quotes, `%VAR%` references and empty entries included.
//! An entry that reaches the folder through a variable,
//! `%LOCALAPPDATA%\gglib-gui`, is another entry: what it names is decided
//! when a program starts.
//!
//! The registry is read and written by [`crate::utils::process::host`], the
//! one file that calls Windows, through a `Store`, so the order of reads
//! and writes is tested on every OS against a stand-in. macOS and Linux
//! keep no such variable, so there both say [`Edit::NotKept`] and change
//! nothing.

use std::io;
use std::path::Path;

use crate::utils::process::host::Registry;

/// The user's own `PATH`, as it is read from where it is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserPath {
    /// This OS keeps no such variable: a shell's startup files set `PATH` on
    /// macOS and Linux.
    NotKept,
    /// Windows keeps one, and this user has no value.
    Absent,
    /// The value as it is stored, every `%VAR%` in it unexpanded.
    Text {
        /// The entries, separated by `;`.
        text: String,
        /// Whether it is stored as `REG_EXPAND_SZ`, whose `%VAR%` references
        /// Windows expands for each new program; otherwise `REG_SZ`.
        expandable: bool,
    },
}

/// What [`add`] or [`remove`] did to the user's `PATH`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    /// The folder was added, or taken off, and running programs were told.
    Changed,
    /// It was already there, or already not there: `PATH` was not written.
    Unchanged,
    /// [`remove`] only: it is on `PATH`, but gglib did not put it there, so
    /// it stays.
    NotOurs,
    /// This OS keeps no `PATH` of the user's own to edit.
    NotKept,
}

/// Where the user's own `PATH` and gglib's record of what it added are
/// kept: the registry, or a stand-in in tests.
pub(crate) trait Store {
    /// The user's own `PATH`.
    fn path(&mut self) -> io::Result<UserPath>;
    /// Store `text` as the user's own `PATH`, with its type.
    fn set_path(&mut self, text: &str, expandable: bool) -> io::Result<()>;
    /// The folders gglib added, separated by `;`; empty when none.
    fn record(&mut self) -> io::Result<String>;
    /// Store the record; empty removes it.
    fn set_record(&mut self, text: &str) -> io::Result<()>;
}

/// Put `folder` on the user's own `PATH`, as its last entry, unless an entry
/// already names it, and record that gglib put it there.
///
/// # Errors
///
/// A folder that cannot be one entry (empty, not Unicode, or holding a `;`
/// or a `"`), or the error reading or writing the variable or the record
/// gave.
pub fn add(folder: &Path) -> io::Result<Edit> {
    add_in(&mut Registry, folder)
}

/// Take `folder` off the user's own `PATH` if gglib put it there, and drop
/// it from the record. Every entry naming it goes; a `PATH` left with
/// nothing in it is stored empty.
///
/// # Errors
///
/// As [`add`].
pub fn remove(folder: &Path) -> io::Result<Edit> {
    remove_in(&mut Registry, folder)
}

/// The user's `PATH` as text and type, or `None` when this OS keeps none.
fn read_path(store: &mut impl Store) -> io::Result<Option<(String, bool)>> {
    Ok(match store.path()? {
        UserPath::NotKept => None,
        // A new value is REG_EXPAND_SZ, as Windows makes the one it gives a
        // new user.
        UserPath::Absent => Some((String::new(), true)),
        UserPath::Text { text, expandable } => Some((text, expandable)),
    })
}

pub(crate) fn add_in(store: &mut impl Store, folder: &Path) -> io::Result<Edit> {
    let folder = entry(folder)?;
    let Some((text, expandable)) = read_path(store)? else {
        return Ok(Edit::NotKept);
    };
    let Some(added) = with_folder(&text, folder) else {
        return Ok(Edit::Unchanged);
    };
    let record = store.record()?;
    // The record first: see the module's notes on order.
    let recorded = with_folder(&record, folder);
    if let Some(recorded) = &recorded {
        store.set_record(recorded)?;
    }
    if let Err(error) = store.set_path(&added, expandable) {
        if recorded.is_some() {
            // Best effort: the error that matters is the one returned.
            let _ = store.set_record(&record);
        }
        return Err(error);
    }
    Ok(Edit::Changed)
}

pub(crate) fn remove_in(store: &mut impl Store, folder: &Path) -> io::Result<Edit> {
    let folder = entry(folder)?;
    let Some((text, expandable)) = read_path(store)? else {
        return Ok(Edit::NotKept);
    };
    let taken = without_folder(&text, folder);
    let Some(rest) = without_folder(&store.record()?, folder) else {
        return Ok(if taken.is_some() {
            Edit::NotOurs
        } else {
            Edit::Unchanged
        });
    };
    // PATH first: see the module's notes on order.
    if let Some(taken) = &taken {
        store.set_path(taken, expandable)?;
    }
    store.set_record(&rest)?;
    Ok(if taken.is_some() {
        Edit::Changed
    } else {
        Edit::Unchanged
    })
}

/// `folder` as the text of one entry.
fn entry(folder: &Path) -> io::Result<&str> {
    let refuse = |why: &str| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} cannot be one entry of PATH: {why}", folder.display()),
        )
    };
    let text = folder
        .to_str()
        .ok_or_else(|| refuse("its name is not Unicode"))?;
    if text.is_empty() {
        return Err(refuse("it is empty"));
    }
    if text.contains(SEPARATOR) {
        return Err(refuse("its name holds a `;`, which separates entries"));
    }
    if text.contains(QUOTE) {
        return Err(refuse("its name holds a `\"`, which quotes entries"));
    }
    Ok(text)
}

/// What separates the entries of a Windows `PATH`, and what quotes one.
const SEPARATOR: char = ';';
const QUOTE: char = '"';

/// The entries of `path`, each its exact text: split at each `;` outside
/// double quotes. Joined with `;` they give back `path`.
fn entries(path: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    for (at, character) in path.char_indices() {
        if character == QUOTE {
            quoted = !quoted;
        } else if character == SEPARATOR && !quoted {
            entries.push(&path[start..at]);
            start = at + SEPARATOR.len_utf8();
        }
    }
    entries.push(&path[start..]);
    entries
}

/// Whether `path` ends inside a quote that is never closed.
fn ends_quoted(path: &str) -> bool {
    path.matches(QUOTE).count() % 2 == 1
}

/// An entry as it is compared: without surrounding quotes or trailing
/// separators, with `/` read as `\`, in lower case.
fn compared(entry: &str) -> String {
    let unquoted = entry
        .strip_prefix(QUOTE)
        .and_then(|inner| inner.strip_suffix(QUOTE))
        .unwrap_or(entry);
    unquoted
        .trim_end_matches(['\\', '/'])
        .replace('/', "\\")
        .to_lowercase()
}

/// `path`, a `PATH` value in Windows' form, with `folder` added as its last
/// entry, or `None` when an entry already names `folder`.
///
/// A `path` that ends in `;` keeps that ending after the folder, and one
/// that ends inside an unclosed quote, where an entry added at the end would
/// be read as part of the quoted one, gets the folder as its first entry
/// instead. Either way [`without_folder`] gives back exactly the `path` this
/// was given.
#[must_use]
pub fn with_folder(path: &str, folder: &str) -> Option<String> {
    let wanted = compared(folder);
    if entries(path).iter().any(|entry| compared(entry) == wanted) {
        return None;
    }
    Some(if path.is_empty() {
        folder.to_owned()
    } else if ends_quoted(path) {
        format!("{folder}{SEPARATOR}{path}")
    } else if path.ends_with(SEPARATOR) {
        format!("{path}{folder}{SEPARATOR}")
    } else {
        format!("{path}{SEPARATOR}{folder}")
    })
}

/// `path` with every entry that names `folder` taken out, or `None` when no
/// entry does. Every other entry, and every separator between them, is kept
/// as it is written.
#[must_use]
pub fn without_folder(path: &str, folder: &str) -> Option<String> {
    let wanted = compared(folder);
    let entries = entries(path);
    let kept: Vec<&str> = entries
        .iter()
        .copied()
        .filter(|entry| compared(entry) != wanted)
        .collect();
    (kept.len() < entries.len()).then(|| kept.join(";"))
}

#[cfg(test)]
#[path = "user_path_tests.rs"]
mod tests;
