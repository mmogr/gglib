#!/usr/bin/env bash
#
# Build GglibFFI.xcframework: gglib-ffi's cdylib wrapped as a dynamic
# framework, one per slice.
#
#   ios-arm64              iPhone
#   ios-arm64-simulator    Simulator on Apple silicon
#   macos-arm64            The Mac, where an app's `swift test` runs the binding
#
# Writes, under target/apple/ (or $CARGO_TARGET_DIR/apple/):
#
#   GglibFFI.xcframework   what an app links
#   swift/                 the binding generated from the macOS slice, the
#                          header and modulemap the frameworks carry
#
# PROFILE picks the cargo profile: `release` (the default, what ships) or
# `dev`. Every slice is then checked by scripts/apple/check-slices.sh.
#
# Needs Xcode, so it runs on macOS only. The crate itself builds anywhere:
# `cargo test -p gglib-ffi` is the check to run elsewhere.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "error: this needs Xcode, so it only runs on macOS." >&2
    echo "       The crate itself builds anywhere: \`cargo test -p gglib-ffi\` is the check to run here." >&2
    exit 1
fi

# `PROFILE` is the cargo profile name, and the output directory is not always
# the same word: the dev profile lands in `target/<triple>/debug`, and
# `--profile debug` is not accepted. So the two are separate variables.
PROFILE="${PROFILE:-release}"
if [[ "${PROFILE}" == "dev" || "${PROFILE}" == "debug" ]]; then
    PROFILE="dev"
    PROFILE_DIR="debug"
else
    PROFILE_DIR="${PROFILE}"
fi

# Deployment targets, and why they are set at all.
#
# rustc's `aarch64-apple-ios` target defaults to a deployment target far
# below the SDK that `cc` compiles any C in the tree against. Unset, a C
# object can reference `___chkstk_darwin`, which the default floor's runtime
# lacks, and an unoptimised cdylib fails to link. Set, every object cargo
# compiles carries the floor asked for, and the linked library carries it
# too.
#
# 26.0, ggchat's own floor (`platforms: [.iOS(.v26), .macOS(.v26)]`): a lower
# one buys reach no consumer can use, and a higher one is a link error in the
# consumer's project rather than here. Overridable, for an app that is not
# ggchat.
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-26.0}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-26.0}"

FRAMEWORK_NAME="GglibFFI"
BUNDLE_ID="com.gglib.ffi"
LIB_NAME="libgglib_ffi.dylib"
TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT_DIR}/target}"
OUT_DIR="${TARGET_DIR}/apple"
SWIFT_DIR="${OUT_DIR}/swift"
STAGE_DIR="${OUT_DIR}/frameworks"
XCFRAMEWORK="${OUT_DIR}/${FRAMEWORK_NAME}.xcframework"

# The workspace version, whole for the record and as dotted numbers for the
# bundle: App Store validation takes only up to three period-separated
# integers in CFBundleShortVersionString and CFBundleVersion, so a release
# candidate's `-rc.N` is left off there.
VERSION="$(cargo pkgid --locked -p gglib-ffi | sed 's/.*[@#]//')"
BUNDLE_VERSION="${VERSION%%-*}"

TARGETS=(
    aarch64-apple-ios
    aarch64-apple-ios-sim
    aarch64-apple-darwin
)

echo "==> Building gglib-ffi ${VERSION} for three slices (profile ${PROFILE})"
echo "    iOS ${IPHONEOS_DEPLOYMENT_TARGET}, macOS ${MACOSX_DEPLOYMENT_TARGET}"
printf '    %s\n' "${TARGETS[@]}"
# One cargo invocation with three `--target` flags, not three invocations:
# each target's build ends in a narrow tail (the last crates, then the link)
# that leaves most cores idle, and one build plan lets the next target's wide
# base fill them. Concurrent invocations would only queue on cargo's lock on
# the target directory.
cargo build --locked -p gglib-ffi --lib --profile "${PROFILE}" "${TARGETS[@]/#/--target=}"

echo "==> Generating the Swift binding, header and modulemap"
# From the macOS slice itself, so the header the frameworks carry describes
# the scaffolding in the binary beside it.
"${ROOT_DIR}/scripts/apple/generate-swift.sh" "${SWIFT_DIR}" \
    "${TARGET_DIR}/aarch64-apple-darwin/${PROFILE_DIR}/${LIB_NAME}"

# The keys an embedded framework's Info.plist needs: what its binary is
# called, that it is a framework, its identity and version, the platform it
# was built for, and its minimum OS, which App Store validation reads from
# every embedded bundle.
#   $1 the plist to write
#   $2 CFBundleSupportedPlatforms' one entry
#   $3 the minimum-OS key (MinimumOSVersion on iOS, LSMinimumSystemVersion on macOS)
#   $4 its value
write_info_plist() {
    cat >"$1" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleExecutable</key>
	<string>${FRAMEWORK_NAME}</string>
	<key>CFBundleIdentifier</key>
	<string>${BUNDLE_ID}</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>${FRAMEWORK_NAME}</string>
	<key>CFBundlePackageType</key>
	<string>FMWK</string>
	<key>CFBundleShortVersionString</key>
	<string>${BUNDLE_VERSION}</string>
	<key>CFBundleVersion</key>
	<string>${BUNDLE_VERSION}</string>
	<key>CFBundleSupportedPlatforms</key>
	<array>
		<string>$2</string>
	</array>
	<key>$3</key>
	<string>$4</string>
</dict>
</plist>
PLIST
    plutil -lint -s "$1"
}

# An iOS framework is shallow: binary, Info.plist, Headers and Modules at the
# bundle's root.
#   $1 the framework to write
#   $2 the built library
#   $3 CFBundleSupportedPlatforms' entry
ios_framework() {
    mkdir -p "$1/Headers" "$1/Modules"
    cp "$2" "$1/${FRAMEWORK_NAME}"
    cp "${SWIFT_DIR}/${FRAMEWORK_NAME}.h" "$1/Headers/"
    cp "${SWIFT_DIR}/module.modulemap" "$1/Modules/"
    write_info_plist "$1/Info.plist" "$3" MinimumOSVersion "${IPHONEOS_DEPLOYMENT_TARGET}"
}

# A macOS framework is versioned: everything under Versions/A, the
# Info.plist under its Resources, and symlinks at the root through
# Versions/Current. A shallow one does not load on macOS.
#   $1 the framework to write
#   $2 the built library
macos_framework() {
    local version="$1/Versions/A"
    mkdir -p "${version}/Headers" "${version}/Modules" "${version}/Resources"
    cp "$2" "${version}/${FRAMEWORK_NAME}"
    cp "${SWIFT_DIR}/${FRAMEWORK_NAME}.h" "${version}/Headers/"
    cp "${SWIFT_DIR}/module.modulemap" "${version}/Modules/"
    write_info_plist "${version}/Resources/Info.plist" MacOSX LSMinimumSystemVersion "${MACOSX_DEPLOYMENT_TARGET}"
    ln -s A "$1/Versions/Current"
    local entry
    for entry in "${FRAMEWORK_NAME}" Headers Modules Resources; do
        ln -s "Versions/Current/${entry}" "$1/${entry}"
    done
}

echo "==> Wrapping each slice as ${FRAMEWORK_NAME}.framework"
rm -rf "${STAGE_DIR}" "${XCFRAMEWORK}"
DEVICE_FW="${STAGE_DIR}/ios/${FRAMEWORK_NAME}.framework"
SIMULATOR_FW="${STAGE_DIR}/ios-simulator/${FRAMEWORK_NAME}.framework"
MAC_FW="${STAGE_DIR}/macos/${FRAMEWORK_NAME}.framework"
ios_framework "${DEVICE_FW}" "${TARGET_DIR}/aarch64-apple-ios/${PROFILE_DIR}/${LIB_NAME}" iPhoneOS
ios_framework "${SIMULATOR_FW}" "${TARGET_DIR}/aarch64-apple-ios-sim/${PROFILE_DIR}/${LIB_NAME}" iPhoneSimulator
macos_framework "${MAC_FW}" "${TARGET_DIR}/aarch64-apple-darwin/${PROFILE_DIR}/${LIB_NAME}"

echo "==> Assembling ${XCFRAMEWORK}"
xcodebuild -create-xcframework \
    -framework "${DEVICE_FW}" \
    -framework "${SIMULATOR_FW}" \
    -framework "${MAC_FW}" \
    -output "${XCFRAMEWORK}"
rm -rf "${STAGE_DIR}"

echo "==> Checking each slice is what it claims to be"
"${ROOT_DIR}/scripts/apple/check-slices.sh" "${XCFRAMEWORK}"

echo
echo "Built ${XCFRAMEWORK}"
echo "Swift binding beside it in ${SWIFT_DIR}"
