use std::env;
use std::fs;
use std::path::Path;

include!("../build_common.rs");

/// The cfg that compiles the decoder over llama.h in: set only when the C
/// shim was compiled and llama.framework linked.
const LINKED: &str = "gglib_llama";

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    process_readme_for_rustdoc(&crate_dir);
    println!("cargo::rustc-check-cfg=cfg({LINKED})");
    #[cfg(feature = "llama")]
    llama::link(Path::new(&crate_dir));
}

/// The `llama` feature: `csrc/gglib_llama.c` compiled against the `llama.h`
/// of the `llama.framework` slice the target links, and that framework
/// linked.
///
/// Only for an Apple target, and only when `GGLIB_LLAMA_XCFRAMEWORK` names
/// the framework `make phone-llama` builds; otherwise nothing is compiled or
/// linked, and the crate builds as it does without the feature. A relative
/// `GGLIB_LLAMA_XCFRAMEWORK` is read from the workspace root.
#[cfg(feature = "llama")]
mod llama {
    use std::env;
    use std::path::Path;

    pub(super) fn link(crate_dir: &Path) {
        println!("cargo::rerun-if-env-changed=GGLIB_LLAMA_XCFRAMEWORK");
        println!("cargo::rerun-if-changed=csrc/gglib_llama.c");
        println!("cargo::rerun-if-changed=csrc/gglib_llama.h");
        let target = |key: &str| env::var(format!("CARGO_CFG_TARGET_{key}")).unwrap_or_default();
        if target("VENDOR") != "apple" {
            return;
        }
        let Some(xcframework) = env::var_os("GGLIB_LLAMA_XCFRAMEWORK") else {
            println!(
                "cargo::warning=the `llama` feature links llama.framework, but \
                 GGLIB_LLAMA_XCFRAMEWORK is unset, so the decoder over llama.h is left out. \
                 `make phone-llama` builds the framework and prints the line to export."
            );
            return;
        };
        // Cargo runs a build script in the crate's folder; a relative path
        // is read from the workspace root, where make and CI run cargo.
        let root = crate_dir.ancestors().nth(2).unwrap_or(crate_dir);
        let xcframework = root.join(xcframework);
        let (os, arch, abi) = (target("OS"), target("ARCH"), target("ABI"));
        let slice = match os.as_str() {
            "macos" => "macos-arm64_x86_64",
            "ios" if abi == "sim" || arch == "x86_64" => "ios-arm64_x86_64-simulator",
            "ios" if abi.is_empty() => "ios-arm64",
            _ => {
                println!(
                    "cargo::warning=llama.xcframework has no slice for {arch}-apple-{os} {abi}, \
                     so the decoder over llama.h is left out."
                );
                return;
            }
        };
        let slice_dir = xcframework.join(slice);
        let headers = slice_dir.join("llama.framework/Headers");
        let llama_h = headers.join("llama.h");
        assert!(
            llama_h.is_file(),
            "GGLIB_LLAMA_XCFRAMEWORK names {} (a relative path is read from the workspace \
             root), which has no {slice}/llama.framework/Headers/llama.h; `make phone-llama` \
             builds the framework at the pin.",
            xcframework.display()
        );
        println!("cargo::rerun-if-changed={}", llama_h.display());

        // A llama.h function the pin renamed, retyped or deprecated is a
        // compile error here, not a warning.
        cc::Build::new()
            .file(crate_dir.join("csrc/gglib_llama.c"))
            .include(crate_dir.join("csrc"))
            .include(&headers)
            .std("c11")
            .warnings(true)
            .extra_warnings(true)
            .flag("-Werror=implicit-function-declaration")
            .flag("-Werror=incompatible-pointer-types")
            .flag("-Werror=int-conversion")
            .flag("-Werror=deprecated-declarations")
            .compile("gglib_llama");

        println!("cargo::rustc-link-search=framework={}", slice_dir.display());
        println!("cargo::rustc-link-lib=framework=llama");
        // A test binary on the Mac finds the framework where it was built;
        // an app embeds it and finds it in its own Frameworks folder.
        if os == "macos" {
            println!("cargo::rustc-link-arg=-Wl,-rpath,{}", slice_dir.display());
        }
        println!("cargo::rustc-cfg={}", super::LINKED);
    }
}
