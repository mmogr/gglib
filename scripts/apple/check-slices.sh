#!/usr/bin/env bash
#
# Assert GglibFFI.xcframework holds the three slices it should, each a
# dynamic framework an app can embed, for the platform it claims.
#
# Usage: scripts/apple/check-slices.sh <path to GglibFFI.xcframework>
#
# For each slice (ios-arm64, ios-arm64-simulator, macos-arm64), and nothing
# else in the XCFramework:
#
#   - the binary is an arm64 Mach-O dynamic library and nothing else;
#   - its install name is the path an app's loader finds it at:
#     @rpath/GglibFFI.framework/GglibFFI, and on macOS, whose frameworks are
#     versioned, @rpath/GglibFFI.framework/Versions/A/GglibFFI;
#   - its LC_BUILD_VERSION names the slice's platform and the deployment
#     floor the build asked for;
#   - it exports every function the framework's header declares, so the
#     generated Swift finds each symbol it calls;
#   - the bundle has the layout its platform loads (shallow on iOS, Versions/A
#     on macOS), its header, a modulemap declaring `framework module
#     GglibFFI`, and an Info.plist naming the binary, the platform and the
#     same floor.
#
# The failure this exists for is a build that says it succeeded and is wrong
# in a way only the consuming app's link or launch shows, somewhere else and
# later: a slice compiled for the host, a framework whose install name points
# where no app embeds it, a bundle macOS will not load.
#
# The floors default to 26.0 and are read from IPHONEOS_DEPLOYMENT_TARGET and
# MACOSX_DEPLOYMENT_TARGET, as build-xcframework.sh reads them, so a run on
# its own asserts what the build asked for.
set -euo pipefail

XCFRAMEWORK="${1:?usage: check-slices.sh <path to GglibFFI.xcframework>}"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "error: needs otool, lipo and plutil, so macOS only." >&2
    exit 1
fi

NAME="GglibFFI"
IOS_MIN="${IPHONEOS_DEPLOYMENT_TARGET:-26.0}"
MACOS_MIN="${MACOSX_DEPLOYMENT_TARGET:-26.0}"
fail=0
# Failures in the slice being checked, so its `ok` line is printed only when
# it has none.
slice_wrongs=0

wrong() {
    echo "  WRONG    $1"
    fail=1
    slice_wrongs=$((slice_wrongs + 1))
}

# Apple's platform constants, from <mach-o/loader.h>: `otool -l` prints the
# number, not the name.
platform_name() {
    case "$1" in
        1) echo "MACOS" ;;
        2) echo "IOS" ;;
        7) echo "IOSSIMULATOR" ;;
        *) echo "$1" ;;
    esac
}

# One plist value, or nothing when the key is absent.
plist_value() {
    plutil -extract "$2" raw -o - "$1" 2>/dev/null || true
}

# Check one slice.
#   $1 the slice's directory name inside the XCFramework
#   $2 the binary, relative to the framework
#   $3 the Info.plist, relative to the framework
#   $4 the install name the binary must carry
#   $5 the platform name its LC_BUILD_VERSION must carry
#   $6 the deployment floor
#   $7 CFBundleSupportedPlatforms' entry
#   $8 the Info.plist's minimum-OS key
check_slice() {
    local slice="$1" fw="${XCFRAMEWORK}/$1/${NAME}.framework"
    local bin="${fw}/$2" plist="${fw}/$3"
    local want_install="$4" want_platform="$5" want_min="$6"
    local want_plist_platform="$7" min_key="$8"
    slice_wrongs=0

    if [[ ! -f "${bin}" ]]; then
        wrong "${slice}: no binary at ${bin}"
        return
    fi

    local archs filetype install platforms minos
    archs="$(lipo -archs "${bin}")"
    [[ "${archs}" == "arm64" ]] || wrong "${slice}: carries '${archs}', wanted exactly arm64"

    filetype="$(otool -hv "${bin}" | awk '/^ *MH_MAGIC/ { print $5 }')"
    [[ "${filetype}" == "DYLIB" ]] || wrong "${slice}: is a ${filetype:-?}, wanted a DYLIB"

    install="$(otool -D "${bin}" | tail -n +2)"
    [[ "${install}" == "${want_install}" ]] \
        || wrong "${slice}: install name '${install}', wanted '${want_install}'"

    platforms=""
    local raw
    for raw in $(otool -l "${bin}" | awk '/^ *platform /{ print $2 }' | sort -u); do
        platforms="${platforms}$(platform_name "${raw}") "
    done
    platforms="${platforms% }"
    [[ "${platforms}" == "${want_platform}" ]] \
        || wrong "${slice}: platform '${platforms:-none}', wanted '${want_platform}'"

    minos="$(otool -l "${bin}" | awk '/^ *minos /{ print $2 }' | sort -u | tr '\n' ' ')"
    minos="${minos% }"
    [[ "${minos}" == "${want_min}" ]] \
        || wrong "${slice}: minos '${minos:-none}', wanted '${want_min}'"

    # Every function the header declares, exported by the binary.
    local header="${fw}/Headers/${NAME}.h" missing declared
    if [[ ! -f "${header}" ]]; then
        wrong "${slice}: no Headers/${NAME}.h"
    else
        declared="$(grep -oE '\b(uniffi|ffi)_gglib_ffi_[A-Za-z0-9_]+\b' "${header}" | sort -u)"
        if [[ -z "${declared}" ]]; then
            wrong "${slice}: Headers/${NAME}.h declares no uniffi function"
        fi
        missing="$(comm -23 <(echo "${declared}") \
            <(nm -gU "${bin}" | awk '{ print $3 }' | sed 's/^_//' | sort -u))"
        if [[ -n "${missing}" ]]; then
            wrong "${slice}: does not export $(echo "${missing}" | wc -l | tr -d ' ') function(s) its header declares:"
            echo "${missing}" | sed 's/^/             /'
        fi
    fi

    local modulemap="${fw}/Modules/module.modulemap"
    if [[ ! -f "${modulemap}" ]]; then
        wrong "${slice}: no Modules/module.modulemap"
    elif ! grep -q "^framework module ${NAME} {" "${modulemap}"; then
        wrong "${slice}: Modules/module.modulemap does not declare framework module ${NAME}"
    fi

    if [[ ! -f "${plist}" ]] || ! plutil -lint -s "${plist}"; then
        wrong "${slice}: no valid Info.plist at $3"
    else
        [[ "$(plist_value "${plist}" CFBundleExecutable)" == "${NAME}" ]] \
            || wrong "${slice}: Info.plist's CFBundleExecutable is not ${NAME}"
        [[ "$(plist_value "${plist}" CFBundlePackageType)" == "FMWK" ]] \
            || wrong "${slice}: Info.plist's CFBundlePackageType is not FMWK"
        [[ "$(plist_value "${plist}" CFBundleSupportedPlatforms.0)" == "${want_plist_platform}" ]] \
            || wrong "${slice}: Info.plist's CFBundleSupportedPlatforms is not [${want_plist_platform}]"
        [[ "$(plist_value "${plist}" "${min_key}")" == "${want_min}" ]] \
            || wrong "${slice}: Info.plist's ${min_key} is not ${want_min}"
    fi

    if [[ "${slice_wrongs}" -eq 0 ]]; then
        echo "  ok       ${slice}  [${archs}]  ${filetype}  platform ${platforms}  minos ${minos}"
        echo "           ${install}"
    fi
}

echo "Checking slices in ${XCFRAMEWORK}"

if [[ ! -d "${XCFRAMEWORK}" ]]; then
    echo "error: ${XCFRAMEWORK} not found. Run \`make ffi-xcframework-fast\` first." >&2
    exit 1
fi

# The three slices and nothing else: a fourth means a target crept into the
# build without this check, or the layout, learning of it.
slices="$(cd "${XCFRAMEWORK}" && find . -mindepth 1 -maxdepth 1 -type d | sed 's|^\./||' | LC_ALL=C sort | tr '\n' ' ')"
[[ "${slices}" == "ios-arm64 ios-arm64-simulator macos-arm64 " ]] \
    || wrong "the XCFramework holds '${slices% }', wanted 'ios-arm64 ios-arm64-simulator macos-arm64'"

check_slice ios-arm64 "${NAME}" Info.plist \
    "@rpath/${NAME}.framework/${NAME}" IOS "${IOS_MIN}" iPhoneOS MinimumOSVersion
check_slice ios-arm64-simulator "${NAME}" Info.plist \
    "@rpath/${NAME}.framework/${NAME}" IOSSIMULATOR "${IOS_MIN}" iPhoneSimulator MinimumOSVersion
check_slice macos-arm64 "Versions/A/${NAME}" Versions/A/Resources/Info.plist \
    "@rpath/${NAME}.framework/Versions/A/${NAME}" MACOS "${MACOS_MIN}" MacOSX LSMinimumSystemVersion

# The versioned bundle's symlinks: a macOS framework is loaded through them.
mac_fw="${XCFRAMEWORK}/macos-arm64/${NAME}.framework"
if [[ -d "${mac_fw}" ]]; then
    [[ "$(readlink "${mac_fw}/Versions/Current")" == "A" ]] \
        || wrong "macos-arm64: Versions/Current does not point at A"
    for entry in "${NAME}" Headers Modules Resources; do
        [[ "$(readlink "${mac_fw}/${entry}")" == "Versions/Current/${entry}" ]] \
            || wrong "macos-arm64: ${entry} is not a symlink to Versions/Current/${entry}"
    done
fi

if [[ "${fail}" -ne 0 ]]; then
    echo
    echo "error: at least one slice is not what it claims to be." >&2
    echo "       A framework that ships like this fails at link or launch in the consuming app." >&2
    exit 1
fi

echo "All three slices are dynamic frameworks for the platform they claim."
