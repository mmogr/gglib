//! What a llama.cpp source build records about itself, and how its binary is
//! put in place.

use super::*;

/// The record names the release, the commit and the acceleration, and the
/// one flag that acceleration configured, which the status prints back.
#[test]
fn a_builds_record_names_its_acceleration_and_that_accelerations_flag() {
    let record = build_record("b11429".into(), "d81235049abc".into(), Acceleration::Vulkan);

    assert_eq!(record.version, "b11429");
    assert_eq!(record.commit_sha, "d81235049abc");
    assert_eq!(record.acceleration, "Vulkan");
    assert_eq!(record.cmake_flags, ["-DGGML_VULKAN=ON"]);
}

/// The built binary goes in as a new file. The one that was there keeps its
/// bytes under a second name it was given, which it would not if it had
/// been written into.
#[test]
fn a_built_binary_replaces_the_file_that_is_there() {
    let tmp = tempfile::tempdir().expect("a temp dir");
    let built = if cfg!(windows) {
        tmp.path().join("build/bin/Release/widget-server.exe")
    } else {
        tmp.path().join("build/bin/widget-server")
    };
    fs::create_dir_all(built.parent().expect("its directory")).expect("the build directory");
    fs::write(&built, b"the new server").expect("the built binary");
    let installed = tmp.path().join("bin").join("widget-server");
    fs::create_dir(tmp.path().join("bin")).expect("the bin directory");
    fs::write(&installed, b"the old server").expect("the installed server");
    let other_name = tmp.path().join("the-server-that-was-there");
    fs::hard_link(&installed, &other_name).expect("a second name for it");

    install_binary(tmp.path(), "widget-server", &installed).expect("the binary is installed");

    assert_eq!(fs::read(&installed).unwrap(), b"the new server");
    assert_eq!(fs::read(other_name).unwrap(), b"the old server");
    let names: Vec<_> = fs::read_dir(tmp.path().join("bin"))
        .expect("read the bin directory")
        .map(|entry| entry.expect("an entry").file_name())
        .collect();
    assert_eq!(names, ["widget-server"]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&installed).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }
}
