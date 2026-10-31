#!/usr/bin/env bash
#
# Prove the generated Swift links against GglibFFI.xcframework, that every
# call that needs no network crosses the boundary, and that the engine it
# serves answers a chat, on the macOS slice.
#
# Builds a scratch SwiftPM package under target/apple/smoke shaped the way an
# app consumes gglib: a binary target for each XCFramework (GglibFFI, and
# llama, which it links), a source target `GGLib` holding the generated
# Swift, and an executable that imports only `GGLib`. Then `swift run`s it.
# Nothing boots a simulator.
#
# Needs GGLIB_ENGINE_MODELS_DIR holding tinygemma3-Q8_0.gguf (the recorder's
# tiny model, `.github/actions/pinned-engines/models.txt`), which
# gglib-ffi-add-model (the crate's `smoke` feature) adds to the smoke's
# library before the Swift runs: the binding has no call that adds a file.
#
# WHAT THIS PROVES
#   - the generated Swift compiles in Swift 6 language mode, with complete
#     strict concurrency, as the consuming app compiles it
#   - `import GglibFFI` in that Swift finds the framework's module
#   - the dynamic framework links, and loads at run time from the install
#     name it carries (@rpath/GglibFFI.framework/Versions/A/GglibFFI), with
#     llama.framework found at its own from the same run path
#   - uniffi's contract version and API checksums agree between the Swift
#     and the binary (a mismatch is a fatalError at the first call)
#   - build_info() returns the record with every field intact
#   - each Library call crosses and answers, an async one awaited from
#     Swift and an error caught as a GglibError: open_library in a scratch
#     folder, suggested, installed, remove of an id the library does not
#     hold, quantizations and download of a repository id with no owner
#     (refused before any request is made), pause_all and resume_all
#   - serve() answers a base URL and a key; a chat without the key is 401;
#     one reply streams from tinygemma3 through URLSession, decoded by
#     llama.cpp on the CPU, and its trailing usage carries the context_size
#     the engine's watch reports as Ready; shutdown(), twice, ends the watch
#     on Stopped and closes the port
#
# WHAT IT DOES NOT PROVE
#   Anything about iOS: the device and simulator slices are checked by
#   check-slices.sh, and run only in an app. Nothing here reaches the
#   network, so search, a download that runs, and the DownloadWatch it
#   answers are left to the crate's own tests, which run them against a
#   stand-in Hub and file server. Metal: the Mac's decoder offloads nothing.
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
# The llama.cpp the framework was built against, as build-xcframework.sh
# finds it.
LLAMA_XCFRAMEWORK="${GGLIB_LLAMA_XCFRAMEWORK:-${ROOT_DIR}/target/apple/llama-$("${ROOT_DIR}/scripts/llama_pin.sh")/llama.xcframework}"
MODEL="${GGLIB_ENGINE_MODELS_DIR:-}/tinygemma3-Q8_0.gguf"

for input in "${XCFRAMEWORK}" "${BINDING}"; do
    if [[ ! -e "${input}" ]]; then
        echo "error: ${input} not found. Run \`make ffi-xcframework-fast\` first." >&2
        exit 1
    fi
done
if [[ ! -d "${LLAMA_XCFRAMEWORK}" ]]; then
    echo "error: no llama.xcframework at ${LLAMA_XCFRAMEWORK}. Run \`make phone-llama\` first." >&2
    exit 1
fi
if [[ -z "${GGLIB_ENGINE_MODELS_DIR:-}" || ! -s "${MODEL}" ]]; then
    echo "error: the smoke serves tinygemma3-Q8_0.gguf from GGLIB_ENGINE_MODELS_DIR, which" >&2
    echo "       is '${GGLIB_ENGINE_MODELS_DIR:-}'. .github/actions/pinned-engines/models.txt says" >&2
    echo "       where the file comes from." >&2
    exit 1
fi

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
ditto "${LLAMA_XCFRAMEWORK}" "${WORK_DIR}/llama.xcframework"
cp "${BINDING}" "${WORK_DIR}/Sources/GGLib/"

# The library the smoke opens, holding tinygemma3, added where it lies.
LIBRARY_DIR="${WORK_DIR}/library"
mkdir -p "${LIBRARY_DIR}/data" "${LIBRARY_DIR}/models"
echo "==> Adding $(basename "${MODEL}") to the smoke's library"
cargo build --locked --quiet -p gglib-ffi --features smoke --bin gglib-ffi-add-model
MODEL_NAME="$("${TARGET_DIR}/debug/gglib-ffi-add-model" \
    "${LIBRARY_DIR}/data" "${LIBRARY_DIR}/models" "${MODEL}")"
echo "    served as ${MODEL_NAME}"

cat >"${WORK_DIR}/Package.swift" <<'SWIFT'
// swift-tools-version: 6.2
import PackageDescription

// The shape an app consumes gglib in: each framework as a binary target, the
// generated Swift as a source target of its own, and code that imports only
// the latter. Nothing imports llama; GglibFFI loads it.
let package = Package(
    name: "GglibFFISmoke",
    platforms: [.macOS(.v26)],
    targets: [
        .binaryTarget(name: "GglibFFI", path: "GglibFFI.xcframework"),
        .binaryTarget(name: "llama", path: "llama.xcframework"),
        .target(name: "GGLib", dependencies: ["GglibFFI", "llama"]),
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

// The smoke's library, holding tinygemma3, asking Hugging Face as nobody.
// Nothing below reaches the network.
let dirs = LibraryDirs(dataDir: "${LIBRARY_DIR}/data", modelsDir: "${LIBRARY_DIR}/models")
let library: Library
do {
    library = try await openLibrary(dirs: dirs, hfToken: nil)
} catch {
    fail("openLibrary threw \(error)")
}
print("ok  openLibrary() opened a library in ${LIBRARY_DIR}")

let reading = DeviceReading(physicalBytes: 8 << 30, availableBytes: 6 << 30, isSimulator: false)
let suggested = library.suggested(reading: reading)
guard let smallest = suggested.last, case .fits = smallest.verdict, !smallest.rationale.isEmpty else {
    fail("suggested() on an 8 GiB phone did not fit its smallest row: \(suggested)")
}
print("ok  suggested() named \(suggested.count) models, the smallest \(smallest.repo) \(smallest.verdict)")

let installedId: Int64
do {
    let installed = try await library.installed()
    guard installed.map(\.name) == ["${MODEL_NAME}"] else {
        fail("the library holds \(installed), not ${MODEL_NAME} alone")
    }
    installedId = installed[0].id
} catch {
    fail("installed() threw \(error)")
}
print("ok  installed() lists ${MODEL_NAME}")

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

// The engine: gglib's proxy on loopback, in front of llama.cpp in this
// process, asked as an app asks it.
let server: Server
do {
    server = try await library.serve(reading: reading)
} catch {
    fail("serve() threw \(error)")
}
let status = server.status()
guard await status.next() == .idle else { fail("a new engine is not idle") }
print("ok  serve() serves at \(server.baseUrl()) with a key of \(server.apiKey().count) characters, idle")

func chat(key: String?) -> URLRequest {
    var request = URLRequest(url: URL(string: server.baseUrl() + "/chat/completions")!)
    request.httpMethod = "POST"
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    if let key { request.setValue("Bearer \(key)", forHTTPHeaderField: "Authorization") }
    let body: [String: Any] = [
        "model": "${MODEL_NAME}",
        "stream": true,
        "return_progress": true,
        "max_tokens": 16,
        "messages": [["role": "user", "content": "Say hello."]],
    ]
    request.httpBody = try! JSONSerialization.data(withJSONObject: body)
    return request
}

do {
    let (_, refused) = try await URLSession.shared.data(for: chat(key: nil))
    guard (refused as? HTTPURLResponse)?.statusCode == 401 else {
        fail("a chat without the key was answered \(refused)")
    }
} catch {
    fail("a chat without the key failed: \(error)")
}
print("ok  a chat without the key is 401")

var reply = ""
var contextSize: Int?
var done = false
do {
    let (bytes, response) = try await URLSession.shared.bytes(for: chat(key: server.apiKey()))
    guard (response as? HTTPURLResponse)?.statusCode == 200 else { fail("the chat was answered \(response)") }
    for try await line in bytes.lines {
        guard line.hasPrefix("data: ") else { continue }
        let data = line.dropFirst("data: ".count)
        if data == "[DONE]" {
            done = true
            break
        }
        guard let frame = try JSONSerialization.jsonObject(with: Data(data.utf8)) as? [String: Any] else {
            fail("a frame that is not an object: \(data)")
        }
        if let choices = frame["choices"] as? [[String: Any]],
           let delta = choices.first?["delta"] as? [String: Any],
           let piece = delta["content"] as? String {
            reply += piece
        }
        if let usage = frame["usage"] as? [String: Any], let size = usage["context_size"] as? Int {
            contextSize = size
        }
    }
} catch {
    fail("the chat failed: \(error)")
}
guard done, !reply.isEmpty else { fail("the stream ended without [DONE] or with no reply: \(reply)") }
guard let contextSize, contextSize > 0 else { fail("the stream's usage carried no context_size") }
var shown = await status.next()
while case .loading = shown { shown = await status.next() }
guard case .ready(let id, let model, let nCtx) = shown, id == installedId, model == "${MODEL_NAME}",
      nCtx == UInt64(contextSize) else {
    fail("the engine says \(String(describing: shown)), and the reply's usage a context of \(contextSize)")
}
print("ok  a reply of \(reply.count) characters streamed; usage.context_size \(contextSize) is the context Ready reports")

await server.shutdown()
await server.shutdown()
var last = await status.next()
while let state = last, state != .stopped { last = await status.next() }
guard last == .stopped, await status.next() == nil else { fail("the watch did not end on stopped") }
do {
    _ = try await URLSession.shared.data(for: chat(key: server.apiKey()))
    fail("the engine still answers once shut down")
} catch {
    print("ok  shutdown(), twice, ended the watch on stopped and closed the port")
}

print("smoke: the binding links and answers across the boundary, and its engine streams a reply")
SMOKE

echo "==> Building and running the smoke executable (Swift $(swift --version 2>&1 | sed -n 's/.*Swift version \([^ ]*\).*/\1/p' | head -1))"
cd "${WORK_DIR}"
swift run --quiet Smoke
