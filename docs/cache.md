# KV cache tiering

gglib manages llama-server's cache stack across three tiers, each sized
automatically and each overridable:

1. **KV cache quantization** — the in-VRAM KV cache itself, quantized to
   halve its footprint.
2. **Host-RAM prompt cache** — llama-server's `--cache-ram`, auto-sized on
   every launch.
3. **Disk slot persistence** — opt-in per-session slot files that survive
   model swaps and restarts.

## KV cache quantization

Every launch defaults the KV cache to `q8_0` quantization on both K and V
(`--cache-type-k`/`--cache-type-v`), roughly halving KV memory versus
llama-server's own `f16` default — which directly buys context headroom on a
16–24 GB card. Override per-axis with the same flags, or set
`GGLIB_DISABLE_KV_QUANT=1` to fall back to `f16`.

## Host-RAM prompt cache auto-sizing

Independently of disk persistence, every launch auto-sizes llama-server's own
host-RAM prompt cache (`--cache-ram`) from system RAM, the model's weights, and
its KV footprint — override with `--cache-ram-mb`.

## Disk slot persistence

Available on `gglib serve` and `gglib proxy` alike, and opt-in on both. For
sequential multi-agent workflows, enable `--cache --slot-dir <path>` to persist
KV cache state between requests. The proxy automatically saves and restores per-session
slot files (saved atomically via a temp-then-rename), gated by a semaphore to prevent
concurrent access. Stale caches are detected via mtime comparison and skipped
(fail-open). A background sweep evicts the least-recently-used slot files once the
on-disk cache exceeds a byte budget — by default auto-sized from free disk space,
override with `--cache-disk-gb`. Use `gglib proxy cache-clear` to manually clear
cached state.

### Models where disk persistence is skipped

Disk persistence is skipped automatically for models whose attention keeps only
part of the token history — sliding-window, hybrid (e.g. a GGUF declaring
`full_attention_interval`), and recurrent/SSM architectures. llama-server's slot
files carry KV state and tokens but not the context checkpoints those models need
to resume, so a restore cannot pick up where it left off and instead re-prefills
the whole prompt, while also crowding out the host-RAM prompt cache that *would*
have resumed cheaply. Such models rely on the RAM cache alone; the proxy logs the
decision at startup. Set `GGLIB_FORCE_HYBRID_DISK_CACHE=1` to re-enable the disk
layer anyway (intended for testing an upstream llama.cpp fix).

## Observability

The [proxy dashboard](../crates/gglib-proxy/README.md#proxy-dashboard) reports
how the cache resolved and how much it is actually doing: the RAM budget,
whether either tier is degraded, and measured reuse (prompt tokens served from
cache vs. re-processed, per request and in total). These are raw counts taken
from the upstream's own `usage` reporting — there is no estimated "time saved",
since reuse is measured exactly but what it saved depends on a prefill that
never ran.
