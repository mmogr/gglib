/**
 * When a change to a setting takes effect, for the settings that do not take
 * effect at once, each read from the Rust that reads it:
 *
 * - the endpoint's port, address, allowed hosts and prompt cache become its
 *   config when it turns on (`gglib-app-services/src/proxy_settings.rs`);
 * - the default context is read at every load the app makes
 *   (`gglib-axum/src/handlers/servers.rs`, a device's chat load in
 *   `hub_model.rs`), but a model the endpoint loads for a client takes the
 *   value the endpoint turned on with (`gglib-proxy/src/server.rs`);
 * - the model servers' base port is fixed when gglib starts its services
 *   (`service_graph.rs`), and the endpoint autostart is read then too
 *   (`gglib-axum/src/daemon/mod.rs`);
 * - the login item is registered when the desktop app starts
 *   (`src-tauri/src/autostart.rs`);
 * - the API key is read again on every request, through the settings cache,
 *   but the key in force when the endpoint bound stays required until it
 *   turns on again (`gglib-core/src/access/bearer.rs`).
 *
 * A setting with no note here is read where it is used, on the next request,
 * run or close.
 *
 * @module places/settings/notes
 */

export const ON_ENDPOINT_START = 'Takes effect when the endpoint next turns on.';
export const ON_GGLIB_START = 'Takes effect when gglib next starts.';
export const ON_NEXT_LOAD =
  'Takes effect for the next model loaded. A model the endpoint loads for a client keeps the value the endpoint turned on with.';
export const ON_DESKTOP_START = 'Takes effect when the desktop app next starts.';
export const ON_KEY_CHANGE =
  'A new key takes effect within a few seconds. A cleared key stops being required when the endpoint next turns on.';
