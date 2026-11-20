//! What extracting a release archive leaves in the bin directory, for both
//! archive kinds, on fixture archives built here.
//!
//! A zip is the Windows release, and its tests run everywhere. On Unix the
//! extractor also makes each file executable, so there a listing says each
//! file's permission bits. A tar.gz is the macOS and Linux release, and its
//! tests are Unix only.

use super::LLAMA_REQUIRED;
use super::tests::extract_binaries;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

/// One archive member that is not a link: its path, and what it is.
enum Member {
    File(&'static str),
    Dir(&'static str),
}

/// The server's name in this system's release: `llama-server`, with `.exe`
/// on Windows.
const SERVER: &str = LLAMA_REQUIRED[0];

/// How a listing says an extracted file: executable where there are
/// permission bits to say so.
const EXTRACTED: &str = if cfg!(unix) { "file 755" } else { "file" };

/// How a listing says a file with `meta`: on Unix with its permission bits.
fn file_kind(meta: &fs::Metadata) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        format!("file {:o}", meta.permissions().mode() & 0o777)
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        "file".to_owned()
    }
}

/// Every entry of `dir` by name: `symlink`, or what [`file_kind`] says.
fn listing(dir: &Path) -> BTreeMap<String, String> {
    fs::read_dir(dir)
        .expect("read the bin directory")
        .map(|entry| {
            let entry = entry.expect("a directory entry");
            let meta = fs::symlink_metadata(entry.path()).expect("an entry's metadata");
            let kind = if meta.file_type().is_symlink() {
                "symlink".to_owned()
            } else {
                file_kind(&meta)
            };
            (entry.file_name().to_string_lossy().into_owned(), kind)
        })
        .collect()
}

fn expected(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(name, kind)| ((*name).to_owned(), (*kind).to_owned()))
        .collect()
}

/// Write a zip holding `members`. Files are stored mode 0644, so a 0755
/// after extraction is the extractor's doing and not the archive's.
fn write_zip(path: &Path, members: &[Member]) {
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o644);
    let mut zip = zip::ZipWriter::new(File::create(path).expect("create the archive"));
    for member in members {
        match member {
            Member::File(name) => {
                zip.start_file(*name, options).expect("a file");
                zip.write_all(b"fixture").expect("a file's bytes");
            }
            Member::Dir(name) => zip.add_directory(*name, options).expect("a dir"),
        }
    }
    zip.finish().expect("finish the zip");
}

#[test]
fn a_zip_yields_its_files_flattened_all_executable() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let archive = tmp.path().join("llama-b1-bin-win-vulkan-x64.zip");
    let bin = tmp.path().join("bin");
    write_zip(
        &archive,
        &[
            Member::File(SERVER),
            Member::File("llama-bench"),
            Member::Dir("sub/"),
            Member::Dir("sub/dir/"),
            Member::File("sub/dir/libllama.so"),
            Member::File("LICENSE"),
            Member::File("docs/LICENSE-httplib"),
            Member::File("include/llama.h"),
            Member::File("ggml-metal.metal"),
        ],
    );

    extract_binaries(&archive, &bin).expect("the archive holds the server");

    assert_eq!(
        listing(&bin),
        expected(&[
            ("libllama.so", EXTRACTED),
            ("llama-bench", EXTRACTED),
            (SERVER, EXTRACTED),
        ])
    );
}

#[test]
fn a_zip_without_llama_server_is_refused() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let archive = tmp.path().join("llama-b1-bin-win-vulkan-x64.zip");
    let bin = tmp.path().join("bin");
    write_zip(&archive, &[Member::File("llama-cli")]);

    let err = extract_binaries(&archive, &bin).expect_err("no server in the archive");

    assert_eq!(
        err.to_string(),
        "Failed to extract all required binaries. Found 0 of 1"
    );
    assert_eq!(listing(&bin), expected(&[("llama-cli", EXTRACTED)]));
}

// Unix only: a tar.gz is the macOS and Linux release, whose members are made
// executable and can be symbolic links.
#[cfg(unix)]
mod tar_gz {
    use super::{EXTRACTED, Member, expected, extract_binaries, listing};
    use std::fs::File;
    use std::path::Path;

    /// Write a tar.gz holding `members`, then each of `links` as a symbolic
    /// link from its name to its target. Files are stored mode 0644, so a
    /// 0755 after extraction is the extractor's doing and not the archive's.
    fn write_tar_gz(path: &Path, members: &[Member], links: &[(&str, &str)]) {
        let gz = flate2::write::GzEncoder::new(
            File::create(path).expect("create the archive"),
            flate2::Compression::fast(),
        );
        let mut tar = tar::Builder::new(gz);
        for member in members {
            let mut header = tar::Header::new_gnu();
            match member {
                Member::File(name) => {
                    let body = b"fixture";
                    header.set_size(body.len() as u64);
                    header.set_mode(0o644);
                    header.set_cksum();
                    tar.append_data(&mut header, name, &body[..])
                        .expect("a file");
                }
                Member::Dir(name) => {
                    header.set_entry_type(tar::EntryType::Directory);
                    header.set_size(0);
                    header.set_mode(0o755);
                    header.set_cksum();
                    tar.append_data(&mut header, name, &b""[..]).expect("a dir");
                }
            }
        }
        for (name, target) in links {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_mode(0o777);
            tar.append_link(&mut header, name, target).expect("a link");
        }
        tar.into_inner()
            .expect("finish the tar")
            .finish()
            .expect("finish the gzip");
    }

    /// The files one directory down are unpacked and made executable; the
    /// directory itself, anything deeper and anything beside it are not.
    ///
    /// A link is unpacked as a link and never followed. A real macOS release
    /// holds versioned aliases of its libraries, and one whose target is not
    /// in the archive, as `libggml.dylib` here, once failed the install with
    /// "No such file or directory" when its permissions were set through it.
    #[test]
    fn a_tar_gz_yields_the_files_one_level_down_all_executable() {
        let tmp = tempfile::tempdir().expect("a temp dir");
        let archive = tmp.path().join("llama-b1-bin-macos-arm64.tar.gz");
        let bin = tmp.path().join("bin");
        write_tar_gz(
            &archive,
            &[
                Member::Dir("llama-b1/"),
                Member::File("llama-b1/llama-server"),
                Member::File("llama-b1/llama-bench"),
                Member::File("llama-b1/libllama.dylib"),
                Member::File("llama-b1/default.metallib"),
                Member::File("llama-b1/LICENSE"),
                Member::File("llama-b1/LICENSE-httplib"),
                Member::File("llama-b1/llama.h"),
                Member::File("llama-b1/ggml-metal.metal"),
                Member::File("llama-b1/share/nested.txt"),
                Member::File("top-level.txt"),
            ],
            &[
                ("llama-b1/libllama.0.dylib", "libllama.dylib"),
                ("llama-b1/libggml.dylib", "libggml-metal.0.dylib"),
            ],
        );

        extract_binaries(&archive, &bin).expect("the archive holds llama-server");

        assert_eq!(
            listing(&bin),
            expected(&[
                ("default.metallib", EXTRACTED),
                ("libggml.dylib", "symlink"),
                ("libllama.0.dylib", "symlink"),
                ("libllama.dylib", EXTRACTED),
                ("llama-bench", EXTRACTED),
                ("llama-server", EXTRACTED),
            ])
        );
    }

    #[test]
    fn a_tar_gz_without_llama_server_is_refused() {
        let tmp = tempfile::tempdir().expect("a temp dir");
        let archive = tmp.path().join("llama-b1-bin-ubuntu-x64.tgz");
        let bin = tmp.path().join("bin");
        write_tar_gz(
            &archive,
            &[
                Member::File("llama-b1/llama-cli"),
                Member::File("llama-b1/deeper/llama-server"),
            ],
            &[],
        );

        let err = extract_binaries(&archive, &bin).expect_err("no llama-server one level down");

        assert_eq!(
            err.to_string(),
            "Failed to extract all required binaries. Found 0 of 1"
        );
        assert_eq!(listing(&bin), expected(&[("llama-cli", EXTRACTED)]));
    }
}
