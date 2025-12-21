use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Monitor README.md for changes so build reruns when it's updated
    println!("cargo:rerun-if-changed=README.md");

    // Get paths
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let readme_path = Path::new(&crate_dir).join("README.md");

    // Read the original README (GitHub/VS Code version with src/ links)
    let content = if readme_path.exists() {
        fs::read_to_string(readme_path).unwrap()
    } else {
        // Graceful fallback for crates without README
        String::new()
    };

    // Transform for rustdoc:
    // 1. Strip 'src/' prefix from links so rustdoc can resolve modules
    // 2. Strip '.rs' extension so links go to modules, not files
    let rustdoc_content = content
        .replace("](src/", "](") // src/handlers/ -> handlers/
        .replace(".rs)", ")"); // add.rs -> add (module link)

    // Write to OUT_DIR (cargo's build artifact directory)
    // This keeps the generated file out of the source tree
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("README_GENERATED.md");
    fs::write(dest_path, rustdoc_content).unwrap();

    // Keep existing workspace root logic
    let workspace_root = std::path::Path::new(&crate_dir)
        .parent() // crates/
        .and_then(|p| p.parent()) // workspace root
        .expect("Could not determine workspace root")
        .to_string_lossy()
        .to_string();

    println!("cargo:rustc-env=GGLIB_REPO_ROOT={}", workspace_root);
}
