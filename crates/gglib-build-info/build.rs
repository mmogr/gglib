use std::{
    env,
    path::{Path, PathBuf},
};

use vergen_gix::{Emitter, GixBuilder};

fn main() {
    // Always rerun when this build script changes.
    println!("cargo:rerun-if-changed=build.rs");

    // Allow CI or packagers to provide a SHA without any git probing.
    println!("cargo:rerun-if-env-changed=GGLIB_BUILD_SHA_SHORT");

    if let Some(override_sha) = env::var("GGLIB_BUILD_SHA_SHORT").ok().and_then(normalize_sha_short) {
        emit_vergen_fallbacks(Some(&override_sha));
        return;
    }

    // Best-effort git probing via vergen-gix, but NEVER fail the build.
    // If no repo is found, we emit explicit fallbacks so `env!()` never fails.
    let Some(repo_root) = find_repo_root(Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap_or_default())) else {
        emit_vergen_fallbacks(None);
        return;
    };

    let git = match GixBuilder::default()
        .repo_path(Some(repo_root))
        .sha(true) // short SHA
        .dirty(false)
        .build()
    {
        Ok(git) => git,
        Err(err) => {
            println!("cargo:warning=gglib-build-info: vergen-gix config failed: {err}");
            emit_vergen_fallbacks(None);
            return;
        }
    };

    if let Err(err) = Emitter::default().add_instructions(&git).and_then(|e| e.emit()) {
        println!("cargo:warning=gglib-build-info: vergen-gix emit failed: {err}");
        emit_vergen_fallbacks(None);
    }
}

fn emit_vergen_fallbacks(sha_short: Option<&str>) {
    // These are the env vars the crate uses via `env!()`.
    // They MUST always be set, or compilation will fail.
    let sha = sha_short.unwrap_or("unknown");
    println!("cargo:rustc-env=VERGEN_GIT_SHA={sha}");
    println!("cargo:rustc-env=VERGEN_GIT_DIRTY=false");
}

fn normalize_sha_short(raw: String) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = if trimmed.len() >= 7 {
        &trimmed[..7]
    } else {
        trimmed
    };

    if candidate.len() == 7 && candidate.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(candidate.to_string())
    } else {
        None
    }
}

fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}
