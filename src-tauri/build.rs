fn main() {
    // Force rebuild when frontend changes (via stamp file)
    println!("cargo:rerun-if-changed=../web_ui/.tauri-stamp");

    tauri_build::build();
}
