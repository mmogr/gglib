//! A path's one canonical form: resolved, and on Windows written the way
//! every program reads it.

use std::ffi::OsStr;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::host::HostOs;

/// `path` resolved, symlinks and `..` included, as gglib stores, compares and
/// shows it and hands it to an engine.
///
/// On Windows `std::fs::canonicalize` writes a verbatim path (`\\?\C:\...`);
/// this writes it without the prefix whenever the plain path names the same
/// file, and keeps it otherwise. One code path on every OS: elsewhere a
/// resolved path never has the prefix, so it comes back as resolved.
///
/// # Errors
///
/// The [`io::Error`] resolving it gave, most often because nothing is there.
#[expect(
    clippy::disallowed_methods,
    reason = "the one place a path is canonicalized; clippy.toml sends every other caller here"
)]
pub fn canonical(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    std::fs::canonicalize(path).map(without_verbatim_prefix)
}

/// Whether `text` is a path written in the verbatim form, as builds before
/// [`canonical`] stored every path on Windows.
#[must_use]
pub fn is_verbatim(text: &str) -> bool {
    text.starts_with(VERBATIM)
}

/// The canonical form of the entry `name` in the canonical folder `dir`: the
/// path [`canonical`] gives that entry when it is no link.
///
/// Joining a name onto a plain folder is not always exact. Windows reads a
/// plain path before opening it, so `C:\proj` joined with `notes.` would open
/// `C:\proj\notes`, and with `aux.c` the AUX device. Such an entry is given in
/// the verbatim form, where its name is taken as it is, and so is one whose
/// plain path would be longer than `MAX_PATH`. `name` is one name, as a
/// folder listing gives it.
#[must_use]
pub fn child(dir: &Path, name: impl AsRef<OsStr>) -> PathBuf {
    let name = name.as_ref();
    let exact = dir.to_str().zip(name.to_str()).and_then(|(d, n)| {
        let joined = join_text(verbatim_form(d)?, [n]);
        Some(plain_form(&joined).unwrap_or(joined))
    });
    exact.map_or_else(|| dir.join(name), PathBuf::from)
}

/// `dir` joined with the relative path `relative`, naming the file the
/// verbatim join would.
///
/// It is [`Path::join`], unless a name in `relative` reads otherwise in a
/// plain path (see [`child`]); then the result is in the verbatim form, where
/// a `..` is a name like any other, so it leads nowhere rather than out of
/// `dir`.
#[must_use]
pub fn join(dir: &Path, relative: &Path) -> PathBuf {
    exact_join(dir, relative).unwrap_or_else(|| dir.join(relative))
}

/// The part of the canonical `path` under the canonical `folder`, empty when
/// they are one; `None` when it lies elsewhere.
///
/// Either may be in the plain or the verbatim form, as [`canonical`] and
/// [`child`] give them: `\\?\C:\proj\notes.` is inside `C:\proj`. Paths with
/// no drive or share, as on macOS and Linux, are compared as
/// [`Path::strip_prefix`] compares them.
#[must_use]
pub fn inside(path: &Path, folder: &Path) -> Option<PathBuf> {
    let windows = |p: &Path| p.to_str().and_then(verbatim_form);
    let (Some(path_text), Some(folder_text)) = (windows(path), windows(folder)) else {
        return path.strip_prefix(folder).ok().map(Path::to_path_buf);
    };
    let rest = path_text.strip_prefix(folder_text.trim_end_matches('\\'))?;
    if rest.is_empty() {
        return Some(PathBuf::new());
    }
    rest.strip_prefix('\\')
        .map(|rest| PathBuf::from(rest.trim_end_matches('\\')))
}

/// [`inside`], comparing names as `os`'s own filesystem does.
///
/// Names are compared without case on Windows and macOS, whose default
/// filesystems ignore it, so `C:\Windows\System32` is inside `C:\WINDOWS`,
/// and exactly on Linux. The part under the folder comes back lower-cased
/// where case is ignored. It reads only the paths' text, so a Windows path
/// is compared as one on every OS.
#[must_use]
pub fn inside_on(os: HostOs, path: &Path, folder: &Path) -> Option<PathBuf> {
    if !matches!(os, HostOs::Windows | HostOs::MacOs) {
        return inside(path, folder);
    }
    // Each is written in one form before it is folded, so the `UNC` a
    // share's verbatim form adds is folded with the rest.
    let lowered = |given: &Path| {
        let text = given.to_str()?;
        Some(
            verbatim_form(text)
                .unwrap_or_else(|| text.to_owned())
                .to_lowercase(),
        )
    };
    match (lowered(path), lowered(folder)) {
        (Some(path), Some(folder)) => inside(Path::new(&path), Path::new(&folder)),
        _ => inside(path, folder),
    }
}

/// The root of the drive a Windows path names, `C:\` for `C:\Windows` in
/// either form; `None` for a path with no drive, a share's or a Unix one.
#[must_use]
pub fn drive_root(path: &Path) -> Option<PathBuf> {
    let text = path.to_str()?;
    let text = text.strip_prefix(VERBATIM).unwrap_or(text);
    is_drive(text).then(|| PathBuf::from(&text[..3]))
}

/// The prefix of a verbatim path.
const VERBATIM: &str = r"\\?\";

/// `path` in its plain form when it has one, else as it is.
fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    let plain = path.to_str().and_then(plain_form);
    plain.map_or(path, PathBuf::from)
}

/// `text` without its verbatim prefix, when it has one and the plain form
/// names the same file; `None` otherwise.
///
/// A verbatim path is handed to the filesystem as it is written. A plain one
/// is parsed first, so it names the same file only when parsing changes
/// nothing: it fits in `MAX_PATH`, and no name in it ends in a dot or a
/// space, holds a character the parser reads (`<>:"/|?*` or one below
/// U+0020), is `.` or `..`, or is a reserved device name.
fn plain_form(text: &str) -> Option<String> {
    let rest = text.strip_prefix(VERBATIM)?;
    let (plain, names) = if let Some(share) = rest.strip_prefix(r"UNC\") {
        // `\\?\UNC\server\share\x` is the share path `\\server\share\x`. A
        // share's root keeps its one trailing separator, as `C:\` does.
        let (server, name, below) = share_parts(share)?;
        let names = match below {
            None | Some("") => &share[..server.len() + 1 + name.len()],
            Some(_) => share,
        };
        (format!(r"\\{share}"), names)
    } else {
        // `\\?\C:\x` is the drive path `C:\x`.
        if !is_drive(rest) {
            return None;
        }
        (rest.to_owned(), &rest[3..])
    };
    let fits = plain.encode_utf16().count() < MAX_PATH;
    let names_read_alike = names.is_empty() || names.split('\\').all(reads_alike);
    (fits && names_read_alike).then_some(plain)
}

/// The longest path Windows' plain-path calls take, in UTF-16 units, with the
/// terminating NUL.
const MAX_PATH: usize = 260;

/// A share path's server, share name and whatever follows them, from the text
/// after its leading separators; `None` unless both names are there.
fn share_parts(share: &str) -> Option<(&str, &str, Option<&str>)> {
    let mut parts = share.splitn(3, '\\');
    let (server, name) = (parts.next()?, parts.next()?);
    (!server.is_empty() && !name.is_empty()).then(|| (server, name, parts.next()))
}

/// Whether `text` starts with a drive and its root, as `C:\` does.
fn is_drive(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\'
}

/// `text`, a Windows path with a drive or a share, in the verbatim form; a
/// verbatim one as it is. `None` for any other path, a Unix one among them.
fn verbatim_form(text: &str) -> Option<String> {
    if is_verbatim(text) {
        return Some(text.to_owned());
    }
    if is_drive(text) {
        return Some(format!("{VERBATIM}{text}"));
    }
    let share = text.strip_prefix(r"\\")?;
    share_parts(share)?;
    Some(format!(r"{VERBATIM}UNC\{share}"))
}

/// [`join`]'s verbatim result, when a name in `relative` needs it.
fn exact_join(dir: &Path, relative: &Path) -> Option<PathBuf> {
    let names = relative
        .components()
        .map(|part| match part {
            Component::Normal(name) => name.to_str(),
            Component::CurDir => Some("."),
            Component::ParentDir => Some(".."),
            Component::Prefix(_) | Component::RootDir => None,
        })
        .collect::<Option<Vec<_>>>()?;
    if names
        .iter()
        .all(|n| matches!(*n, "." | "..") || reads_alike(n))
    {
        return None;
    }
    let names = names.into_iter().filter(|n| *n != ".");
    Some(PathBuf::from(join_text(
        verbatim_form(dir.to_str()?)?,
        names,
    )))
}

/// `names` appended to the Windows path `text`, each after one separator.
fn join_text<'a>(mut text: String, names: impl IntoIterator<Item = &'a str>) -> String {
    for name in names {
        if !text.ends_with('\\') {
            text.push('\\');
        }
        text.push_str(name);
    }
    text
}

/// Whether `name` means the same file in a plain path as in a verbatim one.
fn reads_alike(name: &str) -> bool {
    let read = |c: char| c < ' ' || r#"<>:"/|?*"#.contains(c);
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.ends_with(['.', ' '])
        && !name.contains(read)
        && !is_device_name(name)
}

/// Whether a plain path reads `name` as a device: its part before the first
/// dot, trailing spaces trimmed, is one of the reserved names.
fn is_device_name(name: &str) -> bool {
    const DEVICES: [&str; 6] = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"];
    let stem = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    let numbered = |port: &str| {
        stem.get(..3)
            .is_some_and(|head| head.eq_ignore_ascii_case(port))
            && stem.get(3..).is_some_and(|n| {
                matches!(n.as_bytes(), [b'0'..=b'9']) || ["\u{b9}", "\u{b2}", "\u{b3}"].contains(&n)
            })
    };
    DEVICES.iter().any(|d| stem.eq_ignore_ascii_case(d)) || numbered("COM") || numbered("LPT")
}

#[cfg(test)]
#[path = "canonical_tests.rs"]
mod tests;
