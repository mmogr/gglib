use std::env;
use std::fs;
use std::path::Path;

include!("../build_common.rs");

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    process_readme_for_rustdoc(&crate_dir);
}
