/**
 * Proxy transport sub-interface.
 * Handles multi-model proxy server management.
 */

import type { InferenceConfig } from '../../../types';
import type { StartServerRequest } from '../mappers';

/**
 * Proxy server configuration — the full `StartProxyConfig` wire surface
 * (snake_case keys, matching the Rust serde form).
 */
export interface ProxyConfig {
  /** Omitted resolves to `127.0.0.1`. */
  host?: string;
  /** Omitted falls through to the saved `proxy_port`, then the built-in default. */
  port?: number;
  /**
   * First port of the range spawned llama.cpp servers are allocated from.
   *
   * Read only by `POST /api/proxy/start-pinned`, which routes it through the
   * launch cascade. `POST /api/proxy/start` deserializes it and then never
   * looks at it — `to_runtime_config` does not carry it — so sending it there
   * is accepted and silently ignored.
   */
  llama_base_port?: number;
  /** Omitted resolves through the saved default context size. */
  default_context?: number;
  /** Master switch for disk KV-slot persistence. */
  cache?: boolean;
  /** Slot directory; omitted = default when cache is on. */
  slot_dir?: string;
  /** Byte budget (GiB) for the on-disk slot eviction sweep. */
  cache_disk_gb?: number;
  /** Proxy-wide sampling override (camelCase inner keys — `InferenceConfig`
      serializes camelCase on both sides, so this passes through unmapped). */
  inference_override?: InferenceConfig;
  /** Require this bearer key on the OpenAI surface. */
  api_key?: string;
  /** Extra Host-header allowlist entries beyond loopback. */
  allowed_hosts?: string[];
  /**
   * Pin this run to a single model, refusing every request for another —
   * `gglib serve`'s guarantee, offered on the ordinary start route.
   *
   * The GUI reaches pinned mode through `POST /api/proxy/start-pinned`
   * instead, which resolves the launch cascade server-side; this is the
   * lower-level form that path delegates to.
   */
  pinned?: {
    /** The name clients must address the model by. Matched exactly. */
    name: string;
    /**
     * Standing launch options, already cascade-resolved by the caller.
     *
     * Mirrors Rust `ServerConfigOptions`, which has no TypeScript
     * counterpart — it is a large type no frontend surface builds by hand,
     * and PR 4's generated bindings will supply the real one. Left open
     * rather than half-mirrored: a partial copy is the drift this file exists
     * to stop.
     */
    launch_overrides?: Record<string, unknown>;
  };
}

/**
 * Request body for `POST /api/proxy/start-pinned` — the GUI counterpart of
 * `gglib serve`. The daemon runs the launch cascade server-side.
 */
export interface StartPinnedRequest {
  model_id: number;
  /** Per-model launch overrides, same camelCase shape as `/api/servers/start`. */
  options?: StartServerRequest;
  proxy?: Partial<ProxyConfig>;
}

/**
 * Proxy server status.
 */
export interface ProxyStatus {
  running: boolean;
  /**
   * `null` whenever the proxy is not running — a stopped and a crashed proxy
   * report identically, neither having a port to hand out.
   *
   * `running` is the guard: the daemon sets a port exactly when it sets
   * `running: true`, so a truthy `running` means this is a number.
   */
  port: number | null;
  /**
   * Always `null` today. The daemon hard-codes both this and
   * {@link model_port} rather than reporting the swapping proxy's current
   * upstream; the keys are on the wire, the answers are not.
   */
  current_model: string | null;
  /** Always `null` today — see {@link current_model}. */
  model_port: number | null;
  /** The model this proxy run is pinned to; `null` for the auto-swapping proxy. */
  pinned_model: string | null;
}

/**
 * Proxy transport operations.
 */
export interface ProxyTransport {
  /** Get current proxy status. */
  getProxyStatus(): Promise<ProxyStatus>;

  /** Start the multi-model proxy server. */
  startProxy(config?: Partial<ProxyConfig>): Promise<ProxyStatus>;

  /** Start the proxy pinned to one model — the GUI counterpart of `gglib serve`. */
  startPinnedProxy(request: StartPinnedRequest): Promise<ProxyStatus>;

  /** Stop the proxy server. */
  stopProxy(): Promise<void>;

  /**
   * Shut the daemon down — `gglib daemon stop`.
   *
   * Stops every running inference server with it, and the app loses its
   * backend until the daemon is started again. Confirm before calling.
   */
  shutdownDaemon(): Promise<void>;
}
