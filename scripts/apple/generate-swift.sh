#!/usr/bin/env bash
#
# Generate gglib-ffi's Swift binding, its C header and its framework
# modulemap, out of a built library.
#
# Usage: scripts/apple/generate-swift.sh <out-dir> [<library>]
#
# Writes exactly three files into <out-dir>, replacing whatever was there:
#
#   gglib_ffi.swift    the Swift an app compiles beside the framework
#   GglibFFI.h         the framework's header
#   module.modulemap   `framework module GglibFFI`, the framework's module
#
# <library> is the cdylib to read the scaffolding out of, by default the host
# build this script makes. The scaffolding describes the API, not the
# machine, so any slice gives the same three files, and so does Linux.
#
# The generator is the crate's own `uniffi-bindgen` binary (feature
# `bindgen`), uniffi's Swift-specific command: only it writes a `framework
# module`. It finds crates/gglib-ffi/uniffi.toml through `cargo metadata`,
# which is how the Swift comes to `import GglibFFI`; the modulemap's name is
# passed here, and the two are checked to agree before and after.
#
# Generated into a scratch directory and moved into place only once the three
# files are there and say what they should, so a failed run never leaves
# <out-dir> half-written. No formatter runs: the bytes must not depend on what
# is installed, or a committed copy would differ by machine.
set -euo pipefail

OUT_DIR="${1:?usage: generate-swift.sh <out-dir> [<library>]}"
LIBRARY="${2:-}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

# The framework, the module it declares and the module the Swift imports.
MODULE="GglibFFI"
CONFIG="crates/gglib-ffi/uniffi.toml"
configured="$(sed -n 's/^ffi_module_name = "\(.*\)"$/\1/p' "${CONFIG}")"
if [[ "${configured}" != "${MODULE}" ]]; then
    echo "error: ${CONFIG} names the module '${configured}', and this script ${MODULE}." >&2
    echo "       The Swift imports the first and the framework declares the second;" >&2
    echo "       they must be one name." >&2
    exit 1
fi

TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT_DIR}/target}"
case "$(uname -s)" in
    Darwin) HOST_LIB="libgglib_ffi.dylib" ;;
    *) HOST_LIB="libgglib_ffi.so" ;;
esac

# The generator, and the host library beside it: a package's binary always
# builds its library too.
cargo build --locked -p gglib-ffi --features bindgen --lib --bin uniffi-bindgen
LIBRARY="${LIBRARY:-${TARGET_DIR}/debug/${HOST_LIB}}"

mkdir -p "${TARGET_DIR}"
STAGING="$(mktemp -d "${TARGET_DIR}/gglib-ffi-swift.XXXXXX")"
trap 'rm -rf "${STAGING}"' EXIT
# mktemp makes it private; it becomes <out-dir>, an ordinary directory.
chmod 755 "${STAGING}"

"${TARGET_DIR}/debug/uniffi-bindgen" \
    --swift-sources --headers --modulemap \
    --xcframework \
    --module-name "${MODULE}" \
    --modulemap-filename module.modulemap \
    "${LIBRARY}" "${STAGING}"

expected="$(printf '%s\n' "${MODULE}.h" gglib_ffi.swift module.modulemap | LC_ALL=C sort)"
written="$(cd "${STAGING}" && find . -mindepth 1 | sed 's|^\./||' | LC_ALL=C sort)"
if [[ "${written}" != "${expected}" ]]; then
    echo "error: the generator wrote" >&2
    sed 's/^/         /' <<<"${written}" >&2
    echo "       and this script expects exactly" >&2
    sed 's/^/         /' <<<"${expected}" >&2
    exit 1
fi
if ! grep -qx "import ${MODULE}" "${STAGING}/gglib_ffi.swift"; then
    echo "error: the generated Swift does not import ${MODULE}, so ${CONFIG} was not read." >&2
    exit 1
fi
if ! grep -q "^framework module ${MODULE} {" "${STAGING}/module.modulemap"; then
    echo "error: the generated modulemap does not declare framework module ${MODULE}." >&2
    exit 1
fi

rm -rf "${OUT_DIR}"
mkdir -p "$(dirname "${OUT_DIR}")"
mv "${STAGING}" "${OUT_DIR}"
trap - EXIT
echo "Generated the Swift binding into ${OUT_DIR}"
