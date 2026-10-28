#!/usr/bin/env bash
#
# Check that a llama.xcframework holds what the phone engine links against.
#
#   check-llama-xcframework.sh [path/to/llama.xcframework]
#
# With no argument it checks $GGLIB_LLAMA_XCFRAMEWORK, or else the framework
# scripts/apple/build-llama-xcframework.sh builds for the pin. For each slice
# in SLICES it asserts that the framework's Info.plist lists it, that its
# binary is a dynamic library carrying exactly the slice's architectures, each
# built for the slice's platform, that llama.h is among its headers, and that
# every architecture exports each function in SYMBOLS. A slice that passes
# gets one line, and one that fails a line for each thing it lacks. A slice
# the plist lists beyond these is named and passes.
#
# Needs macOS: it reads the binaries with plutil, lipo, otool and nm.

set -euo pipefail

[ $# -le 1 ] || { sed -n '5p' "$0" | sed 's/^# *//' >&2; exit 2; }

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "error: reading a framework's binaries needs Xcode's tools, so this runs on macOS only." >&2
    exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if [[ $# -eq 1 ]]; then
    FRAMEWORK="$1"
elif [[ -n "${GGLIB_LLAMA_XCFRAMEWORK:-}" ]]; then
    FRAMEWORK="$GGLIB_LLAMA_XCFRAMEWORK"
else
    FRAMEWORK="$ROOT_DIR/target/apple/llama-$("$ROOT_DIR/scripts/llama_pin.sh")/llama.xcframework"
fi

# The slices the phone engine links: the iPhone, the simulator on either Mac
# and the Mac, each named as xcodebuild -create-xcframework names it, with the
# Mach-O platform number otool prints for it and that platform's name in
# <mach-o/loader.h>, and its architectures in sorted order.
SLICES="ios-arm64:2:IOS:arm64
ios-arm64_x86_64-simulator:7:IOSSIMULATOR:arm64 x86_64
macos-arm64_x86_64:1:MACOS:arm64 x86_64"

# The llama.h functions the phone engine calls. A pin whose framework renames
# or drops one fails here, before anything links against it.
SYMBOLS="llama_backend_init llama_model_load_from_file llama_model_free
llama_init_from_model llama_free llama_tokenize llama_token_to_piece
llama_decode llama_batch_get_one llama_sampler_chain_init llama_sampler_sample
llama_set_abort_callback llama_model_n_ctx_train llama_supports_gpu_offload
llama_model_chat_template"

PLIST="$FRAMEWORK/Info.plist"
if [[ ! -f "$PLIST" ]]; then
    echo "error: $FRAMEWORK is not an xcframework: it has no Info.plist." >&2
    exit 1
fi

listed=""
i=0
while id=$(plutil -extract "AvailableLibraries.$i.LibraryIdentifier" raw -o - "$PLIST" 2>/dev/null); do
    listed="$listed $id"
    i=$((i + 1))
done

# shellcheck disable=SC2086 # SYMBOLS is a list of plain names
symbol_count=$(echo $SYMBOLS | wc -w | tr -d ' ')
echo "Checking $FRAMEWORK"
fail=0

# Print the problems with one slice, one per line, and nothing when it has
# none: $1 slice, $2 platform number, $3 platform name, $4 architectures.
slice_problems() {
    local slice="$1" platform="$2" platform_name="$3" want_archs="$4"
    local bin="$FRAMEWORK/$slice/llama.framework/llama"

    case " $listed " in
        *" $slice "*) ;;
        *) echo "not listed in Info.plist" ;;
    esac
    if [[ ! -f "$bin" ]]; then
        echo "no binary at $slice/llama.framework/llama"
        return
    fi
    [[ -f "$FRAMEWORK/$slice/llama.framework/Headers/llama.h" ]] \
        || echo "no Headers/llama.h"

    # Each tool's output is read whole before it is searched: grep -q in a
    # pipe stops reading at its match, and the tool's SIGPIPE would then fail
    # the pipe under pipefail.
    local archs arch header platforms exported sym
    archs=$(lipo -archs "$bin" 2>/dev/null | tr ' ' '\n' | sort | tr '\n' ' ' || true)
    archs="${archs% }"
    [[ "$archs" == "$want_archs" ]] \
        || echo "architectures '$archs', wanted '$want_archs'"
    for arch in $archs; do
        header=$(otool -arch "$arch" -hv "$bin")
        grep -qw DYLIB <<<"$header" || echo "$arch is not a dynamic library"
        platforms=$(otool -arch "$arch" -l "$bin" | awk '/^ *platform /{print $2}' | sort -u | tr '\n' ' ')
        platforms="${platforms% }"
        [[ "$platforms" == "$platform" ]] \
            || echo "$arch is built for platform '$platforms', wanted $platform ($platform_name)"
        exported=$(nm -gUj -arch "$arch" "$bin")
        for sym in $SYMBOLS; do
            grep -qx "_$sym" <<<"$exported" || echo "$arch does not export $sym"
        done
    done
}

while IFS=: read -r slice platform platform_name want_archs; do
    problems=$(slice_problems "$slice" "$platform" "$platform_name" "$want_archs")
    if [[ -z "$problems" ]]; then
        printf '  ok       %-28s %-14s %-13s dynamic, exports %s llama_* functions\n' \
            "$slice" "[$want_archs]" "$platform_name" "$symbol_count"
    else
        printf '  WRONG    %s\n' "$slice"
        printf '%s\n' "$problems" | sed 's/^/             /'
        fail=1
    fi
done <<<"$SLICES"

for id in $listed; do
    grep -q "^$id:" <<<"$SLICES" || printf '  extra    %s\n' "$id"
done

if [[ $fail -ne 0 ]]; then
    echo "error: $FRAMEWORK lacks what the phone engine links against." >&2
    exit 1
fi
echo "Every slice the phone engine links is there and exports what it calls."
