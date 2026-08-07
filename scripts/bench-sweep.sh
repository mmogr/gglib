#!/usr/bin/env bash
# bench-sweep.sh - Run the raw-vs-gglib A/B agentic eval across several models
#
# Usage: ./scripts/bench-sweep.sh [-o OUTDIR] [--ctx-size N] MODEL [MODEL ...]
#   -o, --output-dir  where the per-model JSON reports land (default: bench/)
#       --ctx-size    context size override, passed through to each run
#   MODEL             model name or database ID, as shown by `gglib model list`
#
# Each model runs the 9-task suite twice (pipeline bypassed, then through it),
# so budget roughly the raw arm's wall time per model — tens of minutes on a
# model that drifts. A model that fails does not abort the sweep; the failures
# are listed at the end and its report is simply absent from the table.
#
# Requires: gglib on PATH, python3

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

OUTDIR="bench"
CTX_ARGS=()
MODELS=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help)
            sed -n '2,15p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        -o|--output-dir)
            [[ $# -ge 2 ]] || { echo "Error: $1 needs a value" >&2; exit 1; }
            OUTDIR="$2"; shift 2
            ;;
        --ctx-size)
            [[ $# -ge 2 ]] || { echo "Error: $1 needs a value" >&2; exit 1; }
            CTX_ARGS=(--ctx-size "$2"); shift 2
            ;;
        -*)
            echo "Error: unknown option $1" >&2; exit 1
            ;;
        *)
            MODELS+=("$1"); shift
            ;;
    esac
done

if [[ ${#MODELS[@]} -eq 0 ]]; then
    echo "Error: no models given. See \`gglib model list\` for names." >&2
    echo "Usage: ./scripts/bench-sweep.sh [-o OUTDIR] MODEL [MODEL ...]" >&2
    exit 1
fi

if ! command -v gglib &> /dev/null; then
    echo "Error: gglib not on PATH. Build and install it with: make setup" >&2
    exit 1
fi

if ! command -v python3 &> /dev/null; then
    echo "Error: python3 not found (needed to build the summary table)" >&2
    exit 1
fi

cd "$REPO_ROOT"
mkdir -p "$OUTDIR"

echo "=== Agentic A/B sweep: ${#MODELS[@]} model(s) → ${OUTDIR}/ ==="
echo ""

REPORTS=()
FAILED=()

for model in "${MODELS[@]}"; do
    # Model names can carry / and : (HF paths, Ollama-style tags); flatten so
    # the report is always one file in OUTDIR rather than an implied subtree.
    safe="${model//[^A-Za-z0-9._-]/_}"
    report="${OUTDIR}/${safe}.json"

    echo "--- ${model}"
    # ${arr[@]+"${arr[@]}"} rather than "${arr[@]}": under `set -u`, bash 3.2
    # (what macOS ships) treats an empty array as an unbound variable.
    if gglib benchmark agentic --model "$model" ${CTX_ARGS[@]+"${CTX_ARGS[@]}"} --output "$report"; then
        REPORTS+=("$report")
        echo "    ✓ ${report}"
    else
        FAILED+=("$model")
        echo "    ✗ failed — continuing" >&2
    fi
    echo ""
done

if [[ ${#REPORTS[@]} -eq 0 ]]; then
    echo "No successful runs; nothing to summarize." >&2
    exit 1
fi

TABLE="${OUTDIR}/TABLE.md"
python3 "${SCRIPT_DIR}/bench_table.py" "${REPORTS[@]}" > "$TABLE"

echo "=== Summary ==="
echo "Reports: ${#REPORTS[@]}   Failed: ${#FAILED[@]}"
if [[ ${#FAILED[@]} -gt 0 ]]; then
    printf '  failed: %s\n' "${FAILED[@]}"
fi
echo "Table:   ${TABLE}"
echo ""
cat "$TABLE"
