use std::env;
use std::fs;
use std::path::Path;

include!("../build_common.rs");

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    // Process README for rustdoc (uses shared build_common.rs)
    process_readme_for_rustdoc(&crate_dir);

    // Windows gives a program's main thread the stack its header asks for,
    // 1 MB unless the linker is told otherwise; Linux and macOS give 8 MB.
    // Building the command tree takes more than 1 MB in a debug build
    // (mmogr/gglib#1969), so `gglib.exe` asks for the 8 MB the others have.
    // The target is read from cargo: a build script is compiled for the host.
    // The static C runtime, the other Windows build setting, is a compile
    // flag for every crate and so lives in .cargo/config.toml.
    let target = |key: &str| env::var(format!("CARGO_CFG_TARGET_{key}")).unwrap_or_default();
    if target("OS") == "windows" && target("ENV") == "msvc" {
        println!("cargo:rustc-link-arg-bins=/STACK:8388608");
    }
}
