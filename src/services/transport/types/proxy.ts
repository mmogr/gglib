/**
 * Proxy transport types.
 * Handles multi-model proxy server management.
 */

import type { StartServerRequest } from '../mappers';

/**
 * Proxy server configuration — the body of `POST /api/proxy/start`.
 *
 * `Partial` over the generated shape, and the wrapper is the accurate part.
 * ts-rs renders every field as required: an `Option<T>` becomes `T | null`,
 * and `allowed_hosts: Vec<String>` becomes a plain `Array<string>`. The
 * handler accepts all of them omitted — the struct derives `Default`, every
 * non-`Option` field carries `#[serde(default)]`, and serde fills a missing
 * `Option` with `None` on its own — so a caller sending `{}` is starting the
 * proxy on saved settings, which is the ordinary case.
 *
 * One field is worth knowing before sending it: `llama_base_port` is read
 * only by `POST /api/proxy/start-pinned`, which routes it through the launch
 * cascade. `POST /api/proxy/start` deserializes it and never looks at it, so
 * sending it there is accepted and silently ignored.
 *
 * `pinned.launch_overrides` is now the real `ServerConfigOptions` rather than
 * the `Record<string, unknown>` this file left open pending the generator.
 */
import type { StartProxyConfig } from '../../../types/generated/StartProxyConfig';
export type ProxyConfig = Partial<StartProxyConfig>;

export type { PinnedSpec } from '../../../types/generated/PinnedSpec';
export type { ServerConfigOptions } from '../../../types/generated/ServerConfigOptions';

/**
 * Request body for `POST /api/proxy/start-pinned` — the GUI counterpart of
 * `gglib serve`. The daemon runs the launch cascade server-side.
 *
 * `model_id` is the only required key. `options` and `proxy` both carry
 * `#[serde(default)]`, which ts-rs does not render as optional — so the
 * generated body demands two objects the handler is happy to construct
 * itself, and the GUI's own pinned-start call omits `proxy` entirely.
 *
 * `options` keeps the local `StartServerRequest` rather than the generated
 * one for the same reason: its fields are `Option` with defaults, and the
 * mapper builds a sparse object.
 */
import type { StartPinnedBody } from '../../../types/generated/StartPinnedBody';
export type StartPinnedRequest = Pick<StartPinnedBody, 'model_id'> &
  Partial<{
    /** Per-model launch overrides, same camelCase shape as `/api/servers/start`. */
    options: StartServerRequest;
    proxy: ProxyConfig;
  }>;

/**
 * Proxy server status.
 *
 * Two things the generated type states as `T | null` without saying why, and
 * which a reader needs:
 *
 * - `running` is the guard for `port`. The daemon sets a port exactly when it
 *   sets `running: true` — a stopped proxy and a crashed one report
 *   identically, neither having a port to hand out — so a truthy `running`
 *   means `port` is a number. TypeScript cannot express that pairing across
 *   two fields, so the narrowing has to be written at each call site.
 * - `current_model` and `model_port` are always `null` today. The daemon
 *   hard-codes both rather than reporting the swapping proxy's current
 *   upstream; the keys are on the wire, the answers are not.
 */
import type { ProxyStatus } from '../../../types/generated/ProxyStatus';
export type { ProxyStatus };
