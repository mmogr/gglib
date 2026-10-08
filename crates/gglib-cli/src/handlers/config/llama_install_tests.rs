//! What the source build says before it starts.

use super::*;

fn tool(name: &'static str, found: Option<&str>) -> BuildTool {
    BuildTool {
        name,
        found: found.map(str::to_owned),
    }
}

#[test]
fn the_dependency_check_lists_each_tool_it_found() {
    let tools = [
        tool("git", Some("2.39.5 (Apple Git-154)")),
        tool("cmake", Some("3.28.1")),
        tool(
            "C++ compiler",
            Some("clang++ (Apple clang version 15.0.0 (clang-1500.1.0.2.5))"),
        ),
    ];

    assert_eq!(
        dependency_lines(&tools),
        [
            "Checking build dependencies...",
            "✓ git (version 2.39.5 (Apple Git-154))",
            "✓ cmake (version 3.28.1)",
            "✓ C++ compiler clang++ (Apple clang version 15.0.0 (clang-1500.1.0.2.5))",
        ]
    );
}

/// A missing tool is named, and the platform's install lines follow between
/// the two sentences that have always framed them.
#[test]
fn the_dependency_check_says_how_to_install_what_is_missing() {
    let tools = [
        tool("git", Some("2.43.0")),
        tool("cmake", None),
        tool("C++ compiler", None),
    ];

    let mut expected = vec![
        "Checking build dependencies...".to_owned(),
        "✓ git (version 2.43.0)".to_owned(),
        "✗ cmake not found".to_owned(),
        "✗ C++ compiler not found".to_owned(),
        String::new(),
        "Missing dependencies detected. Please install:".to_owned(),
        String::new(),
    ];
    expected.extend(build_tool_install_lines());
    expected.push(String::new());
    expected.push("After installing, run 'gglib config llama install' again.".to_owned());

    assert_eq!(dependency_lines(&tools), expected);
}

/// The four requirements line up, and each missing package is followed by
/// what installs it on each family.
#[test]
fn a_vulkan_build_that_cannot_start_says_what_is_missing_and_how_to_get_it() {
    let vk = VulkanStatus {
        has_loader: true,
        has_headers: false,
        has_glslc: true,
        has_spirv_headers: false,
        missing: vec![MissingPackage::VulkanHeaders, MissingPackage::Glslc],
    };

    assert_eq!(
        vulkan_unready_lines(&vk),
        [
            "",
            "\x1b[1;31m✗ Vulkan build requirements not met\x1b[0m",
            "",
            "  Vulkan runtime (loader): ✓ found",
            "  Vulkan dev headers:      ✗ missing",
            "  SPIR-V compiler (glslc): ✓ found",
            "  SPIR-V headers:          ✗ missing",
            "",
            "Install the missing components to build with Vulkan:",
            "",
            "  Vulkan development headers:",
            "    Arch             sudo pacman -S vulkan-headers",
            "    Ubuntu/Debian    sudo apt install libvulkan-dev",
            "    Fedora           sudo dnf install vulkan-devel",
            "  SPIR-V shader compiler (glslc):",
            "    Arch             sudo pacman -S shaderc",
            "    Ubuntu/Debian    sudo apt install glslc",
            "    Fedora           sudo dnf install glslc",
            "",
        ]
    );
}

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
