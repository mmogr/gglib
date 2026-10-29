#!/usr/bin/env bash
#
# Package a built GglibFFI.xcframework, its Swift binding and the
# llama.xcframework it links as the three release assets an app takes, then
# check each one as the app meets it.
#
# Usage: scripts/apple/package-release.sh <version> <dist-dir>
#
# Writes exactly three files into <dist-dir>:
#
#   GglibFFI-<version>.xcframework.zip  GglibFFI.xcframework at the archive's
#                                       root, where SwiftPM looks for a binary
#                                       target's framework
#   GglibFFI-swift-<version>.zip        gglib_ffi.swift, GglibFFI.h and
#                                       module.modulemap as the build generated
#                                       them, and binding.json
#   llama-<version>.xcframework.zip     llama.cpp's own llama.xcframework at
#                                       the pin, which every GglibFFI slice
#                                       links: the zip
#                                       build-llama-xcframework.sh made of it
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
# leaves it, so `make ffi-xcframework` comes first, and the llama.xcframework
# it was built against, with the zip and pin.txt beside it: the one
# GGLIB_LLAMA_XCFRAMEWORK names, or else target/apple/llama-<pin>/. Refuses
# before packaging anything when the Swift there is not the committed
# crates/gglib-ffi/swift/ (an app vendors the Swift from the release, and the
# tag must show the same bytes), or when pin.txt names another pin. Then
# checks:
#
#   - the framework archive holds GglibFFI.xcframework/ and nothing beside it,
#     and unzips to a framework check-slices.sh passes, so the symlinks of the
#     macOS slice's versioned bundle survive the round trip;
#   - the binding archive holds its four files and nothing else;
#   - the llama archive holds llama.xcframework/ and nothing beside it, and
#     unzips to a framework check-llama-xcframework.sh passes;
#   - SwiftPM takes both framework archives as binary targets: a scratch
#     package naming them by path, with the Swift from the binding archive,
#     builds and runs, and build_info() answers what binding.json says;
#   - `swift package compute-checksum` of each framework archive, the
#     checksum an app pins, is the SHA-256 the release's SHA256SUMS lists for
#     it.
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
LLAMA_ZIP="${DIST}/llama-${VERSION}.xcframework.zip"
SWIFT_FILES=(gglib_ffi.swift "${NAME}.h" module.modulemap)
PIN="$("${ROOT_DIR}/scripts/llama_pin.sh")"
# As build-xcframework.sh finds it; the zip and pin.txt
# build-llama-xcframework.sh writes beside it.
LLAMA_XCFRAMEWORK="${GGLIB_LLAMA_XCFRAMEWORK:-${ROOT_DIR}/target/apple/llama-${PIN}/llama.xcframework}"
LLAMA_DIR="$(dirname "${LLAMA_XCFRAMEWORK}")"
BUILT_LLAMA_ZIP="${LLAMA_DIR}/llama-${PIN}.xcframework.zip"

fail() {
    echo "error: $1" >&2
    exit 1
}

for input in "${XCFRAMEWORK}" "${SWIFT_DIR}"; do
    [[ -d "${input}" ]] || fail "${input} not found. Run \`make ffi-xcframework\` first."
done
[[ -f "${BUILT_LLAMA_ZIP}" ]] || fail "${BUILT_LLAMA_ZIP} not found. Run \`make phone-llama\` first."
[[ "$(cat "${LLAMA_DIR}/pin.txt" 2>/dev/null)" == "${PIN}" ]] \
    || fail "${LLAMA_DIR}/pin.txt does not name the pin, ${PIN}: that llama.xcframework is another llama.cpp."

if ! diff -rq "${SWIFT_DIR}" "${COMMITTED_SWIFT}"; then
    fail "the Swift this build generated is not the committed ${COMMITTED_SWIFT}. Run \`make ffi-binding\` and commit the result."
fi

# What build_info() reports, read from the sources the build was made from,
# as swift-smoke.sh reads them. The SwiftPM check below holds the framework in
# the archive to the same three values.
GGLIB_VERSION="$(cargo pkgid --locked -p gglib-ffi | sed 's/.*[@#]//')"
UNIFFI="$(awk '
    /^\[\[package\]\]/ { name = "" }
    /^name = / { name = $3 }
    /^version = / && name == "\"uniffi\"" { gsub(/"/, "", $3); print $3 }
' Cargo.lock)"
[[ -n "${UNIFFI}" ]] || fail "Cargo.lock names no uniffi version."

rm -rf "${WORK_DIR}"
mkdir -p "${WORK_DIR}"
rm -f "${FRAMEWORK_ZIP}" "${SWIFT_ZIP}" "${LLAMA_ZIP}"

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

echo "==> Packaging ${LLAMA_ZIP##*/} from ${BUILT_LLAMA_ZIP##*/}"
cp "${BUILT_LLAMA_ZIP}" "${LLAMA_ZIP}"

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

roots="$(unzip -Z1 "${LLAMA_ZIP}" | cut -d/ -f1 | LC_ALL=C sort -u | tr '\n' ' ')"
[[ "${roots}" == "llama.xcframework " ]] \
    || fail "${LLAMA_ZIP##*/} holds '${roots% }' at its root, wanted llama.xcframework alone."
echo "ok  ${LLAMA_ZIP##*/} holds llama.xcframework at its root and nothing beside it"

unzip -q "${FRAMEWORK_ZIP}" -d "${WORK_DIR}/unzipped"
"${ROOT_DIR}/scripts/apple/check-slices.sh" "${WORK_DIR}/unzipped/${NAME}.xcframework"
unzip -q "${LLAMA_ZIP}" -d "${WORK_DIR}/unzipped"
"${ROOT_DIR}/scripts/apple/check-llama-xcframework.sh" "${WORK_DIR}/unzipped/llama.xcframework"

# binding.json as the archive carries it, so the check is of the shipped file.
json="$(unzip -p "${SWIFT_ZIP}" binding.json)"
want_gglib="$(jq -er .gglib_version <<<"${json}")"
want_uniffi="$(jq -er .uniffi_version <<<"${json}")"
want_pin="$(jq -er .llama_pin <<<"${json}")"
echo "ok  binding.json: $(jq -c . <<<"${json}")"

# A package shaped the way an app consumes the release: each framework archive
# itself as a binary target, which SwiftPM unpacks as it would a downloaded
# one, and only gglib_ffi.swift from the binding archive in a source target of
# its own. GglibFFI loads llama, so the check runs only with both.
CONSUMER="${WORK_DIR}/consumer"
mkdir -p "${CONSUMER}/Sources/GGLib" "${CONSUMER}/Sources/Check"
cp "${FRAMEWORK_ZIP}" "${LLAMA_ZIP}" "${CONSUMER}/"
unzip -q "${SWIFT_ZIP}" gglib_ffi.swift -d "${CONSUMER}/Sources/GGLib"

cat >"${CONSUMER}/Package.swift" <<SWIFT
// swift-tools-version: 6.2
import PackageDescription

let package = Package(
    name: "GglibFFIReleaseCheck",
    platforms: [.macOS(.v26)],
    targets: [
        .binaryTarget(name: "${NAME}", path: "${FRAMEWORK_ZIP##*/}"),
        .binaryTarget(name: "llama", path: "${LLAMA_ZIP##*/}"),
        .target(name: "GGLib", dependencies: ["${NAME}", "llama"]),
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

echo "==> Building a package that takes the archives as its binary targets"
(cd "${CONSUMER}" && swift run --quiet Check)

# SwiftPM's checksum is the SHA-256 of the file, so the line the release's
# hash step writes for each archive is the one an app's Package.swift pins.
for zip in "${FRAMEWORK_ZIP}" "${LLAMA_ZIP}"; do
    sha256="$(shasum -a 256 "${zip}" | cut -d' ' -f1)"
    checksum="$(cd "${WORK_DIR}" && swift package compute-checksum "${zip}")"
    [[ "${checksum}" == "${sha256}" ]] \
        || fail "swift package compute-checksum ${zip##*/} says ${checksum}, and SHA-256 ${sha256}."
    echo "ok  swift package compute-checksum ${zip##*/} is its SHA-256, ${checksum}"
done

echo
echo "Packaged into ${DIST}:"
for zip in "${FRAMEWORK_ZIP}" "${SWIFT_ZIP}" "${LLAMA_ZIP}"; do
    printf '  %s  %s bytes\n' "${zip##*/}" "$(stat -f %z "${zip}")"
done
