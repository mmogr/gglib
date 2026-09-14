//! Directories and files that nobody else on this machine can read.
//!
//! `data/` holds the database (chat history, the proxy's API key, the
//! environment variables given to MCP servers) beside the endpoint identity
//! and the device keys. A build made in a checkout resolves it inside the
//! repository, which other accounts on the machine can often reach, so `data/`
//! is `0700` and the database in it `0600`, each from the moment it exists.
//!
//! **Creating is strict; tightening is best effort.** A mode asked of `mkdir`
//! or `open` costs nothing a plain create did not, so these fail exactly when
//! one would. Tightening what is already there, which an older build left or
//! this crate's `build.rs` made in a checkout, is a `chmod` afterwards, and a
//! filesystem with no Unix modes refuses that for every file. So a tightening
//! that fails is logged rather than returned: a gglib that would not start
//! there would protect nothing.

use std::fs;
use std::io;
use std::path::Path;

/// Create `dir` and any parents it lacks `0700`, and take group and other's
/// access away from a `dir` that was already there.
///
/// # Errors
///
/// Whatever creating the directory returns. A tightening that fails is logged
/// (see the module docs).
pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    create_dir(dir)?;
    make_private(dir);
    Ok(())
}

/// Create `file` empty and `0600`, or take group and other's access away from
/// one that is already there.
///
/// An existing file keeps every byte. The database goes through here on every
/// start, and a truncate would be the user's history gone.
///
/// # Errors
///
/// Whatever creating the file returns, other than that it already exists. A
/// tightening that fails is logged (see the module docs).
pub fn create_private_file(file: &Path) -> io::Result<()> {
    match create_file(file) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            make_private(file);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Take group and other's access away from whatever is at `path`; the owner
/// keeps what it had.
///
/// Nothing at `path` is nothing to do. A failure is logged rather than
/// returned (see the module docs).
pub fn make_private(path: &Path) {
    match tighten(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => tracing::warn!(
            path = %path.display(),
            error = %e,
            "could not take group and other's access away; other accounts may be able to read this"
        ),
    }
}

/// `mkdir` asked for `0700`. The mode applies to every directory this
/// creates, parents included, and to none that already exist.
#[cfg(unix)]
fn create_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

/// Windows has no mode to ask for; the directory inherits its parent's ACL.
#[cfg(not(unix))]
fn create_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)
}

/// `open` asked for `0600`, and only if nothing is there: `create_new` is what
/// makes truncating an existing file impossible here.
#[cfg(unix)]
fn create_file(file: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(file)
}

/// Windows has no mode to ask for; the file inherits its directory's ACL.
#[cfg(not(unix))]
fn create_file(file: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(file)
}

/// Clear group's and other's bits, keeping the owner's and the setuid, setgid
/// and sticky bits, and only when group or other has one, so a file that is
/// already private costs a `stat` and nothing more.
#[cfg(unix)]
#[allow(clippy::verbose_bit_mask)] // `trailing_zeros() >= 6` hides "group and other have nothing"
fn tighten(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)?.permissions().mode();
    if mode & 0o077 == 0 {
        return Ok(());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o7700))
}

/// Windows has no mode to take away.
#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps, clippy::missing_const_for_fn)] // the Unix twin really can fail
fn tighten(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::paths::test_utils::{ENV_LOCK, EnvVarGuard};
    use crate::paths::{database_path, remote_identity_path};

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).expect("metadata").permissions().mode() & 0o777
    }

    fn set_mode(path: &Path, mode: u32) {
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("chmod");
    }

    /// Checked straight after `mkdir`, before `make_private` has had a chance
    /// to hide a create that asked for no mode.
    ///
    /// It bites wherever the bug does. Under the usual `022` umask a plain
    /// `mkdir` gives `0755`; under `077` it gives `0700` as well, and this
    /// cannot tell the two apart. The umask is process-wide and these tests
    /// run in parallel, so it is not set here to force the question.
    #[test]
    fn a_new_directory_is_private_before_any_chmod() {
        let root = tempfile::tempdir().expect("tempdir");
        let dir = root.path().join("data");

        create_dir(&dir).expect("create");

        assert_eq!(mode(&dir) & 0o077, 0, "{:o}", mode(&dir));
    }

    /// The same for a file: the database, whose `-wal` and `-shm` take its
    /// mode when `SQLite` creates them.
    #[test]
    fn a_new_file_is_private_before_any_chmod() {
        let root = tempfile::tempdir().expect("tempdir");
        let file = root.path().join("gglib.db");

        create_file(&file).expect("create");

        assert_eq!(mode(&file) & 0o077, 0, "{:o}", mode(&file));
    }

    /// Every start runs this against a database with the user's history in
    /// it, so an existing file comes out tightened and byte-for-byte whole.
    #[test]
    fn an_existing_database_keeps_every_byte_and_loses_what_others_had() {
        let root = tempfile::tempdir().expect("tempdir");
        let file = root.path().join("gglib.db");
        fs::write(&file, b"SQLite format 3\0and the rest").expect("write");
        set_mode(&file, 0o644);

        create_private_file(&file).expect("an existing file is not an error");

        assert_eq!(
            fs::read(&file).expect("read"),
            b"SQLite format 3\0and the rest"
        );
        assert_eq!(mode(&file), 0o600);
    }

    /// Both accessors create `data/`, and in a checkout `build.rs` has usually
    /// made it first at whatever the umask allowed, so each must leave it
    /// private whether it made it or found it.
    #[test]
    fn database_path_and_remote_identity_path_leave_data_private() {
        let _lock = ENV_LOCK.lock().unwrap();
        let root = tempfile::tempdir().expect("tempdir");
        let _env = EnvVarGuard::set("GGLIB_DATA_DIR", root.path().to_string_lossy().as_ref());
        let data = root.path().join("data");

        database_path().expect("database_path");
        assert_eq!(mode(&data) & 0o077, 0, "made: {:o}", mode(&data));

        set_mode(&data, 0o755);
        remote_identity_path().expect("remote_identity_path");
        assert_eq!(mode(&data), 0o700, "found");
    }

    /// Only group's and other's bits go: a setgid directory someone keeps
    /// the database in stays setgid.
    #[test]
    fn tightening_keeps_the_setgid_and_sticky_bits() {
        let root = tempfile::tempdir().expect("tempdir");
        let dir = root.path().join("shared");
        fs::create_dir(&dir).expect("mkdir");
        set_mode(&dir, 0o3775);

        make_private(&dir);

        let kept = fs::metadata(&dir).expect("metadata").permissions().mode() & 0o7777;
        assert_eq!(kept, 0o3700, "{kept:o}");
    }
}
