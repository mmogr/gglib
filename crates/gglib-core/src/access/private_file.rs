//! Writing a secret to disk that nobody else on this machine can read.
//!
//! Split from `device_keys.rs` when the daemon token became the second secret
//! written this way: both files are `0600` beside the endpoint identity, and
//! both are replaced whole, so one writer serves the two.

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::paths::{create_new_private_file, create_private_dir};

/// Replace the file at `path` with `bytes`, `0600`, atomically.
///
/// Written to a sibling temporary file and renamed, so a crash mid-write
/// leaves the previous file rather than a truncated one: a half-written
/// roster is a listener that admits some devices and not others, with nothing
/// saying which.
///
/// The temporary file is `0600` from the moment it exists, not from a chmod
/// once the keys are already in it; `create_private` says why. A temporary a
/// crash leaves behind is therefore no more readable than the file it would
/// have replaced, and it is not swept here: another process may be mid-write
/// on a temporary of its own, and deleting that one brings back the rename
/// collision the per-writer names below exist to prevent. The one exception
/// is a leftover under this writer's own name, which `create_private` removes.
///
/// **The temporary file is named per writer, not per path.** A fixed
/// `.tmp` sibling makes two concurrent writers collide on one filename:
/// both write it, the first renames it away, and the second fails at its own
/// `rename` with `NotFound` — an error raised for a write that was perfectly
/// valid. The rename is what makes this atomic, and it only does so if each
/// writer has its own thing to rename.
///
/// # Errors
///
/// [`io::Error`] from creating the directory, creating or writing the
/// temporary file, removing a leftover under its name, setting its mode, or
/// the rename.
pub(super) fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        create_private_dir(parent)?;
    }

    let tmp = path.with_extension(format!(
        "tmp.{}.{}",
        std::process::id(),
        NEXT_TMP.fetch_add(1, Ordering::Relaxed)
    ));
    let written = create_private(&tmp)
        .and_then(|mut file| {
            restrict(&file)?;
            file.write_all(bytes)
        })
        .and_then(|()| fs::rename(&tmp, path));
    if written.is_err() {
        // Best effort: a temporary nobody renamed is litter beside a `0600`
        // secret, and the error being returned is the one that matters.
        let _ = fs::remove_file(&tmp);
    }
    written
}

/// Distinguishes one writer's temporary file from another's within a process;
/// the pid does it across processes in one pid namespace.
static NEXT_TMP: AtomicU64 = AtomicU64::new(0);

/// Open the temporary file for writing: new, and `0600` from the moment it
/// exists.
///
/// `fs::write` creates with `0666` less the umask, which is `0644` on most
/// machines, and a mode set afterwards leaves a window in which every device
/// key is on disk and readable by anyone on the machine. A crash inside that
/// window leaves them that way for good, under a name nothing goes back to.
/// Asking `open` for the mode closes the window; it is how modelpipe creates
/// the endpoint identity beside this file.
///
/// **New, never reused.** A file already under this name — a pid used again,
/// after a reboot or when pids wrap, starts the counter again — keeps the
/// mode it has, and somebody may have opened it while that let them, holding
/// a descriptor no chmod reaches; a symlink there sends the open, and its
/// truncate, to whatever file it names. So [`create_new_private_file`] refuses
/// a name that is taken, a link included, and the leftover is removed and the
/// create tried once more: no other writer on this machine, in this pid
/// namespace, can be using a name that carries this process's pid and a count
/// only it drew. Removing a name writes nothing to the file it named, and
/// removes a link rather than its target. A second refusal is returned rather
/// than chased, and so is a leftover that cannot be removed, such as a
/// directory.
pub(super) fn create_private(path: &Path) -> io::Result<fs::File> {
    match create_new_private_file(path) {
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            fs::remove_file(path)?;
            create_new_private_file(path)
        }
        opened => opened,
    }
}

/// `0600` exactly where the platform has a notion of it: the umask can take
/// bits from the mode the create asked for, the owner's own among them. Set
/// on the descriptor, so what changes is the file this writer created and
/// not whatever is under its name by now.
#[cfg(unix)]
fn restrict(file: &fs::File) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
}

/// Windows has no mode to set; the file inherits the directory's ACL, which is
/// the same protection the endpoint identity gets there.
#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps, clippy::missing_const_for_fn)] // the Unix twin really can fail
fn restrict(_file: &fs::File) -> io::Result<()> {
    Ok(())
}
