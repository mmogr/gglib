#!/usr/bin/env bash
# The TypeScript/CSS file-size ratchet: `check_file_size.sh ts` with its
# baseline, and nothing of its own. Module docs that explain why a file was
# split name this file.
#
# Usage: ./scripts/check_file_complexity.sh [--update]

set -euo pipefail

SCRIPTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPTS_DIR/check_file_size.sh" ts "$SCRIPTS_DIR/ts-complexity-baseline.txt" "$@"
