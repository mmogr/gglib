#!/usr/bin/env bash
#
# A stable release ships exactly the code of its last release candidate.
#
#   scripts/check_rc_promotion.sh
#
# Reads the version from the working tree's Cargo.toml. For a stable X.Y.Z it
# finds the newest vX.Y.Z-rc.N tag and requires every tracked file to equal
# that tag's copy once the RC's version string is replaced by X.Y.Z. The
# version bump is therefore the only change a promotion can carry: a fix that
# lands after the RC, or a dependency moved in Cargo.lock, needs an rc.N+1.
#
# A stable version with no RC tag fails too. Cutting rc.1 and promoting it is
# two runs of the Bump Version workflow, and it means every stable release has
# been out as a prerelease first.
#
# Run in two places: bump-version.yml, before it opens a promotion PR, and
# release.yml's check-version, before it tags. The second is the guard; the
# first only saves a merged PR that the release would then refuse.
#
# A prerelease version passes untouched.
set -euo pipefail

cd "$(dirname "$0")/.."

VERSION="$(grep -m1 '^version = ' Cargo.toml | sed -E 's/version = "(.*)"/\1/')"
if [[ "$VERSION" == *-* ]]; then
  echo "✓ $VERSION is a prerelease; nothing to check"
  exit 0
fi

RC_TAG="$(git tag -l "v${VERSION}-rc.*" | sort -V | tail -n1)"
if [ -z "$RC_TAG" ]; then
  echo "::error::$VERSION has no release candidate. Run Bump Version with 'rc-patch', 'rc-minor' or 'rc-major' to cut ${VERSION}-rc.1, let it release, then 'promote'."
  exit 1
fi
RC_VERSION="${RC_TAG#v}"
RC_PATTERN="$(printf '%s' "$RC_VERSION" | sed 's/[.[\*^$/]/\\&/g')"

echo "Comparing the working tree at $VERSION against $RC_TAG..."

bad=0
while IFS= read -r file; do
  if ! git cat-file -e "$RC_TAG:$file" 2>/dev/null; then
    echo "  ✗ $file is not in $RC_TAG"
    bad=$((bad + 1))
  elif [ ! -e "$file" ]; then
    echo "  ✗ $file is in $RC_TAG but not here"
    bad=$((bad + 1))
  elif ! git show "$RC_TAG:$file" | sed "s/$RC_PATTERN/$VERSION/g" | cmp -s - "$file"; then
    echo "  ✗ $file differs from $RC_TAG by more than the version"
    git show "$RC_TAG:$file" | sed "s/$RC_PATTERN/$VERSION/g" \
      | diff -u --label "$RC_TAG:$file" --label "$file" - "$file" | head -n 20 | sed 's/^/      /' || true
    bad=$((bad + 1))
  fi
done < <(git diff --no-renames --name-only "$RC_TAG" --)

if [ "$bad" -ne 0 ]; then
  echo "::error::$VERSION is not $RC_TAG with its version changed: $bad file(s) differ. Cut another release candidate (Bump Version, 'next-rc') and promote that."
  exit 1
fi
echo "✓ $VERSION is $RC_TAG with only its version changed"
