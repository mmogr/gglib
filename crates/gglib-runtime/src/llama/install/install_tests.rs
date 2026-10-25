//! What a llama.cpp source build records about itself.

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
