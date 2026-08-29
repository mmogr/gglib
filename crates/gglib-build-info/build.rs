use std::{
    env, fs,
    path::{Path, PathBuf},
};

use vergen_gix::{Emitter, GixBuilder};

include!("../build_common.rs");

/// How many hex characters of the commit id every gglib surface shows.
///
/// Must match `SHA_LEN` in `src/lib.rs`, which is what the crate's tests
/// assert against. Twelve is the width the build fingerprint has always used;
/// seven stopped being unambiguous for this repository some time ago.
const SHA_LEN: usize = 12;

/// The env var carrying the abbreviated commit id to `src/lib.rs`.
const SHA_VAR: &str = "GGLIB_GIT_SHA";

/// The env var carrying the commit id plus a `-dirty` marker.
const FINGERPRINT_VAR: &str = "GGLIB_FINGERPRINT";

/// What both vars say when there was no git to ask.
const UNAVAILABLE: &str = "unknown";

fn main() {
    // Always rerun when this build script changes.
    println!("cargo:rerun-if-changed=build.rs");

    // Process README for rustdoc (uses shared build_common.rs)
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    process_readme_for_rustdoc(&manifest_dir);

    // Allow CI or packagers to provide a SHA without any git probing.
    println!("cargo:rerun-if-env-changed=GGLIB_BUILD_SHA_SHORT");

    // An empty value is how a caller says "no override" — `VAR= cargo build`
    // — so it falls through to probing without complaint.
    if let Some(raw) = env::var("GGLIB_BUILD_SHA_SHORT")
        .ok()
        .filter(|s| !s.trim().is_empty())
    {
        match normalize_sha(&raw) {
            Some(sha) => {
                emit_build_identity(Some(&sha), false);
                return;
            }
            None => {
                // The old code discarded a malformed override in silence and
                // fell through to git probing, so a packager who mistyped it
                // got a build that looked fine and reported the wrong commit.
                println!(
                    "cargo:warning=gglib-build-info: ignoring GGLIB_BUILD_SHA_SHORT={raw:?}; \
                     expected at least {SHA_LEN} hex characters of a commit id"
                );
            }
        }
    }

    // Best-effort git probing via vergen-gix, but NEVER fail the build.
    // If no repo is found, we emit explicit fallbacks so `env!()` never fails.
    let Some(repo_root) = find_repo_root(Path::new(
        &env::var("CARGO_MANIFEST_DIR").unwrap_or_default(),
    )) else {
        emit_build_identity(None, false);
        return;
    };

    // `sha(false)` asks for the *full* commit id rather than gix's abbreviated
    // one. gix abbreviates the way git does — from the size of the object
    // database — so its width grows with the repository, and this crate spent
    // a release printing no commit at all because a hard-coded seven-character
    // check stopped matching an eight-character SHA. Taking the full id and
    // cutting it ourselves makes the width a property of this file instead of
    // a property of whoever's clone did the build.
    //
    // `dirty(false)` excludes untracked files, matching the
    // `--untracked-files=no` of the fingerprint this replaces.
    let git = match GixBuilder::default()
        .repo_path(Some(repo_root))
        .sha(false)
        .dirty(false)
        .build()
    {
        Ok(git) => git,
        Err(err) => {
            println!("cargo:warning=gglib-build-info: vergen-gix config failed: {err}");
            emit_build_identity(None, false);
            return;
        }
    };

    // Captured rather than emitted straight to stdout: vergen writes the
    // instructions itself, and the SHA has to be cut to width on the way past.
    // The rest of the stream — notably the `rerun-if-changed` lines for
    // `.git/HEAD` *and* the resolved ref — is passed through untouched. Both
    // matter: a commit on the current branch rewrites the ref file and leaves
    // `.git/HEAD` alone, so watching HEAD by itself reports a stale commit.
    let mut captured = Vec::new();
    if let Err(err) = Emitter::default()
        .add_instructions(&git)
        .and_then(|e| e.emit_to(&mut captured))
    {
        println!("cargo:warning=gglib-build-info: vergen-gix emit failed: {err}");
        emit_build_identity(None, false);
        return;
    }

    match String::from_utf8(captured) {
        Ok(instructions) => forward_instructions(&instructions),
        Err(err) => {
            println!("cargo:warning=gglib-build-info: vergen-gix emitted non-UTF-8: {err}");
            emit_build_identity(None, false);
        }
    }
}

/// Replay vergen's instructions, taking the SHA and dirty flag out of the
/// stream and re-emitting them under this crate's own names.
fn forward_instructions(instructions: &str) {
    const SHA_PREFIX: &str = "cargo:rustc-env=VERGEN_GIT_SHA=";
    const DIRTY_PREFIX: &str = "cargo:rustc-env=VERGEN_GIT_DIRTY=";

    let mut sha = None;
    let mut saw_sha_line = false;
    let mut dirty = false;

    for line in instructions.lines() {
        if let Some(value) = line.strip_prefix(SHA_PREFIX) {
            saw_sha_line = true;
            sha = normalize_sha(value);
        } else if let Some(value) = line.strip_prefix(DIRTY_PREFIX) {
            dirty = value.trim() == "true";
        } else {
            println!("{line}");
        }
    }

    // Matching a prefix against another crate's output is a coupling, and the
    // failure mode if it ever stops matching — cargo changing `cargo:` to
    // `cargo::`, vergen renaming a key — is that the SHA quietly becomes
    // "unknown" and every surface goes back to printing a bare version. That
    // is precisely the silent degradation this crate is being fixed to stop
    // doing, so say it out loud instead.
    if !saw_sha_line {
        println!(
            "cargo:warning=gglib-build-info: vergen-gix emitted no {SHA_PREFIX} line; \
             the build will report an unknown commit"
        );
    }

    emit_build_identity(sha.as_deref(), dirty);
}

/// Emit the two vars `src/lib.rs` reads. Every path through `main` ends here,
/// because `env!()` is a compile error when the variable is absent — a crate
/// that fails to build outside a git checkout would be a worse bug than the
/// one this file exists to fix.
fn emit_build_identity(sha: Option<&str>, dirty: bool) {
    let sha = sha.unwrap_or(UNAVAILABLE);
    println!("cargo:rustc-env={SHA_VAR}={sha}");

    let fingerprint = if sha == UNAVAILABLE {
        UNAVAILABLE.to_owned()
    } else if dirty {
        format!("{sha}-dirty")
    } else {
        sha.to_owned()
    };
    println!("cargo:rustc-env={FINGERPRINT_VAR}={fingerprint}");
}

/// Cut a commit id down to [`SHA_LEN`], or reject it.
///
/// Used for both the id vergen probes and the one a packager supplies, so the
/// two cannot disagree about width. A shorter value used to be accepted and
/// silently produced a narrower SHA than every other build of the same commit,
/// which is the inconsistency this crate is being changed to remove.
///
/// Anything that is not a hex id is rejected rather than truncated into
/// something that looks like a commit — vergen substitutes the literal
/// `VERGEN_IDEMPOTENT_OUTPUT` when `SOURCE_DATE_EPOCH` or `VERGEN_IDEMPOTENT`
/// is set, which packaging builds do.
fn normalize_sha(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.len() < SHA_LEN || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    // `get` rather than `[..n]`: the length check above is on bytes and the
    // hex check makes a split mid-character impossible, but indexing a `str`
    // panics where this simply declines.
    trimmed.get(..SHA_LEN).map(str::to_owned)
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
