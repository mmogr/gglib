//! Putting a file where an engine is run from.

use std::fs::File;
use std::io;
use std::path::Path;

/// Put a file at `dest` as a new file: `fill` writes it under a temporary
/// name beside `dest`, and it is then renamed over whatever is there.
///
/// The file that was there is never written into. A process may still be
/// running from it, and macOS kills every later start of a program whose
/// file was written into under a running process ([#2204]). When `fill` or
/// the rename fails, `dest` is as it was and the temporary file is gone.
///
/// The new file has the permissions `File::create` gives one; making it
/// executable is the caller's.
///
/// [#2204]: https://github.com/mmogr/gglib/issues/2204
pub(crate) fn place(dest: &Path, fill: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    let dir = match dest.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    let mut builder = tempfile::Builder::new();
    builder.prefix(".placing-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o666));
    }
    let mut new = builder.tempfile_in(dir)?;
    fill(new.as_file_mut())?;
    new.persist(dest).map_err(|refused| refused.error)?;
    Ok(())
}
