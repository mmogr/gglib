//! What the source-build pre-flight tells the user before it asks to continue.

use super::preflight_lines;
use gglib_runtime::llama::Acceleration;
use std::path::Path;

/// Every line is something the command checked or will do. There is no
/// disk-space line, because nothing measures free space.
#[test]
fn the_preflight_lists_what_was_checked_and_what_will_happen() {
    assert_eq!(
        preflight_lines(Acceleration::Vulkan, Path::new("/data/.llama/bin")),
        [
            "Pre-flight check:",
            "✓ Build dependencies installed",
            "✓ Detected: Vulkan",
            "",
            "This will:",
            "  1. Clone llama.cpp repository (~150 MB)",
            "  2. Configure with CMake (Vulkan enabled)",
            "  3. Compile llama-server (~3-5 minutes)",
            "  4. Install to /data/.llama/bin",
            "",
        ]
    );
}
