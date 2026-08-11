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
  host: string;
  port: number;
  default_context: number;
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
  port: number;
  current_model?: string;
  model_port?: number;
  /** The model this proxy run is pinned to; absent for the auto-swapping proxy. */
  pinned_model?: string | null;
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
