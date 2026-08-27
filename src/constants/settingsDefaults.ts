/**
 * Default values and accepted ranges for the numeric fields in the settings modal.
 *
 * The authoritative values live in Rust — `Settings::with_defaults()` and
 * `validate_settings()` in `crates/gglib-core/src/settings.rs`. These entries
 * mirror them so the GUI can show the user what the backend will fall back to
 * and reject out-of-range input before a round trip.
 *
 * Mirroring is not enforcement: nothing here fails if the Rust side changes.
 * What this module buys is that a Rust-side change is a one-file fix instead of
 * a hunt through five JSX literals, and that each value is written exactly once
 * (the ranges used to be spelled out twice per field — as `min`/`max` props and
 * again in prose).
 *
 * Values are strings because they feed `<input type="number">` attributes.
 */

/** The range a numeric settings input accepts, shared by both spec shapes. */
interface NumericBounds {
  readonly min: string;
  readonly max: string;
}

/**
 * What a field does when it is left empty, for fields with no fixed default.
 *
 * Both halves are required. A field that renders neither a number nor a
 * sentence loses its hint row and its placeholder silently, and nothing in the
 * type system or the tests would say so — hence the union below rather than an
 * optional property beside a nullable one.
 */
interface UnsetBehaviour {
  /** Short token shown inside the box. The control column is narrow. */
  readonly placeholder: string;
  /** Sentence shown under the field in place of "Default: N". */
  readonly hint: string;
}

/**
 * Default value plus the range accepted by a numeric settings input.
 *
 * Either the field has a fixed default the backend falls back to, or it has
 * none and must say what happens instead. The context window is the second
 * kind: it is resolved per launch — from the model and the machine, or from
 * the built-in floor on a host whose device gglib cannot read — so there is no
 * one number to promise.
 */
export type NumericSettingSpec =
  | (NumericBounds & { readonly default: string; readonly unset?: never })
  | (NumericBounds & { readonly default: null; readonly unset: UnsetBehaviour });

/**
 * OpenAI-compatible proxy listener.
 *
 * Rust: `DEFAULT_PROXY_PORT`; ports below 1024 are rejected by
 * `validate_settings`, and the field is a `u16`.
 */
export const PROXY_PORT: NumericSettingSpec = {
  default: '8080',
  min: '1024',
  max: '65535',
};

/**
 * First port handed out to llama-server instances.
 *
 * Rust: `DEFAULT_LLAMA_BASE_PORT`; same bounds as the proxy port.
 */
export const LLAMA_BASE_PORT: NumericSettingSpec = {
  default: '9000',
  min: '1024',
  max: '65535',
};

/**
 * How many model downloads may be queued at once.
 *
 * Rust: literal `10` in `with_defaults()`, range `1..=50` in `validate_settings`.
 */
export const MAX_DOWNLOAD_QUEUE_SIZE: NumericSettingSpec = {
  default: '10',
  min: '1',
  max: '50',
};

/**
 * Context window applied to a model with no per-model override.
 *
 * Rust: unset in `Settings::with_defaults()`, range `512..=1_000_000` in
 * `validate_settings`. Deliberately has no default: leaving this empty is what
 * lets the daemon size each launch, and a stored number — including one this
 * box put there — outranks that.
 *
 * "Size", not "fit", and the hint is hedged for the same reason: the fit is
 * not always reachable. `fit_context` returns `None` unless it can read the
 * device budget, the weight size and the KV geometry, and ADR 0009 records
 * that AMD, Intel, Vulkan and CPU-only hosts get no fit at all and fall
 * through to the 4096 floor. An unhedged "the context is fitted to this
 * machine" promised those users something they never receive, on the control
 * that governs the rung.
 */
export const CONTEXT_SIZE: NumericSettingSpec = {
  default: null,
  unset: {
    placeholder: 'auto',
    hint: 'Left empty, the server sizes each launch — fitted to this machine where gglib can read the device, and the built-in floor where it cannot.',
  },
  min: '512',
  max: '1000000',
};

/**
 * Tool-call rounds the agentic loop will run before giving up.
 *
 * Rust: `domain::agent::DEFAULT_MAX_ITERATIONS`. Note the range is a UI-side
 * guard rail only — `validate_settings` does not bound this field.
 */
export const MAX_TOOL_ITERATIONS: NumericSettingSpec = {
  default: '25',
  min: '1',
  max: '50',
};

/**
 * Maximum consecutive no-progress agent steps. No `validate_settings` entry
 * on the Rust side — the ceiling here mirrors
 * `MAX_STAGNATION_STEPS_CEILING` in `domain/agent/config.rs`.
 */
export const MAX_STAGNATION_STEPS: NumericSettingSpec = {
  default: '5',
  min: '1',
  max: '100',
};
