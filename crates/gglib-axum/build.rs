use std::env;
use std::fs;
use std::path::Path;

include!("../build_common.rs");

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    process_readme_for_rustdoc(&crate_dir);

    // The whole invalidation story for the embedded dashboard (`src/ui.rs`).
    //
    // rust-embed emits no `cargo::rerun-if-changed` of its own — a proc macro
    // cannot, absent the unstable `tracked_path` API. What it does give is an
    // `include_bytes!` per file that existed at expansion time, which rustc
    // records; that covers **in-place edits only**. Vite content-hashes its
    // filenames and wipes the directory on each build (`emptyOutDir`), so a
    // frontend change is always an add plus a remove and never an in-place
    // edit. The one thing rust-embed tracks for free is therefore the one case
    // that never happens here.
    //
    // Cargo scans a directory path recursively (`cargo-util`'s
    // `mtime_recursive` walks it with no depth limit and takes the max mtime),
    // so this covers `web_ui/assets/` where every hashed chunk lives. It also
    // catches deletions, because the walk yields directory entries and a
    // removal bumps the parent's mtime.
    //
    // Deliberately unguarded by an `exists()` check. `build_common.rs` above
    // already emits `rerun-if-changed` lines, and once a build script emits
    // any, Cargo stops scanning the package directory and honours only that
    // set — so a guarded line would never be registered on the run where
    // `web_ui/` first appears, and the build would keep an empty asset set
    // permanently rather than for one build. The cost of leaving it unguarded
    // is that a missing path reads as always-dirty. Measured: with no
    // `web_ui/`, every `cargo check` reports `Dirty gglib-axum: the file
    // .../web_ui is missing` and re-runs rustc for this crate *and* for both
    // reverse dependencies that link it — `gglib-cli` and `gglib-app` — which
    // is three rustc units per invocation, never converging. It is compile
    // time only: with no `web_ui/` there is nothing to be stale about.
    println!("cargo::rerun-if-changed=../../web_ui");
}
