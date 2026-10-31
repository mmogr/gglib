#!/usr/bin/env bash
#
# Say whether a change reaches what CI's `ffi-apple` job builds and runs.
#
# Usage: scripts/apple/ffi-inputs-changed.sh <base> <head>
#
# Prints `changed=true` or `changed=false` on stdout, the line a workflow
# step appends to $GITHUB_OUTPUT, and on stderr the inputs it read and the
# files of the change that touch them.
#
# The inputs are every file the job's builds can read:
#
#   - each workspace crate in gglib-ffi's dependency closure (normal and build
#     edges, every feature, every platform) as `cargo tree` resolves it, so a
#     crate gglib-ffi comes to depend on is an input from that change on,
#     with no list here to keep;
#   - crates/build_common.rs, which those crates' build scripts include;
#   - scripts/llama_pin.sh, which the smoke test runs for the pin it expects
#     build_info() to report, and the llama.xcframework build reads;
#   - .github/actions/llama-xcframework, which builds or restores the
#     llama.xcframework every slice links, and
#     .github/actions/pinned-engines/models.txt, which names the model the
#     smoke test serves;
#   - the root Cargo.toml and Cargo.lock, rust-toolchain.toml, the Makefile,
#     scripts/apple/ and .github/workflows/ci.yml.
#
# Every file the closure crates' library code embeds sits inside its crate
# (gglib-core's `assets/` among them) and is an input with it; only a test
# reaches outside, and this job builds no tests. A script scripts/apple/
# runs from elsewhere in the tree is listed by name, as llama_pin.sh is.
#
# CI skips `ffi-apple` when this says false, so an input missing here would
# be a change the job never sees and a green run that says nothing. The
# closure is therefore read rather than written down, and a failure here
# fails scripts/ci_scope.py, the `scope` job, and the run.
#
# Needs cargo (it builds nothing) and a checkout holding both commits.
set -euo pipefail

BASE="${1:?usage: ffi-inputs-changed.sh <base> <head>}"
HEAD="${2:?usage: ffi-inputs-changed.sh <base> <head>}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

# A path dependency prints as `name vX.Y.Z (/abs/path)`; keep the path,
# relative to the workspace root.
crates="$(cargo tree --locked -p gglib-ffi -e normal,build --all-features --target all --prefix none \
    | sed -n "s|.*(${ROOT_DIR}/\([^)]*\)).*|\1|p" | LC_ALL=C sort -u)"
if ! grep -qx 'crates/gglib-ffi' <<<"${crates}"; then
    echo "error: cargo tree did not name crates/gglib-ffi among its own closure:" >&2
    echo "${crates:-  (nothing)}" >&2
    exit 1
fi

inputs=()
while IFS= read -r crate; do
    inputs+=("${crate}")
done <<<"${crates}"
inputs+=(
    crates/build_common.rs
    scripts/llama_pin.sh
    .github/actions/llama-xcframework
    .github/actions/pinned-engines/models.txt
    Cargo.toml
    Cargo.lock
    rust-toolchain.toml
    Makefile
    scripts/apple
    .github/workflows/ci.yml
)
echo "Inputs of the Swift framework:" >&2
printf '  %s\n' "${inputs[@]}" >&2

touched="$(git diff --name-only "${BASE}" "${HEAD}" -- "${inputs[@]}")"
if [[ -n "${touched}" ]]; then
    echo "This change touches:" >&2
    sed 's/^/  /' <<<"${touched}" >&2
    echo "changed=true"
else
    echo "This change touches none of them." >&2
    echo "changed=false"
fi
