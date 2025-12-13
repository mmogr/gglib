/**
 * Proxy transport sub-interface.
 * Handles multi-model proxy server management.
 */

/**
 * Proxy server configuration.
 */
export interface ProxyConfig {
  host: string;
  port: number;
  default_context: number;
}

/**
 * Proxy server status.
 */
export interface ProxyStatus {
  running: boolean;
  port: number;
  current_model?: string;
  model_port?: number;
}

/**
 * Proxy transport operations.
 */
export interface ProxyTransport {
  /** Get current proxy status. */
  getProxyStatus(): Promise<ProxyStatus>;

  /** Start the multi-model proxy server. */
  startProxy(config?: Partial<ProxyConfig>): Promise<ProxyStatus>;

  /** Stop the proxy server. */
  stopProxy(): Promise<void>;
}
