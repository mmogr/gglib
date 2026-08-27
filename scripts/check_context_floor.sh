#!/usr/bin/env bash
# Fail if anything outside the resolver fabricates the built-in context floor.
#
# The pattern is always the same shape:
#
#     let default_ctx = settings
#         .and_then(|s| s.default_context_size)
#         .unwrap_or(DEFAULT_CONTEXT_SIZE);
#
# which reads as a harmless default and is not one. `default_context_size` is
# `None` in the ordinary case — #926 stopped `Settings::with_defaults` writing a
# number, precisely so the rungs below it stay reachable — so `.unwrap_or` here
# turns "the user chose nothing" into "the user chose 4096". That value then
# travels as a *chosen* one, and `resolve_context_size` ranks it above the
# fitted rung, which makes every rung below it dead code.
#
# This has shipped three times.
#
#   #925  `resolve_launch_opts` set `global_default_ctx = Some(default_ctx)`
#         from a non-optional `u64`, so the fitted rung it had just added was
#         unreachable on the path that added it.
#   #926  `gglib proxy` pre-resolved the chain and sent the answer in the
#         explicit slot, where the daemon's own filter could not see it.
#   #934  the three benchmark entry points did it and then passed the result as
#         `num_ctx` — the top rung — so the fit was computed and thrown away on
#         every benchmark launch, and the resident it produced disagreed with
#         the one the proxy wanted. They share a `ProcessManager`, so that
#         disagreement is an evict and a relaunch, in both directions.
#
# Each was found by reading, months apart, and each looked correct in review.
# The construct is the tell, so the construct is what is checked.
#
# ── What is allowed ──────────────────────────────────────────────────────────
#
# `server_config.rs` is the one place the floor belongs: it is the last rung of
# the chain, and returning it there is the whole point. Test doubles may use it
# too — a fake runtime has no machine to fit against — so files whose match sits
# inside a `#[cfg(test)]` module are allowed by path allowlist rather than by
# parsing, which would be worse than the check.
#
# Everything else should pass `Option<u64>` through untouched and let admission
# resolve it. That is the only place that can reach the fitted rung.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Paths permitted to name the floor as a fallback, with the reason.
ALLOWED=(
  "crates/gglib-core/src/server_config.rs"          # the chain's last rung
  "crates/gglib-core/src/ports/model_runtime.rs"    # MinimalRuntime, a test double
)

is_allowed() {
  local path="$1"
  for a in "${ALLOWED[@]}"; do
    [ "$path" = "$a" ] && return 0
  done
  return 1
}

# `unwrap_or`/`unwrap_or_else`/`unwrap_or_default` reaching for the floor.
# Comment lines are skipped: this file and the three it cites all describe the
# construct in prose, and a checker that cannot tell prose from code would make
# writing that prose impossible.
PATTERN='unwrap_or(_else)?\s*\(\s*(gglib_core::settings::|crate::settings::)?DEFAULT_CONTEXT_SIZE'

echo "Checking for fabricated context floors..."
echo "========================================"

failed=false
while IFS=: read -r path line text; do
  [ -z "${path:-}" ] && continue
  stripped="$(printf '%s' "$text" | sed 's/^[[:space:]]*//')"
  case "$stripped" in
    '//'*|'///'*|'//!'*|'*'*) continue ;;
  esac
  if is_allowed "$path"; then
    continue
  fi
  echo "❌ $path:$line"
  echo "   $stripped"
  failed=true
done < <(cd "$ROOT_DIR" && grep -rnE --include='*.rs' "$PATTERN" crates/ || true)

if [ "$failed" = true ]; then
  cat <<'EOF'

✗ A context floor is being fabricated outside the resolver.

  `default_context_size: None` means the user chose nothing, and the rungs
  below it — a per-model default, and the context fitted to this machine —
  exist to answer that case. Substituting the floor here ranks it above both.

  Pass the `Option<u64>` through untouched and let `admit` resolve the chain.
  If this really is the last rung, add the path to ALLOWED with the reason.
EOF
  exit 1
fi

echo "✅ only the resolver names the built-in floor"
