//! `gglib config llama detect` — hardware acceleration detection.
//!
//! Prints a human-readable summary of GPU acceleration and Vulkan
//! build-readiness by default, or emits raw JSON when `--json` is
//! passed. Exit code 0 means all build dependencies are met; non-zero
//! means something is missing.

use anyhow::Result;

use gglib_runtime::llama::{Acceleration, detect_optimal_acceleration, vulkan_status};

/// Execute the `detect` subcommand.
///
/// Returns `Ok(())` when all build dependencies for the detected
/// acceleration are present. Returns `Err` (non-zero exit) when
/// dependencies are missing.
pub fn execute(json: bool) -> Result<()> {
    let accel = detect_optimal_acceleration();
    let vk = vulkan_status();

    if json {
        print_json(&accel, &vk);
    } else {
        print_human(&accel, &vk);
    }

    // Exit non-zero when the detected backend can't actually build.
    // For Vulkan, that means headers + glslc must be present.
    // For other backends, detection success implies build-readiness.
    match &accel {
        Ok(Acceleration::Vulkan) if !vk.ready_for_build() => {
            if !json {
                eprintln!();
                eprintln!("Vulkan runtime detected but build dependencies are missing.");
                eprintln!("Install the missing components listed above, then re-run.");
            }
            std::process::exit(1);
        }
        Err(_) => {
            std::process::exit(1);
        }
        _ => Ok(()),
    }
}

/// JSON output for machine consumption by shell scripts.
fn print_json(accel: &Result<Acceleration>, vk: &gglib_runtime::llama::VulkanStatus) {
    let (acceleration, ready) = match accel {
        Ok(a) => {
            let ready = match a {
                Acceleration::Vulkan => vk.ready_for_build(),
                _ => true,
            };
            (Some(a.display_name().to_string()), ready)
        }
        Err(_) => (None, false),
    };

    let vulkan_status = match accel {
        Ok(Acceleration::Vulkan) => Some(serde_json::to_value(vk).unwrap()),
        _ => None,
    };

    let output = serde_json::json!({
        "acceleration": acceleration,
        "readyForBuild": ready,
        "vulkanStatus": vulkan_status,
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&output).expect("serialization cannot fail")
    );
}

/// Human-readable summary for interactive CLI use.
fn print_human(accel: &Result<Acceleration>, vk: &gglib_runtime::llama::VulkanStatus) {
    println!("GPU Acceleration Detection");
    println!("==========================");
    println!();

    match accel {
        Ok(a) => {
            println!("  Detected: {}", a.display_name());
            println!();

            if *a == Acceleration::Vulkan {
                println!("Vulkan Build Readiness");
                println!("----------------------");
                let check = |ok: bool| if ok { "✓" } else { "✗" };
                println!("  Vulkan runtime (loader): {}", check(vk.has_loader));
                println!("  Vulkan dev headers:      {}", check(vk.has_headers));
                println!("  SPIR-V compiler (glslc): {}", check(vk.has_glslc));
                println!();

                if vk.ready_for_build() {
                    println!("  ✓ Ready to build with -DGGML_VULKAN=ON");
                } else {
                    println!("  ✗ Not ready — install missing components:");
                    println!();
                    for pkg in &vk.missing {
                        println!("  {}:", pkg.label());
                        for (distro, cmd) in pkg.install_hints() {
                            println!("    {distro:16} {cmd}");
                        }
                    }
                }
            } else {
                println!("  ✓ Ready to build with {}", a.display_name());
            }
        }
        Err(e) => {
            println!("  ✗ {e}");
        }
    }
}
