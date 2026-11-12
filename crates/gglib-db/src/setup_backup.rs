//! A copy of the library, kept beside it, from before an upgrade changed it.
//!
//! An upgrade raises the schema generation, and from then on an older gglib
//! refuses the file. If the newer build was a branch that went nowhere, the
//! library would be stranded on a generation no release has. So before
//! upgrading a file, setup copies it to `<db file name>.gen<N>.bak`, where `N`
//! is the generation it had, and removes the backups of other generations.
//! The refusal names it and how to put it back.
//!
//! The first copy of a generation is kept and never written over, so of two
//! processes upgrading one file at once, the copy kept is the one taken
//! before either changed it. The cost: a backup of that generation from an
//! earlier open is kept, not refreshed.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sqlx::SqlitePool;

use super::generation::{SCHEMA_GENERATION, stored};

/// Copy the file `pool` opens to its backup, if this open is about to
/// upgrade it: it holds a schema, stamped below this build's generation, and
/// has no backup of that generation yet. A new, empty file has nothing to
/// keep.
pub(super) async fn before_upgrade(pool: &SqlitePool, db_path: &Path) -> Result<()> {
    let objects: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master")
        .fetch_one(pool)
        .await?;
    let generation = stored(pool).await?;
    if objects == 0 || generation >= SCHEMA_GENERATION {
        return Ok(());
    }
    let backup = backup_path(db_path, generation);
    if !is_file(&backup) {
        write(pool, &backup).await.with_context(|| {
            format!(
                "backing up {} to {} before upgrading it; nothing has been changed",
                db_path.display(),
                backup.display()
            )
        })?;
    }
    for (_, older) in backups(db_path) {
        if older != backup
            && let Err(e) = std::fs::remove_file(&older)
        {
            tracing::warn!(path = %older.display(), error = %e, "could not remove an older backup");
        }
    }
    Ok(())
}

/// The backup beside `db_path`, and the generation it holds. Of several, the
/// highest generation.
pub fn backup_beside(db_path: &Path) -> Option<(i64, PathBuf)> {
    backups(db_path)
        .into_iter()
        .max_by_key(|(generation, _)| *generation)
}

/// What to add to the refusal of a newer file: a backup this build opens,
/// when it was taken, and how to put it back.
pub(super) fn how_to_restore(db_path: &Path) -> Option<String> {
    let (_, backup) = backup_beside(db_path).filter(|(g, _)| *g <= SCHEMA_GENERATION)?;
    let taken = std::fs::metadata(&backup)
        .and_then(|m| m.modified())
        .map(|t| {
            chrono::DateTime::<chrono::Local>::from(t)
                .format(", taken %Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default();
    let (db, bak) = (db_path.display(), backup.display());
    // A `-wal` left beside the restored file would be replayed onto it. Moved,
    // not copied, so the next upgrade takes a fresh copy instead of keeping
    // this one.
    let command = if cfg!(windows) {
        format!("del \"{db}-wal\" \"{db}-shm\" & move /y \"{bak}\" \"{db}\"  (Command Prompt)")
    } else {
        format!("rm -f '{db}-wal' '{db}-shm' && mv '{bak}' '{db}'")
    };
    Some(format!(
        " This build opens the backup {bak}{taken}. To go back to it, losing what changed \
         since: quit the newer gglib's desktop app and stop its daemon with that build \
         (`gglib daemon stop` refuses here too), then\n  {command}"
    ))
}

/// `<db file name>.gen<generation>.bak`, beside the database.
fn backup_path(db_path: &Path, generation: i64) -> PathBuf {
    let mut name = db_path.file_name().unwrap_or_default().to_owned();
    name.push(format!(".gen{generation}.bak"));
    db_path.with_file_name(name)
}

/// Every backup beside `db_path`: a plain file named exactly as
/// [`backup_path`] names one.
fn backups(db_path: &Path) -> Vec<(i64, PathBuf)> {
    let Some(prefix) = db_path.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };
    let prefix = format!("{prefix}.gen");
    let dir = db_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if !entry.file_type().ok()?.is_file() {
                return None;
            }
            let name = entry.file_name();
            let digits = name.to_str()?.strip_prefix(&prefix)?.strip_suffix(".bak")?;
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            Some((digits.parse().ok()?, db_path.with_file_name(&name)))
        })
        .collect()
}

/// Write the backup through a fresh temporary file beside it, so a failure
/// leaves no partial backup.
async fn write(pool: &SqlitePool, backup: &Path) -> Result<()> {
    let mut tmp = OsString::from(backup);
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    let written = write_through(pool, &tmp, backup).await;
    // Published or not, the temporary file is done with; one left over is
    // only litter.
    let _ = std::fs::remove_file(&tmp);
    written
}

async fn write_through(pool: &SqlitePool, tmp: &Path, backup: &Path) -> Result<()> {
    match std::fs::remove_file(tmp) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    // Empty and this user's alone; `VACUUM INTO` writes into an empty file
    // and keeps its mode.
    gglib_core::paths::create_private_file(tmp)?;
    let tmp_str = tmp
        .to_str()
        .with_context(|| format!("{} is not UTF-8", tmp.display()))?;
    sqlx::query("VACUUM INTO ?")
        .bind(tmp_str)
        .execute(pool)
        .await?;
    publish(tmp, backup)?;
    Ok(())
}

/// Link the finished copy in under the backup's name. A backup already there
/// was published by another process opening the file at the same moment,
/// before that process changed it, so it is kept. Anything else in the way
/// fails the backup.
fn publish(tmp: &Path, backup: &Path) -> io::Result<()> {
    match std::fs::hard_link(tmp, backup) {
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists && is_file(backup) => Ok(()),
        linked => linked,
    }
}

/// Whether `path` is a plain file, not a directory or a link.
fn is_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
}

#[cfg(test)]
#[path = "setup_backup_tests.rs"]
mod tests;
