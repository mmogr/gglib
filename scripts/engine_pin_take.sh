#!/usr/bin/env bash
# Take a recording job's handover into this checkout, as data.
#
#   engine_pin_take.sh <handover> <pin file>
#
# The Engine pins workflow (.github/workflows/engine-pins.yml) records a
# candidate in one job, which builds gglib and runs the upstream engines and a
# model, and proposes it from another, which holds the write token and runs
# none of them. The first hands over <handover>/files/ (the pin file and
# contracts/upstream/, at their paths in the repository) and
# <handover>/recorder.txt (`ok`, or why the recorder did not run). On a
# llama.cpp bump a job between them, on macOS and as read-only as the first,
# takes that handover, records the in-process engine beside it, and hands
# over in the same form. This script is how either takes a handover: it
# refuses anything but regular files at those paths, mirrors
# contracts/upstream/ from the handover, keeps the hand-kept approvals.json
# and README.md from this checkout, copies the pin file, and prints the
# recorder line made safe for the body. It runs from the checkout's root, and
# never runs anything the handover holds.

set -euo pipefail

[ $# -eq 2 ] || { echo "usage: engine_pin_take.sh <handover> <pin file>" >&2; exit 2; }
HANDOVER=$1 FILE=$2
FILES=$HANDOVER/files
UPSTREAM=contracts/upstream
HAND_KEPT=("$UPSTREAM/llama/approvals.json" "$UPSTREAM/README.md")

refuse() { echo "::error title=the handover is refused::$1" >&2; exit 1; }

[ -d "$FILES" ] && [ ! -L "$FILES" ] || refuse "it has no files/ folder"
[ -f "$FILES/$FILE" ] || refuse "it has no $FILE"
[ -f "$FILES/$UPSTREAM/llama/observed.json" ] || refuse "it has no $UPSTREAM/llama/observed.json"

# Every entry is a folder or a regular file; no symlink, device or pipe.
odd=$(find "$FILES" -mindepth 1 ! -type d ! -type f -print | head -5)
[ -z "$odd" ] || refuse "it holds something that is not a regular file: ${odd//$'\n'/, }"

# Every file is the pin file or under contracts/upstream/, by a plain path.
taken=()
while IFS= read -r -d '' path; do
  rel=${path#"$FILES"/}
  [[ $rel =~ ^[A-Za-z0-9._/-]+$ ]] || refuse "a path has characters outside [A-Za-z0-9._/-]"
  [[ /$rel/ != */../* && /$rel/ != */./* && $rel != *//* ]] || refuse "$rel is not a plain path"
  [[ $rel == "$FILE" || $rel == "$UPSTREAM"/* ]] || refuse "$rel is neither $FILE nor under $UPSTREAM/"
  taken+=("$rel")
done < <(find "$FILES" -mindepth 1 -type f -print0)

rm -rf "$UPSTREAM"
for rel in "${taken[@]}"; do
  mkdir -p "$(dirname "$rel")"
  cp "$FILES/$rel" "$rel"
done
git checkout HEAD -- "${HAND_KEPT[@]}"

# The recorder's line: printable, no backticks, at most 200 characters.
reason=$(head -1 "$HANDOVER/recorder.txt" 2>/dev/null | tr -cd '[:print:]' | tr -d '`' | cut -c1-200 || true)
echo "${reason:-the recorder left no reason}"
