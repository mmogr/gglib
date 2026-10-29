use std::env;
use std::fs;
use std::path::Path;

include!("../build_common.rs");

/// The framework the cdylib is the binary of, as its install name says.
const FRAMEWORK: &str = "GglibFFI";

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    process_readme_for_rustdoc(&crate_dir);
    emit_uniffi_version(&crate_dir);
    emit_install_name();
}

/// Hands the resolved `uniffi` version to the crate as
/// `GGLIB_FFI_UNIFFI_VERSION`, for `BuildInfo::uniffi_version`.
///
/// `uniffi` publishes no constant naming its own version, and the manifest's
/// requirement (`0.32`) is a range, so the version the library was built with
/// is read from the workspace's `Cargo.lock`, which names exactly one.
fn emit_uniffi_version(crate_dir: &str) {
    let lock = Path::new(crate_dir).join("../../Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    let text =
        fs::read_to_string(&lock).unwrap_or_else(|e| panic!("reading {}: {e}", lock.display()));
    let versions: Vec<&str> = text
        .split("[[package]]")
        .filter(|block| {
            block
                .lines()
                .any(|line| line.trim() == r#"name = "uniffi""#)
        })
        .filter_map(|block| {
            block
                .lines()
                .find_map(|line| line.trim().strip_prefix("version = "))
                .map(|v| v.trim_matches('"'))
        })
        .collect();
    let [version] = versions.as_slice() else {
        panic!(
            "Cargo.lock should name one uniffi, and names {}: {versions:?}",
            versions.len()
        );
    };
    println!("cargo:rustc-env=GGLIB_FFI_UNIFFI_VERSION={version}");
}

/// Names the cdylib by the path an app finds it at inside its framework.
///
/// An app's loader reads a library's install name to find it, and Xcode
/// embeds a framework under `@rpath`. A macOS framework is a versioned bundle,
/// so its binary sits under `Versions/A`; an iOS one is flat. Set at link
/// time rather than rewritten afterwards with `install_name_tool`, so the
/// linker's own ad-hoc signature stays valid. Every other target keeps
/// rustc's default.
fn emit_install_name() {
    let install_name = match env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("ios") => format!("@rpath/{FRAMEWORK}.framework/{FRAMEWORK}"),
        Ok("macos") => format!("@rpath/{FRAMEWORK}.framework/Versions/A/{FRAMEWORK}"),
        _ => return,
    };
    println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,{install_name}");
}
