#!/usr/bin/env bash
#
# Package a built GglibFFI.xcframework and its Swift binding as the two
# release assets an app takes, then check each one as the app meets it.
#
# Usage: scripts/apple/package-release.sh <version> <dist-dir>
#
# Writes exactly two files into <dist-dir>:
#
#   GglibFFI-<version>.xcframework.zip  GglibFFI.xcframework at the archive's
#                                       root, where SwiftPM looks for a binary
#                                       target's framework
#   GglibFFI-swift-<version>.zip        gglib_ffi.swift, GglibFFI.h and
#                                       module.modulemap as the build generated
#                                       them, and binding.json
#
# binding.json names what the framework's build_info() reports, under the
# same field names:
#
#   {"gglib_version": "…", "uniffi_version": "…", "llama_pin": "…"}
#
# so an app can tell which framework a vendored binding belongs to without
# loading either.
#
# <version> names the files and nothing else. release.yml passes the version
# it releases, read from the `[workspace.package]` line gglib-ffi inherits its
# own version from; binding.json's gglib_version is read from the crate, since
# it describes the bytes.
#
# Reads target/apple/ (or $CARGO_TARGET_DIR/apple/) as build-xcframework.sh
# leaves it, so `make ffi-xcframework` comes first. Refuses before packaging
# anything when the Swift there is not the committed crates/gglib-ffi/swift/:
# an app vendors the Swift from the release, and the tag must show the same
# bytes. Then checks:
#
#   - the framework archive holds GglibFFI.xcframework/ and nothing beside it,
#     and unzips to a framework check-slices.sh passes, so the symlinks of the
#     macOS slice's versioned bundle survive the round trip;
#   - the binding archive holds its four files and nothing else;
#   - SwiftPM takes the framework archive as a binary target: a scratch package
#     naming it by path, with the Swift from the binding archive, builds and
#     runs, and build_info() answers what binding.json says;
#   - `swift package compute-checksum` of the framework archive, the checksum
#     an app pins, is the SHA-256 the release's SHA256SUMS lists for it.
#
# Needs Xcode, so macOS only. Boots no simulator.
set -euo pipefail

usage="usage: package-release.sh <version> <dist-dir>"
VERSION="${1:?${usage}}"
DIST_ARG="${2:?${usage}}"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "error: needs Swift and the macOS slice, so macOS only." >&2
    exit 1
fi

# The version becomes part of two file names, so anything but a version is
# refused rather than written somewhere unexpected.
if [[ ! "${VERSION}" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
    echo "error: '${VERSION}' is not a version (X.Y.Z, or X.Y.Z-suffix)." >&2
    exit 1
fi

# Resolved before the cd below, so a relative <dist-dir> is the caller's.
mkdir -p "${DIST_ARG}"
DIST="$(cd "${DIST_ARG}" && pwd)"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

NAME="GglibFFI"
TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT_DIR}/target}"
APPLE_DIR="${TARGET_DIR}/apple"
XCFRAMEWORK="${APPLE_DIR}/${NAME}.xcframework"
SWIFT_DIR="${APPLE_DIR}/swift"
COMMITTED_SWIFT="crates/gglib-ffi/swift"
WORK_DIR="${APPLE_DIR}/release-check"
FRAMEWORK_ZIP="${DIST}/${NAME}-${VERSION}.xcframework.zip"
SWIFT_ZIP="${DIST}/${NAME}-swift-${VERSION}.zip"
SWIFT_FILES=(gglib_ffi.swift "${NAME}.h" module.modulemap)

fail() {
    echo "error: $1" >&2
    exit 1
}

for input in "${XCFRAMEWORK}" "${SWIFT_DIR}"; do
    [[ -d "${input}" ]] || fail "${input} not found. Run \`make ffi-xcframework\` first."
done

if ! diff -rq "${SWIFT_DIR}" "${COMMITTED_SWIFT}"; then
    fail "the Swift this build generated is not the committed ${COMMITTED_SWIFT}. Run \`make ffi-binding\` and commit the result."
fi

# What build_info() reports, read from the sources the build was made from,
# as swift-smoke.sh reads them. The SwiftPM check below holds the framework in
# the archive to the same three values.
GGLIB_VERSION="$(cargo pkgid --locked -p gglib-ffi | sed 's/.*[@#]//')"
PIN="$("${ROOT_DIR}/scripts/llama_pin.sh")"
UNIFFI="$(awk '
    /^\[\[package\]\]/ { name = "" }
    /^name = / { name = $3 }
    /^version = / && name == "\"uniffi\"" { gsub(/"/, "", $3); print $3 }
' Cargo.lock)"
[[ -n "${UNIFFI}" ]] || fail "Cargo.lock names no uniffi version."

rm -rf "${WORK_DIR}"
mkdir -p "${WORK_DIR}"
rm -f "${FRAMEWORK_ZIP}" "${SWIFT_ZIP}"

echo "==> Packaging ${FRAMEWORK_ZIP##*/}"
# From inside target/apple, so every entry starts at GglibFFI.xcframework/.
# -y stores a symlink as a symlink: the macOS slice's root entries and
# Versions/Current are links, and a copy of each target in its place is a
# bundle macOS does not load.
(cd "${APPLE_DIR}" && zip -qry "${FRAMEWORK_ZIP}" "${NAME}.xcframework")

echo "==> Packaging ${SWIFT_ZIP##*/}"
jq -n --arg gglib "${GGLIB_VERSION}" --arg uniffi "${UNIFFI}" --arg pin "${PIN}" \
    '{gglib_version: $gglib, uniffi_version: $uniffi, llama_pin: $pin}' \
    >"${WORK_DIR}/binding.json"
# -j keeps the bare names: the four files sit at the archive's root.
zip -qj "${SWIFT_ZIP}" "${SWIFT_FILES[@]/#/${SWIFT_DIR}/}" "${WORK_DIR}/binding.json"

echo "==> Checking each archive as an app meets it"
roots="$(unzip -Z1 "${FRAMEWORK_ZIP}" | cut -d/ -f1 | LC_ALL=C sort -u | tr '\n' ' ')"
[[ "${roots}" == "${NAME}.xcframework " ]] \
    || fail "${FRAMEWORK_ZIP##*/} holds '${roots% }' at its root, wanted ${NAME}.xcframework alone."
echo "ok  ${FRAMEWORK_ZIP##*/} holds ${NAME}.xcframework at its root and nothing beside it"

entries="$(unzip -Z1 "${SWIFT_ZIP}" | LC_ALL=C sort | tr '\n' ' ')"
wanted="$(printf '%s\n' "${SWIFT_FILES[@]}" binding.json | LC_ALL=C sort | tr '\n' ' ')"
[[ "${entries}" == "${wanted}" ]] \
    || fail "${SWIFT_ZIP##*/} holds '${entries% }', wanted '${wanted% }'."
echo "ok  ${SWIFT_ZIP##*/} holds ${wanted% }"

unzip -q "${FRAMEWORK_ZIP}" -d "${WORK_DIR}/unzipped"
"${ROOT_DIR}/scripts/apple/check-slices.sh" "${WORK_DIR}/unzipped/${NAME}.xcframework"

# binding.json as the archive carries it, so the check is of the shipped file.
json="$(unzip -p "${SWIFT_ZIP}" binding.json)"
want_gglib="$(jq -er .gglib_version <<<"${json}")"
want_uniffi="$(jq -er .uniffi_version <<<"${json}")"
want_pin="$(jq -er .llama_pin <<<"${json}")"
echo "ok  binding.json: $(jq -c . <<<"${json}")"

# A package shaped the way an app consumes the release: the archive itself as
# the binary target, which SwiftPM unpacks as it would a downloaded one, and
# only gglib_ffi.swift from the binding archive in a source target of its own.
CONSUMER="${WORK_DIR}/consumer"
mkdir -p "${CONSUMER}/Sources/GGLib" "${CONSUMER}/Sources/Check"
cp "${FRAMEWORK_ZIP}" "${CONSUMER}/"
unzip -q "${SWIFT_ZIP}" gglib_ffi.swift -d "${CONSUMER}/Sources/GGLib"

cat >"${CONSUMER}/Package.swift" <<SWIFT
// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "GglibFFIReleaseCheck",
    platforms: [.macOS(.v26)],
    targets: [
        .binaryTarget(name: "${NAME}", path: "${FRAMEWORK_ZIP##*/}"),
        .target(name: "GGLib", dependencies: ["${NAME}"]),
        .executableTarget(name: "Check", dependencies: ["GGLib"]),
    ],
    swiftLanguageModes: [.v6]
)
SWIFT

cat >"${CONSUMER}/Sources/Check/main.swift" <<SWIFT
import Foundation
import GGLib

// The first call also runs uniffi's own checks of the binding against the
// binary: its contract version and every API checksum.
let info = buildInfo()
let want = BuildInfo(gglibVersion: "${want_gglib}", uniffiVersion: "${want_uniffi}", llamaPin: "${want_pin}")
guard info == want else {
    FileHandle.standardError.write(Data("release check: build_info() is \(info), and binding.json says \(want)\n".utf8))
    exit(1)
}
print("ok  the framework SwiftPM unpacked from the archive answers what binding.json says")
SWIFT

echo "==> Building a package that takes the archive as its binary target"
(cd "${CONSUMER}" && swift run --quiet Check)

# SwiftPM's checksum is the SHA-256 of the file, so the line the release's
# hash step writes for the archive is the one an app's Package.swift pins.
sha256="$(shasum -a 256 "${FRAMEWORK_ZIP}" | cut -d' ' -f1)"
checksum="$(cd "${WORK_DIR}" && swift package compute-checksum "${FRAMEWORK_ZIP}")"
[[ "${checksum}" == "${sha256}" ]] \
    || fail "swift package compute-checksum says ${checksum}, and SHA-256 ${sha256}."
echo "ok  swift package compute-checksum ${FRAMEWORK_ZIP##*/} is its SHA-256, ${checksum}"

echo
echo "Packaged into ${DIST}:"
for zip in "${FRAMEWORK_ZIP}" "${SWIFT_ZIP}"; do
    printf '  %s  %s bytes\n' "${zip##*/}" "$(stat -f %z "${zip}")"
done
