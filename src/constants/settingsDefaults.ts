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

/** Default value plus the range accepted by a numeric settings input. */
export interface NumericSettingSpec {
  /** Value the backend falls back to when the field is left empty. */
  readonly default: string;
  readonly min: string;
  readonly max: string;
}

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
 * Rust: `DEFAULT_CONTEXT_SIZE`, range `512..=1_000_000` in `validate_settings`.
 */
export const CONTEXT_SIZE: NumericSettingSpec = {
  default: '4096',
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
