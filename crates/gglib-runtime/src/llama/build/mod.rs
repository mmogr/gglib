//! Build orchestration for llama.cpp with progress tracking.
//!
//! Note: CXXFLAGS="-O2" is set during cmake configure and build to work around
//! GCC 15.2.1 segfault bug during optimization passes (particularly -O3).
//! This is a known compiler issue affecting chat.cpp and other files.

use super::detect::{Acceleration, get_cuda_path, get_num_cores, validate_cuda_gcc_compatibility};

#[cfg(target_os = "linux")]
use super::detect::select_cuda_compiler_for_build;

use anyhow::{Context, Result, bail};

use indicatif::{ProgressBar, ProgressStyle};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

/// Build llama.cpp with the specified acceleration
pub fn build_llama_cpp(llama_dir: &Path, acceleration: Acceleration) -> Result<()> {
    println!();
    println!(
        "Building llama.cpp with {} support...",
        acceleration.display_name()
    );
    println!();

    let build_dir = llama_dir.join("build");
    std::fs::create_dir_all(&build_dir).context("Failed to create build directory")?;

    // Step 1: CMake Configure
    configure_cmake(llama_dir, &build_dir, acceleration)?;

    // Step 2: Build
    build_project(&build_dir, acceleration)?;

    println!();
    println!("✓ Build completed successfully");

    Ok(())
}

/// Run `CMake` configuration
fn configure_cmake(llama_dir: &Path, build_dir: &Path, acceleration: Acceleration) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .unwrap(),
    );
    pb.set_message("Configuring with CMake...");

    let mut args = vec![
        "-S",
        llama_dir.to_str().unwrap(),
        "-B",
        build_dir.to_str().unwrap(),
        "-DCMAKE_BUILD_TYPE=Release",
        "-DCMAKE_CXX_FLAGS=-O1 -g0", // Use -O1 to work around GCC 15.2.1 ICE
        "-DCMAKE_C_FLAGS=-O1 -g0",   // Use -O1 to work around GCC 15.2.1 ICE
        "-DCMAKE_CXX_FLAGS_RELEASE=-O1 -DNDEBUG", // Override Release flags
        "-DCMAKE_C_FLAGS_RELEASE=-O1 -DNDEBUG", // Override Release flags
        "-DGGML_METAL_EMBED_LIBRARY=ON",
        "-DLLAMA_BUILD_SERVER=ON",
        "-DLLAMA_BUILD_EXAMPLES=OFF", // Skip examples to avoid GCC bug in some files
        "-DLLAMA_BUILD_TESTS=OFF",    // Skip tests to avoid GCC bug in some files
    ];

    // Add acceleration-specific flags
    let accel_flags = acceleration.cmake_flags();
    args.extend(accel_flags);

    let mut cmd = Command::new("cmake");

    // Work around GCC 15.2.1 segfault bug by reducing optimization level
    cmd.env("CXXFLAGS", "-O1");
    cmd.env("CFLAGS", "-O1");

    // Compiler selection priority (platform-specific):
    // Linux CUDA builds: Clang (best) > GCC 12/11 (compatible) > system GCC
    // macOS: Use system clang (Metal, not CUDA - CUDA not supported on macOS)
    // Windows: Use system compiler (MSVC - CUDA uses MSVC on Windows)
    // Non-CUDA builds: Clang > GCC 14 > system GCC
    //
    // Note: Only Linux requires explicit CUDA/compiler compatibility checks.
    // macOS doesn't support CUDA (uses Metal), Windows uses MSVC with different compatibility.

    #[cfg(target_os = "linux")]
    {
        // Only set CC/CXX if neither is already set by the user
        // If user sets one or both, we respect their choice and let CMake handle pairing
        let cc_set = std::env::var("CC").is_ok();
        let cxx_set = std::env::var("CXX").is_ok();

        if !cc_set && !cxx_set {
            if matches!(acceleration, Acceleration::Cuda) {
                // For CUDA builds, use the same selection logic as validation
                // This ensures we set the compiler that was validated
                let (compiler, _version) = select_cuda_compiler_for_build()?;

                if compiler.contains("clang") {
                    cmd.env("CC", "clang");
                    cmd.env("CXX", "clang++");
                    pb.println("Using clang/clang++ for CUDA build (best compatibility)");
                } else if compiler == "gcc-12" {
                    cmd.env("CC", "gcc-12");
                    cmd.env("CXX", "g++-12");
                    pb.println("Using gcc-12/g++-12 for CUDA compatibility");
                } else if compiler == "gcc-11" {
                    cmd.env("CC", "gcc-11");
                    cmd.env("CXX", "g++-11");
                    pb.println("Using gcc-11/g++-11 for CUDA compatibility");
                }
                // If "gcc" (system default), don't set explicitly
            } else {
                // Non-CUDA builds on Linux: prefer Clang over GCC (GCC 14/15 have compiler bugs)
                if Command::new("clang").arg("--version").output().is_ok() {
                    cmd.env("CC", "clang");
                    cmd.env("CXX", "clang++");
                } else if Command::new("gcc-14").arg("--version").output().is_ok() {
                    cmd.env("CC", "gcc-14");
                    cmd.env("CXX", "g++-14");
                }
            }
        }
    }

    // macOS and Windows: use system default compilers (clang on macOS, MSVC on Windows)
    // No special version selection needed on these platforms

    // NOW validate CUDA/GCC compatibility AFTER compiler selection
    // This ensures we validate the compiler that will actually be used
    if matches!(acceleration, Acceleration::Cuda)
        && let Err(e) = validate_cuda_gcc_compatibility()
    {
        bail!("{}", e);
    }

    // For CUDA builds, set CUDA paths explicitly
    let cuda_args = if matches!(acceleration, Acceleration::Cuda) {
        if let Some(cuda_path) = get_cuda_path() {
            pb.println(format!("Using CUDA installation at: {}", cuda_path));

            // Set environment variables for FindCUDAToolkit
            cmd.env("CUDAToolkit_ROOT", &cuda_path);
            cmd.env("CUDA_PATH", &cuda_path);
            cmd.env("CUDA_TOOLKIT_ROOT_DIR", &cuda_path);

            // Also pass as CMake arguments (more reliable)
            let nvcc_path = format!("{}/bin/nvcc", cuda_path);
            vec![
                format!("-DCUDAToolkit_ROOT={}", cuda_path),
                format!("-DCMAKE_CUDA_COMPILER={}", nvcc_path),
            ]
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Combine all arguments
    args.extend(cuda_args.iter().map(|s| s.as_str()));
    cmd.args(&args);

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to run CMake")?;

    // Capture and display output
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (tx, rx) = mpsc::channel();
    let tx2 = tx.clone();

    // Read stdout
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });

    // Read stderr
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx2.send(line);
        }
    });

    // Update spinner with output
    while let Ok(line) = rx.recv_timeout(std::time::Duration::from_millis(100)) {
        if !line.trim().is_empty() {
            pb.println(&line);
        }
        pb.tick();
    }

    let status = child.wait().context("Failed to wait for CMake")?;

    // Drain any remaining output in the channel before finishing
    // This prevents losing error messages due to race conditions
    while let Ok(line) = rx.recv_timeout(std::time::Duration::from_millis(50)) {
        if !line.trim().is_empty() {
            pb.println(&line);
        }
    }

    pb.finish_and_clear();

    if !status.success() {
        bail!("CMake configuration failed");
    }

    println!("✓ CMake configuration complete");
    Ok(())
}

/// Build the project with progress tracking
fn build_project(build_dir: &Path, acceleration: Acceleration) -> Result<()> {
    println!();

    let num_cores = build_parallelism(acceleration);
    println!("Building with {} parallel jobs...", num_cores);

    let pb = ProgressBar::new(100);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) {msg}")
            .unwrap()
            .progress_chars("#>-")
    );

    let mut child = Command::new("cmake")
        .env("CXXFLAGS", "-O1") // Work around GCC 15.2.1 segfault bug
        .env("CFLAGS", "-O1") // Work around GCC 15.2.1 segfault bug
        .args([
            "--build",
            build_dir.to_str().unwrap(),
            "--config",
            "Release",
            "-j",
            &num_cores.to_string(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to run build")?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (tx, rx) = mpsc::channel();
    let tx2 = tx.clone();

    // Read stdout
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });

    // Read stderr
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx2.send(line);
        }
    });

    let mut last_progress = 0;
    let mut total_files = 100; // Default estimate

    // Process output and update progress
    while let Ok(line) = rx.recv_timeout(std::time::Duration::from_millis(100)) {
        pb.tick();

        // Parse build progress from output
        // Look for patterns like "[ 50%]" or "[150/200]"
        if let Some(progress) = parse_build_progress(&line, &mut total_files)
            && progress > last_progress
        {
            pb.set_length(total_files as u64);
            pb.set_position(progress as u64);
            last_progress = progress;
        }

        // Show important lines: build progress, errors, and warnings
        let line_lower = line.to_ascii_lowercase();
        if line.contains("Building")
            || line.contains("Linking")
            || line_lower.contains("error")
            || line_lower.contains("warning:")
            || line_lower.contains("fatal")
            || line_lower.contains("undefined reference")
            || line_lower.contains("cannot find")
        {
            pb.println(&line);
        }
    }

    let status = child.wait().context("Failed to wait for build")?;

    // Drain any remaining output after process exits
    while let Ok(line) = rx.recv_timeout(std::time::Duration::from_millis(100)) {
        let line_lower = line.to_ascii_lowercase();
        if line_lower.contains("error") || line_lower.contains("fatal") {
            eprintln!("{}", line);
        }
    }

    pb.finish_and_clear();

    if !status.success() {
        bail!("Build failed (exit code: {})", status.code().unwrap_or(-1));
    }

    println!("✓ Compilation complete");
    Ok(())
}

/// Determine build parallelism, capping CUDA builds to avoid OOM.
///
/// CUDA compilation (nvcc) uses significantly more memory per process than
/// regular C++ compilation. Using all CPU cores (e.g. -j32) for CUDA builds
/// can easily exhaust system memory, especially on WSL2 where available RAM
/// may be limited. We cap CUDA builds at 4 parallel jobs by default.
///
/// Respects `CMAKE_BUILD_PARALLEL_LEVEL` environment variable as an override.
fn build_parallelism(acceleration: Acceleration) -> usize {
    // Allow explicit override via environment variable
    if let Ok(val) = std::env::var("CMAKE_BUILD_PARALLEL_LEVEL") {
        if let Ok(n) = val.parse::<usize>() {
            if n > 0 {
                return n;
            }
        }
    }

    let cores = get_num_cores();

    match acceleration {
        // CUDA compilation is very memory-intensive (~2-4 GB per nvcc process)
        Acceleration::Cuda => cores.min(4),
        // CPU and Metal builds are fine with full parallelism
        _ => cores,
    }
}

/// Parse build progress from `CMake` output
fn parse_build_progress(line: &str, total_files: &mut usize) -> Option<usize> {
    // Match "[ 50%]" pattern
    if let Some(start) = line.find('[')
        && let Some(end) = line[start..].find(']')
    {
        let bracket_content = &line[start + 1..start + end];

        // Try percentage format "50%"
        if let Some(pct_pos) = bracket_content.find('%')
            && let Ok(percent) = bracket_content[..pct_pos].trim().parse::<usize>()
        {
            *total_files = 100;
            return Some(percent);
        }

        // Try "150/200" format
        if let Some(slash_pos) = bracket_content.find('/') {
            let current = bracket_content[..slash_pos].trim().parse::<usize>().ok()?;
            let total = bracket_content[slash_pos + 1..]
                .trim()
                .parse::<usize>()
                .ok()?;
            *total_files = total;
            return Some(current);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_build_progress_percentage() {
        let mut total = 100;
        assert_eq!(
            parse_build_progress("[ 50%] Building file.cpp", &mut total),
            Some(50)
        );
        assert_eq!(total, 100);
    }

    #[test]
    fn test_parse_build_progress_fraction() {
        let mut total = 100;
        assert_eq!(
            parse_build_progress("[150/200] Linking target", &mut total),
            Some(150)
        );
        assert_eq!(total, 200);
    }

    #[test]
    fn test_parse_build_progress_no_match() {
        let mut total = 100;
        assert_eq!(parse_build_progress("Some random output", &mut total), None);
        assert_eq!(total, 100);
    }
}
