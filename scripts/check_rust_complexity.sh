#!/usr/bin/env bash
# The Rust file-size ratchet: `check_file_size.sh rust` with its baseline, and
# nothing of its own. Module docs that explain why a file was split name this
# file.
#
# Usage: ./scripts/check_rust_complexity.sh [--update]

set -euo pipefail

SCRIPTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPTS_DIR/check_file_size.sh" rust "$SCRIPTS_DIR/rust-complexity-baseline.txt" "$@"
