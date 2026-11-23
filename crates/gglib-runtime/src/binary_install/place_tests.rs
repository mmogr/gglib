//! An install puts each file down as a new file: what was there is left as
//! it was under any other name it has, and a program still running from it
//! does not stop the new one from starting.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::fake_github::{FakeGitHub, Reply, a_working_server};
use super::fetch::{GitHubAsset, GitHubRelease, download_cuda_runtime};
use super::install_tests::zip_with;
use super::place::place;
use super::*;

/// The server an archive here holds.
const SERVER: &str = "widget-server";

fn no_downloads() -> Result<PathBuf> {
    bail!("an unpack downloads nothing")
}

fn everything(_: &str) -> bool {
    true
}

fn spec() -> ReleaseSpec {
    ReleaseSpec {
        product: "widget.cpp",
        repo: "acme/widget.cpp",
        pinned: "v1",
        env: "GGLIB_WIDGET_RELEASE",
        download_dir: no_downloads,
        archive: ArchiveLayout::Flat,
        choice: AssetChoice::First,
        wanted: everything,
    }
}

/// A tar.gz holding `name` with `bytes`, at the archive's root.
fn tar_gz_with(name: &str, bytes: &[u8]) -> Vec<u8> {
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut tar = tar::Builder::new(gz);
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, name, bytes).expect("a member");
    tar.into_inner()
        .expect("finish the tar")
        .finish()
        .expect("finish the gzip")
}

/// A bin directory whose server holds `old`, and a second name for that
/// same file. Written into, the file shows its new bytes under both names;
/// replaced, the second name keeps the old ones.
fn installed(tmp: &Path, old: &[u8]) -> (PathBuf, PathBuf) {
    let bin = tmp.join("bin");
    fs::create_dir(&bin).expect("the bin directory");
    fs::write(bin.join(SERVER), old).expect("the installed server");
    let other_name = tmp.join("the-server-that-was-there");
    fs::hard_link(bin.join(SERVER), &other_name).expect("a second name for it");
    (bin, other_name)
}

/// Every name in `dir`, sorted.
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("read the directory")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn a_zip_member_replaces_the_file_that_is_there() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let (bin, other_name) = installed(tmp.path(), b"the old server");
    let archive = tmp.path().join("w.zip");
    fs::write(&archive, zip_with(&[(SERVER, b"the new server".to_vec())])).unwrap();

    extract_binaries(&spec(), &[SERVER], &archive, &bin).expect("the server is there");

    assert_eq!(fs::read(bin.join(SERVER)).unwrap(), b"the new server");
    assert_eq!(fs::read(other_name).unwrap(), b"the old server");
    assert_eq!(names_in(&bin), [SERVER]);
}

#[test]
fn a_tar_gz_member_replaces_the_file_that_is_there() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let (bin, other_name) = installed(tmp.path(), b"the old server");
    let archive = tmp.path().join("w.tar.gz");
    fs::write(&archive, tar_gz_with(SERVER, b"the new server")).unwrap();

    extract_binaries(&spec(), &[SERVER], &archive, &bin).expect("the server is there");

    assert_eq!(fs::read(bin.join(SERVER)).unwrap(), b"the new server");
    assert_eq!(fs::read(other_name).unwrap(), b"the old server");
    assert_eq!(names_in(&bin), [SERVER]);
}

#[test]
fn a_file_that_cannot_be_written_whole_leaves_the_one_that_is_there() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let (bin, other_name) = installed(tmp.path(), b"the old server");

    let err = place(&bin.join(SERVER), |file| {
        file.write_all(b"half of the new")?;
        Err(std::io::Error::other("the archive ended early"))
    })
    .expect_err("the write failed");

    assert_eq!(err.to_string(), "the archive ended early");
    assert_eq!(fs::read(bin.join(SERVER)).unwrap(), b"the old server");
    assert_eq!(fs::read(other_name).unwrap(), b"the old server");
    assert_eq!(names_in(&bin), [SERVER]);
}

#[tokio::test]
async fn a_cuda_runtime_dll_replaces_the_file_that_is_there() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let bin = tmp.path().join("bin");
    fs::create_dir(&bin).expect("the bin directory");
    fs::write(bin.join("cudart64_12.dll"), b"the old runtime").expect("the installed DLL");
    let other_name = tmp.path().join("the-dll-that-was-there");
    fs::hard_link(bin.join("cudart64_12.dll"), &other_name).expect("a second name for it");
    let downloads = tmp.path().join("downloads");
    fs::create_dir(&downloads).expect("the download directory");
    let fake = FakeGitHub::serve().await;
    fake.route(
        "/download/cudart.zip",
        Reply::ok(zip_with(&[(
            "cudart64_12.dll",
            b"the new runtime".to_vec(),
        )])),
    );
    let release = GitHubRelease {
        tag_name: "v1".to_owned(),
        assets: vec![GitHubAsset {
            name: "cudart-widget-bin-win-cu12-x64.zip".to_owned(),
            browser_download_url: format!("{}/download/cudart.zip", fake.base),
        }],
    };

    download_cuda_runtime(
        &reqwest::Client::new(),
        &release,
        "cudart",
        &bin,
        &downloads,
    )
    .await
    .expect("the runtime is unpacked");

    assert_eq!(
        fs::read(bin.join("cudart64_12.dll")).unwrap(),
        b"the new runtime"
    );
    assert_eq!(fs::read(other_name).unwrap(), b"the old runtime");
    assert_eq!(names_in(&bin), ["cudart64_12.dll"]);
}

/// A started program, stopped when the test is done with it.
struct Running(std::process::Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A server started before the install and still running when its file is
/// replaced, as one that has outlived its daemon is (#2204). macOS and Linux
/// put the new file in, and it starts. Windows refuses to replace the file
/// of a running program, and the server that was there is left whole.
#[test]
fn a_zip_unpacked_over_a_running_server_leaves_one_that_starts() {
    use gglib_core::utils::process::bounded::PROBE_WAIT;
    use std::time::{Duration, Instant};

    let tmp = tempfile::tempdir().expect("a temp dir");
    let bin = tmp.path().join("bin");
    fs::create_dir(&bin).expect("the bin directory");
    let started = tmp.path().join("started");
    let server = gglib_test_proc::install(
        &bin,
        SERVER,
        &format!("touch {}\nsleep 60", started.display()),
    );
    let name = server
        .file_name()
        .and_then(|name| name.to_str())
        .expect("the server's file name")
        .to_owned();
    let running = Running(
        gglib_core::utils::process::cmd(&server)
            .spawn()
            .expect("start the server that is there"),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while !started.exists() {
        assert!(Instant::now() < deadline, "the server never started");
        std::thread::sleep(Duration::from_millis(20));
    }
    // A server with no instructions beside it answers `--version`.
    fs::remove_file(gglib_test_proc::instructions_path(&server)).expect("its instructions");
    let archive = tmp.path().join("w.zip");
    fs::write(&archive, zip_with(&[(&name, a_working_server())])).unwrap();

    let unpacked = extract_binaries(&spec(), &[&name], &archive, &bin);
    let answer = crate::engine::validate_binary(&server, "widget install", PROBE_WAIT);
    drop(running);

    if cfg!(windows) {
        unpacked.expect_err("a running program's file is not replaced");
    } else {
        unpacked.expect("the server is there");
    }
    answer.expect("the server that is there answers");
    assert_eq!(names_in(&bin), [name]);
}
