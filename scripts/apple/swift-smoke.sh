#!/usr/bin/env bash
#
# Prove the generated Swift links against GglibFFI.xcframework and that
# every call that needs no network crosses the boundary, on the macOS slice.
#
# Builds a scratch SwiftPM package under target/apple/smoke shaped the way an
# app consumes gglib: a binary target for the XCFramework, a source target
# `GGLib` holding the generated Swift, and an executable that imports only
# `GGLib`. Then `swift run`s it. Nothing boots a simulator.
#
# WHAT THIS PROVES
#   - the generated Swift compiles in Swift 6 language mode, with complete
#     strict concurrency, as the consuming app compiles it
#   - `import GglibFFI` in that Swift finds the framework's module
#   - the dynamic framework links, and loads at run time from the install
#     name it carries (@rpath/GglibFFI.framework/Versions/A/GglibFFI)
#   - uniffi's contract version and API checksums agree between the Swift
#     and the binary (a mismatch is a fatalError at the first call)
#   - build_info() returns the record with every field intact
#   - each Library call crosses and answers, an async one awaited from
#     Swift and an error caught as a GglibError: open_library in a scratch
#     folder, suggested, installed, remove of an id the library does not
#     hold, quantizations and download of a repository id with no owner
#     (refused before any request is made), pause_all and resume_all
#
# WHAT IT DOES NOT PROVE
#   Anything about iOS: the device and simulator slices are checked by
#   check-slices.sh, and run only in an app. Nothing here reaches the
#   network, so search, a download that runs, and the DownloadWatch it
#   answers are left to the crate's own tests, which run them against a
#   stand-in Hub and file server.
#
# Uses the binding generated beside the framework (target/apple/swift), so
# it tests the pair one build made; `make ffi-binding-check` holds the
# committed copy to the same bytes.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT_DIR}"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "error: needs Swift and the macOS slice, so macOS only." >&2
    exit 1
fi

TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT_DIR}/target}"
APPLE_DIR="${TARGET_DIR}/apple"
XCFRAMEWORK="${APPLE_DIR}/GglibFFI.xcframework"
BINDING="${APPLE_DIR}/swift/gglib_ffi.swift"
WORK_DIR="${APPLE_DIR}/smoke"

for input in "${XCFRAMEWORK}" "${BINDING}"; do
    if [[ ! -e "${input}" ]]; then
        echo "error: ${input} not found. Run \`make ffi-xcframework-fast\` first." >&2
        exit 1
    fi
done

# What build_info() must answer, read here from the sources the library was
# built from rather than from the library.
VERSION="$(cargo pkgid --locked -p gglib-ffi | sed 's/.*[@#]//')"
PIN="$("${ROOT_DIR}/scripts/llama_pin.sh")"
UNIFFI="$(awk '
    /^\[\[package\]\]/ { name = "" }
    /^name = / { name = $3 }
    /^version = / && name == "\"uniffi\"" { gsub(/"/, "", $3); print $3 }
' Cargo.lock)"

rm -rf "${WORK_DIR}"
mkdir -p "${WORK_DIR}/Sources/GGLib" "${WORK_DIR}/Sources/Smoke"
# SwiftPM takes a local binary target only from inside the package.
ditto "${XCFRAMEWORK}" "${WORK_DIR}/GglibFFI.xcframework"
cp "${BINDING}" "${WORK_DIR}/Sources/GGLib/"

cat >"${WORK_DIR}/Package.swift" <<'SWIFT'
// swift-tools-version: 6.2
import PackageDescription

// The shape an app consumes gglib in: the framework as a binary target, the
// generated Swift as a source target of its own, and code that imports only
// the latter.
let package = Package(
    name: "GglibFFISmoke",
    platforms: [.macOS(.v26)],
    targets: [
        .binaryTarget(name: "GglibFFI", path: "GglibFFI.xcframework"),
        .target(name: "GGLib", dependencies: ["GglibFFI"]),
        .executableTarget(name: "Smoke", dependencies: ["GGLib"]),
    ],
    swiftLanguageModes: [.v6]
)
SWIFT

cat >"${WORK_DIR}/Sources/Smoke/main.swift" <<SMOKE
import Foundation
import GGLib

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("smoke: \(message)\n".utf8))
    exit(1)
}

// The first call also runs uniffi's own checks of the binding against the
// binary: its contract version and every API checksum.
let info = buildInfo()
print("ok  buildInfo() returned: gglib \(info.gglibVersion), uniffi \(info.uniffiVersion), llama.cpp \(info.llamaPin)")

guard info.gglibVersion == "${VERSION}" else {
    fail("gglibVersion is \(info.gglibVersion), and the workspace is ${VERSION}")
}
guard info.uniffiVersion == "${UNIFFI}" else {
    fail("uniffiVersion is \(info.uniffiVersion), and Cargo.lock names ${UNIFFI}")
}
guard info.llamaPin == "${PIN}" else {
    fail("llamaPin is \(info.llamaPin), and the pin is ${PIN}")
}
print("ok  every field is the one the sources name")

// A record crosses as a value: equal fields, equal records.
let same = BuildInfo(gglibVersion: "${VERSION}", uniffiVersion: "${UNIFFI}", llamaPin: "${PIN}")
guard info == same else {
    fail("a BuildInfo built in Swift from the same fields is not equal to the one returned")
}

// A library in folders of the smoke's own, asking Hugging Face as nobody.
// Nothing below reaches the network.
let root = FileManager.default.temporaryDirectory
    .appendingPathComponent("gglib-ffi-smoke-\(ProcessInfo.processInfo.processIdentifier)")
defer { try? FileManager.default.removeItem(at: root) }
let dirs = LibraryDirs(
    dataDir: root.appendingPathComponent("data").path,
    modelsDir: root.appendingPathComponent("models").path
)
let library: Library
do {
    library = try await openLibrary(dirs: dirs, hfToken: nil)
} catch {
    fail("openLibrary threw \(error)")
}
print("ok  openLibrary() opened a library in \(root.path)")

let reading = DeviceReading(physicalBytes: 8 << 30, availableBytes: 6 << 30, isSimulator: false)
let suggested = library.suggested(reading: reading)
guard let smallest = suggested.last, case .fits = smallest.verdict, !smallest.rationale.isEmpty else {
    fail("suggested() on an 8 GiB phone did not fit its smallest row: \(suggested)")
}
print("ok  suggested() named \(suggested.count) models, the smallest \(smallest.repo) \(smallest.verdict)")

do {
    let installed = try await library.installed()
    guard installed.isEmpty else { fail("a new library holds \(installed)") }
} catch {
    fail("installed() threw \(error)")
}
print("ok  installed() is empty in a new library")

do {
    try await library.remove(id: 404)
    fail("remove() removed a model the library does not hold")
} catch GglibError.NotFound {
    print("ok  remove() of an unknown id threw GglibError.NotFound")
} catch {
    fail("remove() threw \(error), not GglibError.NotFound")
}

// A repository id names an owner and a name; one without is refused before
// a request is made.
do {
    _ = try await library.quantizations(repo: "no-owner", reading: reading)
    fail("quantizations() answered for a repository id with no owner")
} catch let error as GglibError {
    print("ok  quantizations() of an id with no owner threw \(error)")
}
do {
    _ = try await library.download(repo: "no-owner", quantization: "Q4_K_M")
    fail("download() queued a repository id with no owner")
} catch let error as GglibError {
    print("ok  download() of an id with no owner threw \(error)")
}

await library.pauseAll()
await library.resumeAll()
print("ok  pauseAll() and resumeAll() returned with nothing queued")

print("smoke: the binding links and answers across the boundary")
SMOKE

echo "==> Building and running the smoke executable (Swift $(swift --version 2>&1 | sed -n 's/.*Swift version \([^ ]*\).*/\1/p' | head -1))"
cd "${WORK_DIR}"
swift run --quiet Smoke
