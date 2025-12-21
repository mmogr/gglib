use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=README.md");

    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let readme_path = Path::new(&crate_dir).join("README.md");

    let content = if readme_path.exists() {
        fs::read_to_string(readme_path).unwrap()
    } else {
        return;
    };

    // Transform for rustdoc: strip 'src/' prefix and '.rs' extension
    let rustdoc_content = content.replace("](src/", "](").replace(".rs)", ")");

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("README_GENERATED.md");
    fs::write(dest_path, rustdoc_content).unwrap();
}
