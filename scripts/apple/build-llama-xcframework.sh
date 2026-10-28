#!/usr/bin/env bash
#
# Build llama.cpp's own llama.xcframework at the pin, for the phone engine.
#
#   build-llama-xcframework.sh
#
# The framework is what upstream's build-xcframework.sh makes, run unchanged
# in a shallow clone of the tag scripts/llama_pin.sh prints, for three of its
# builds: ios-device, ios-sim and macos. Upstream's released
# llama-<tag>-xcframework.zip is not used because it has no iOS simulator
# slice. Each slice is a dynamic llama.framework with llama.h and ggml's
# headers.
#
# Output, in target/apple/llama-<pin>/:
#   llama.xcframework            ios-arm64, ios-arm64_x86_64-simulator,
#                                macos-arm64_x86_64
#   llama-<pin>.xcframework.zip  llama.xcframework at the zip's root
#   pin.txt                      the tag it was built from, written last
#
# A folder holding llama.xcframework and a pin.txt naming the pin is kept: the
# build is skipped and only a missing zip is made again. Otherwise the clone
# and upstream's build trees go in target/apple/llama-<pin>.work/, which is
# removed once the folder is complete and kept when the build fails, for
# upstream's build_*.log files. The last line printed is
# GGLIB_LLAMA_XCFRAMEWORK=<path>, the variable the phone engine's build reads.
#
# Needs macOS with Xcode and CMake. A cold build compiles llama.cpp three
# times, one platform after another; scripts/apple/check-llama-xcframework.sh
# checks the result.

set -euo pipefail

[ $# -eq 0 ] || { sed -n '5p' "$0" | sed 's/^# *//' >&2; exit 2; }

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "error: building llama.xcframework needs Xcode, so it runs on macOS only." >&2
    exit 1
fi
for tool in git cmake xcrun zip; do
    command -v "$tool" >/dev/null || { echo "error: $tool is not on PATH." >&2; exit 1; }
done

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PIN="$("$ROOT_DIR/scripts/llama_pin.sh")"
# The repository gglib-runtime's source builds clone (LLAMA_SOURCE in
# crates/gglib-runtime/src/llama/install/mod.rs), cloned the same way: the
# tag alone, with no submodules.
LLAMA_URL=https://github.com/ggml-org/llama.cpp
PLATFORMS=(ios-device ios-sim macos)

OUT="$ROOT_DIR/target/apple/llama-$PIN"
WORK="$OUT.work"
ZIP_NAME="llama-$PIN.xcframework.zip"

zip_framework() {
    local dir="$1"
    rm -f "$dir/$ZIP_NAME"
    # -y keeps the macOS slice's Versions/Current symlinks as links.
    (cd "$dir" && zip -qry "$ZIP_NAME" llama.xcframework)
}

if [[ -d "$OUT/llama.xcframework" && -f "$OUT/pin.txt" && "$(cat "$OUT/pin.txt")" == "$PIN" ]]; then
    echo "llama.xcframework at $PIN is already built in $OUT"
    [[ -f "$OUT/$ZIP_NAME" ]] || zip_framework "$OUT"
else
    echo "Building llama.xcframework at $PIN for ${PLATFORMS[*]}"
    rm -rf "$WORK"
    mkdir -p "$WORK"
    git -c advice.detachedHead=false clone --quiet --depth 1 --branch "$PIN" \
        "$LLAMA_URL" "$WORK/llama.cpp"
    # Upstream's script reads this one setting from the environment; its own
    # default, the Metal library embedded in the binary as source, is the one
    # built, and it needs no Metal compiler.
    unset GGML_METAL_EMBED_LIBRARY
    (cd "$WORK/llama.cpp" && ./build-xcframework.sh "${PLATFORMS[@]}")

    mkdir -p "$WORK/out"
    mv "$WORK/llama.cpp/build-apple/llama.xcframework" "$WORK/out/"
    zip_framework "$WORK/out"
    printf '%s\n' "$PIN" > "$WORK/out/pin.txt"
    rm -rf "$OUT"
    mv "$WORK/out" "$OUT"
    rm -rf "$WORK"
    echo "Built llama.xcframework at $PIN in $((SECONDS / 60))m$((SECONDS % 60))s"
fi

echo "GGLIB_LLAMA_XCFRAMEWORK=$OUT/llama.xcframework"
