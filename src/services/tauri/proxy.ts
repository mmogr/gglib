// Proxy domain operations
// Multi-model proxy server management

import { apiFetch, isTauriApp, tauriInvoke, ApiResponse } from "./base";

export interface ProxyConfig {
  host: string;
  port: number;
  start_port: number;
  default_context: number;
}

export interface ProxyStatus {
  running: boolean;
  port: number;
}

/**
 * Get the current proxy server status.
 */
export async function getProxyStatus(): Promise<ProxyStatus> {
  if (isTauriApp) {
    return await tauriInvoke<ProxyStatus>('get_proxy_status');
  } else {
    const response = await apiFetch(`/proxy/status`);
    if (!response.ok) {
      throw new Error(`Failed to get proxy status: ${response.statusText}`);
    }
    const data: ApiResponse<ProxyStatus> = await response.json();
    return data.data || { running: false, port: 8080 };
  }
}

/**
 * Start the proxy server.
 */
export async function startProxy(config: ProxyConfig): Promise<string> {
  if (isTauriApp) {
    return await tauriInvoke<string>('start_proxy', {
      host: config.host,
      port: config.port,
      startPort: config.start_port,
      defaultContext: config.default_context,
    });
  } else {
    const response = await apiFetch(`/proxy/start`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(config),
    });

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to start proxy');
    }

    const data: ApiResponse<string> = await response.json();
    return data.data || 'Proxy started';
  }
}

/**
 * Stop the proxy server.
 */
export async function stopProxy(): Promise<string> {
  if (isTauriApp) {
    return await tauriInvoke<string>('stop_proxy');
  } else {
    const response = await apiFetch(`/proxy/stop`, {
      method: 'POST',
    });

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to stop proxy');
    }

    const data: ApiResponse<string> = await response.json();
    return data.data || 'Proxy stopped';
  }
}
