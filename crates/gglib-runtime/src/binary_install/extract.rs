//! Unpacking a release archive into the bin directory.

use anyhow::{Context, Result, bail};
use std::fs::{self, File};
use std::io;
use std::path::Path;

use super::release::{ArchiveLayout, ReleaseSpec};

/// Extract `spec`'s wanted files from the archive (zip or tar.gz) into
/// `bin_dir`, and fail unless every name in `required` was among them.
///
/// This includes the server binary and every shared library beside it
/// (.dylib on macOS, .dll on Windows, .so on Linux).
pub(crate) fn extract_binaries(
    spec: &ReleaseSpec,
    required: &[&str],
    archive_path: &Path,
    bin_dir: &Path,
) -> Result<()> {
    let name = archive_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    #[allow(
        clippy::case_sensitive_file_extension_comparisons,
        reason = "release asset names are lower case; the rule llama.cpp's install always had"
    )]
    let is_tar_gz = name.ends_with(".tar.gz") || name.ends_with(".tgz");
    if is_tar_gz {
        extract_binaries_tar_gz(spec, required, archive_path, bin_dir)
    } else {
        extract_binaries_zip(spec, required, archive_path, bin_dir)
    }
}

/// Make the extracted `dest_path` executable.
///
/// Reads `symlink_metadata` (lstat) so a symlink is not followed to a target
/// that may not be extracted yet, which would fail with ENOENT. A symlink is
/// left as it is, because `set_permissions` would follow it too.
#[cfg(unix)]
fn make_executable(dest_path: &Path, file_name: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let meta = fs::symlink_metadata(dest_path)
        .with_context(|| format!("Failed to read metadata: {file_name}"))?;
    if !meta.file_type().is_symlink() {
        let mut perms = meta.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(dest_path, perms)
            .with_context(|| format!("Failed to set permissions: {file_name}"))?;
    }
    Ok(())
}

/// Fail unless `found`, the count of extracted members named in `required`,
/// is the count of names in it.
fn ensure_required(found: usize, required: &[&str]) -> Result<()> {
    if found != required.len() {
        bail!(
            "Failed to extract all required binaries. Found {} of {}",
            found,
            required.len()
        );
    }
    Ok(())
}

/// How many path components a tar member to install has under `layout`.
const fn tar_depth(layout: ArchiveLayout) -> usize {
    match layout {
        ArchiveLayout::OneDirDeep => 2,
        ArchiveLayout::Flat => 1,
    }
}

/// Extract binaries from a tar.gz archive (macOS and Linux).
fn extract_binaries_tar_gz(
    spec: &ReleaseSpec,
    required: &[&str],
    archive_path: &Path,
    bin_dir: &Path,
) -> Result<()> {
    use flate2::read::GzDecoder;
    use tar::Archive;

    let file = File::open(archive_path).context("Failed to open downloaded archive")?;
    let gz = GzDecoder::new(file);
    let mut archive = Archive::new(gz);

    fs::create_dir_all(bin_dir).context("Failed to create bin directory")?;

    let depth = tar_depth(spec.archive);
    let mut extracted_binaries = 0;

    for entry in archive.entries().context("Failed to read tar archive")? {
        let mut entry = entry.context("Failed to read archive entry")?;
        let path = entry
            .path()
            .context("Failed to get entry path")?
            .into_owned();
        // Keep only files at the layout's depth: llama.cpp's archives are
        // `llama-b<tag>/<filename>`, so the top-level directory entry itself
        // and anything nested deeper are skipped.
        if path.components().count() != depth {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) if !name.is_empty() => name.to_string(),
            _ => continue,
        };

        if !(spec.wanted)(&file_name) {
            continue;
        }

        let dest_path = bin_dir.join(&file_name);
        entry
            .unpack(&dest_path)
            .with_context(|| format!("Failed to extract: {file_name}"))?;

        #[cfg(unix)]
        make_executable(&dest_path, &file_name)?;

        if required.contains(&file_name.as_str()) {
            extracted_binaries += 1;
        }
    }

    ensure_required(extracted_binaries, required)
}

/// Extract binaries from a zip archive, each member by its file name
/// wherever it sits.
fn extract_binaries_zip(
    spec: &ReleaseSpec,
    required: &[&str],
    zip_path: &Path,
    bin_dir: &Path,
) -> Result<()> {
    let file = File::open(zip_path).context("Failed to open downloaded archive")?;
    let mut archive = zip::ZipArchive::new(file).context("Failed to read zip archive")?;

    fs::create_dir_all(bin_dir).context("Failed to create bin directory")?;

    let mut extracted_binaries = 0;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .context("Failed to read archive entry")?;
        let entry_name = entry.name().to_string();

        if entry.is_dir() {
            continue;
        }

        let file_name = match entry_name.rsplit('/').next() {
            Some(name) if !name.is_empty() => name,
            _ => continue,
        };

        if !(spec.wanted)(file_name) {
            continue;
        }

        let dest_path = bin_dir.join(file_name);
        let mut dest_file = File::create(&dest_path)
            .with_context(|| format!("Failed to create file: {}", dest_path.display()))?;

        io::copy(&mut entry, &mut dest_file)
            .with_context(|| format!("Failed to extract: {file_name}"))?;

        #[cfg(unix)]
        make_executable(&dest_path, file_name)?;

        if required.contains(&file_name) {
            extracted_binaries += 1;
        }
    }

    ensure_required(extracted_binaries, required)
}
