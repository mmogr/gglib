#!/bin/bash
# install-llama.sh - Install the pinned llama.cpp
#
# Runs `gglib config llama install` and chooses nothing itself. The command
# installs the pinned release: the download where this machine has one, and
# a source build, for the acceleration it detects, where it has none. This
# script finds the binary to run and answers the command's one question, a
# source build's "Continue?", when nobody is there to.
#
# It exits 1 without installing when it finds no gglib binary, and otherwise
# with the command's own status.
#
# Usage: ./scripts/install-llama.sh    (`make llama-install-auto`)
set -e

# Locate the gglib binary (prefer local builds, then PATH).
find_gglib() {
    if [ -f "./target/release/gglib" ]; then
        echo "./target/release/gglib"
    elif [ -f "./target/debug/gglib" ]; then
        echo "./target/debug/gglib"
    elif command -v gglib >/dev/null 2>&1; then
        echo "gglib"
    else
        echo ""
    fi
}

GGLIB_BIN=$(find_gglib)
if [ -z "$GGLIB_BIN" ]; then
    echo "Error: gglib binary not found. Please build it first."
    exit 1
fi

echo "Running: $GGLIB_BIN config llama install"
if [ -t 0 ]; then
    "$GGLIB_BIN" config llama install
else
    # No terminal to answer the command's "Continue?" (make run from a script
    # or CI). This script was run to install, so answer yes: end of input
    # alone would cancel, and the make target would fail.
    printf 'y\n' | "$GGLIB_BIN" config llama install
fi
