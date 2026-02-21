use std::env;
use std::fs;
use std::path::Path;

include!("../build_common.rs");

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    process_readme_for_rustdoc(&crate_dir);

    // ONNX Runtime (via sherpa-onnx-sys) uses ETW telemetry on Windows, which
    // requires Advapi32. The sherpa-onnx-sys build script does not always emit
    // this directive, so we declare it here to ensure the linker can resolve
    // EventWriteTransfer / EventRegister / EventUnregister.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=Advapi32");
    }
}
