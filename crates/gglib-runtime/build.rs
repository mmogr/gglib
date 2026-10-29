use std::env;
use std::fs;
use std::path::Path;

include!("../build_common.rs");

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    process_readme_for_rustdoc(&crate_dir);
    find_llama_framework(Path::new(&crate_dir));
}

/// With the `inproc-contract` feature on a Mac, this crate's test binaries
/// link llama.framework through gglib-engine-llama, whose own build script
/// reaches only that crate's tests: they find the Mac's slice where
/// `GGLIB_LLAMA_XCFRAMEWORK` names it, read from the workspace root when
/// relative, as that build script reads it.
fn find_llama_framework(crate_dir: &Path) {
    println!("cargo::rerun-if-env-changed=GGLIB_LLAMA_XCFRAMEWORK");
    let feature = env::var_os("CARGO_FEATURE_INPROC_CONTRACT").is_some();
    let mac = env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "macos");
    let Some(xcframework) = env::var_os("GGLIB_LLAMA_XCFRAMEWORK") else {
        return;
    };
    if !(feature && mac) {
        return;
    }
    let root = crate_dir.ancestors().nth(2).unwrap_or(crate_dir);
    let slice = root.join(xcframework).join("macos-arm64_x86_64");
    println!("cargo::rustc-link-arg=-Wl,-rpath,{}", slice.display());
}
