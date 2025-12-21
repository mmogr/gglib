use std::process::Command;

fn main() {
    // Force rebuild when frontend changes (via stamp file)
    println!("cargo:rerun-if-changed=../web_ui/.tauri-stamp");
    
    // Capture git commit hash at build time
    let git_hash = get_git_commit_hash();
    println!("cargo:rustc-env=GIT_COMMIT_HASH={}", git_hash);
    
    // Cache-busting: rerun when git state changes
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads/");
    
    tauri_build::build();
}

fn get_git_commit_hash() -> String {
    // Try to get short commit hash (7 chars)
    let hash_result = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output();
    
    let hash = match hash_result {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => {
            // Fallback for CI or non-git environments
            return env!("CARGO_PKG_VERSION").to_string();
        }
    };
    
    // Check for uncommitted changes (dirty state)
    let diff_result = Command::new("git")
        .args(["diff", "--quiet"])
        .status();
    
    let is_dirty = match diff_result {
        Ok(status) => !status.success(), // exit code 1 means there are changes
        _ => false, // if git command fails, assume clean
    };
    
    if is_dirty {
        format!("{}-dirty", hash)
    } else {
        hash
    }
}
