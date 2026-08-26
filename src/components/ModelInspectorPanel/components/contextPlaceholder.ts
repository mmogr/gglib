import type { GgufModel, AppSettings } from '../../../types';

/**
 * What leaving the Context Length box empty will actually get you.
 *
 * The branches walk the daemon's resolution ladder
 * (`resolve_context_size_with_source`, `crates/gglib-core/src/server_config.rs`)
 * from
 * the rung below `Explicit` downwards: per-model server default → global
 * default → whatever the server settles on. `Explicit` is the rung the box
 * itself fills, and the Serve action now puts nothing else there — neither the
 * stored default nor the GGUF's trained window, both of which it used to.
 * See ADR 0009.
 *
 * One answer serves both launch paths, which is only true because they agree:
 * a pinned serve plans through `plan_pinned_launch` and a bare one through
 * `ServerOps::launch_overrides`, and each now receives the typed value alone.
 * While they disagreed, no single string could be right for both — the pinned
 * path already discarded the stored default while the bare one sent it.
 *
 * The last branch deliberately does **not** name the fitted rung, even though
 * that is the interesting one and ADR 0009 exists to introduce it. Whether the
 * fit is reachable is not a fact this client holds. `fit_context` returns
 * `None` unless it can read the device budget, the model's weight size and its
 * KV geometry — and also when the weights alone exceed the budget, which is
 * simply a model too big for the card and is the likeliest of the five. ADR
 * 0009 records on top of that that AMD, Intel, Vulkan and CPU-only hosts get
 * no fit at all and fall through to the 4096 floor. Saying "fitted to this
 * machine" would be a false promise to every one of those users, on the
 * surface this module exists to stop making false promises. `gpuMemoryBytes`
 * would answer the hardware half — `useSystemMemory` already reads it — but
 * none of the rest, so the honest claim is the one that holds everywhere:
 * the server decides.
 */
export function contextPlaceholder(model: GgufModel, settings: AppSettings | null): string {
  if (model.serverDefaults?.contextLength) {
    return `Model default: ${model.serverDefaults.contextLength.toLocaleString()}`;
  }
  if (settings?.defaultContextSize) {
    return `Default: ${settings.defaultContextSize.toLocaleString()}`;
  }
  return 'Sized per launch';
}
