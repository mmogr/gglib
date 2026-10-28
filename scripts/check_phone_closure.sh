#!/bin/bash
# check_phone_closure.sh - Keep the crates a phone build compiles clear of
# desktop-only packages
#
# The crates named on the command line are the Makefile's PHONE_CRATES, which
# `make boundaries` passes here and `make ios-check` checks for
# aarch64-apple-ios. For each one this reads the dependency closure cargo
# resolves for aarch64-apple-ios (normal edges at every depth, with the
# crate's default features) and fails on any package named in FORBIDDEN,
# printing the phone crate and one path from it to that package. A package
# matches by its exact name.
#
# FORBIDDEN is what the desktop build links and a phone build must not: the
# host and process crates sysinfo, netdev and nix (sysinfo does not compile
# for aarch64-apple-ios); iroh and modelpipe, the remote-access transport;
# gglib-runtime, which starts llama-server as a child process; and
# gglib-app-services, the desktop's composition of the backend.
#
# It builds nothing. `cargo tree --target` takes the target's cfg from
# `rustc --print cfg`, which needs no standard library for that target, so
# this runs on Linux with no iOS toolchain installed.
#
# Usage: ./scripts/check_phone_closure.sh <crate>...
#
# Exit codes:
#   0 - No phone crate reaches a forbidden package
#   1 - A phone crate reaches one, or cargo tree could not read a crate
#   2 - No crate was named

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

TARGET=aarch64-apple-ios
FORBIDDEN=(sysinfo netdev nix iroh modelpipe gglib-runtime gglib-app-services)

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

log() {
    echo -e "$1"
}

# One line per FORBIDDEN package the closure reaches: the path from the root
# crate to it, as `a -> b -> sysinfo`. Reads `cargo tree --prefix depth`,
# which prints a package's dependencies in full where it first appears and
# marks its later appearances `(*)`, so the first line naming a package ends
# a complete path to it.
forbidden_paths() {
    awk -v forbidden="${FORBIDDEN[*]}" '
        BEGIN {
            n = split(forbidden, names, " ")
            for (i = 1; i <= n; i++) bad[names[i]] = 1
        }
        !/^[0-9]/ { next }
        {
            match($0, /^[0-9]+/)
            depth = substr($0, 1, RLENGTH) + 0
            split(substr($0, RLENGTH + 1), field, " ")
            name = field[1]
            stack[depth] = name
            if ((name in bad) && !(name in seen)) {
                seen[name] = 1
                path = stack[0]
                for (i = 1; i <= depth; i++) path = path " -> " stack[i]
                print path
            }
        }'
}

main() {
    if [[ $# -eq 0 ]]; then
        echo "usage: $0 <crate>..." >&2
        exit 2
    fi

    log "📱 Phone crates (no desktop-only package in their $TARGET closure)"
    local failed=0
    local crate tree paths
    for crate in "$@"; do
        if ! tree=$(cargo tree --locked -e normal --target "$TARGET" -p "$crate" --prefix depth); then
            log "${RED}FAIL${NC}: $crate"
            log "  cargo tree could not read its dependencies"
            failed=1
            continue
        fi
        paths=$(forbidden_paths <<<"$tree")
        if [[ -n "$paths" ]]; then
            log "${RED}FAIL${NC}: $crate"
            while IFS= read -r path; do
                log "  $path"
            done <<<"$paths"
            failed=1
        else
            log "${GREEN}PASS${NC}: $crate"
        fi
    done
    log ""

    if [[ $failed -eq 0 ]]; then
        log "✅ ${GREEN}No phone crate reaches a desktop-only package${NC}"
        exit 0
    else
        log "❌ ${RED}Phone crate closure violations detected${NC} (FORBIDDEN is in scripts/check_phone_closure.sh)"
        exit 1
    fi
}

main "$@"
