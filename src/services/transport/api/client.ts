/**
 * HTTP client for API transport.
 * 
 * Handles embedded API discovery in Tauri mode, bearer token authentication,
 * and recoverable error handling with single retry on 401/network errors.
 */

import { readData } from '../errors';

/**
 * Module-level API session context.
 * Shared between HTTP client and SSE streaming.
 */
let apiBaseUrl = '';
let apiAuthToken: string | undefined;

/**
 * Set the API session context.
 * Called during transport initialization to establish baseUrl and auth token.
 */
export function setApiSession(baseUrl: string, authToken?: string): void {
  apiBaseUrl = baseUrl;
  apiAuthToken = authToken;
  
  if (import.meta.env.DEV) {
    console.debug('[ApiClient] API session set:', {
      baseUrl,
      hasToken: !!authToken,
    });
  }
}

/**
 * Get the current API base URL.
 * Used by SSE and other fetch-based utilities.
 */
export function getApiBaseUrl(): string {
  return apiBaseUrl;
}

/**
 * Get auth headers for the current API session.
 * Used by SSE and other fetch-based utilities that need authentication.
 */
export function getAuthHeaders(): HeadersInit {
  return apiAuthToken ? { Authorization: `Bearer ${apiAuthToken}` } : {};
}

/**
 * Embedded API info discovered from Tauri.
 */
interface EmbeddedApiInfo {
  port: number;
  token: string;
}

/**
 * HTTP client configuration.
 */
interface HttpClientConfig {
  baseUrl: string;
  token?: string;
}

/**
 * HTTP client with automatic auth injection.
 */
export interface HttpClient {
  request<T>(path: string, options?: RequestOptions, isRetry?: boolean): Promise<T>;
}

/**
 * Request options for HTTP client.
 */
interface RequestOptions {
  method?: 'GET' | 'POST' | 'PUT' | 'DELETE';
  body?: unknown;
}

/**
 * Detect if running in Tauri environment.
 */
function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/**
 * Invoke Tauri command.
 */
async function invokeTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  // @ts-expect-error - Tauri API is injected at runtime
  const { invoke } = window.__TAURI_INTERNALS__;
  return invoke(cmd, args);
}

/**
 * Discover embedded API info from Tauri.
 * Throws on failure and clears cache to allow retry.
 */
async function discoverEmbeddedApi(): Promise<EmbeddedApiInfo> {
  try {
    if (import.meta.env.DEV) {
      console.debug('[ApiClient] Discovering embedded API...');
    }
    
    const info = await invokeTauri<EmbeddedApiInfo>('get_embedded_api_info');
    
    if (import.meta.env.DEV) {
      console.debug('[ApiClient] Embedded API discovered', {
        port: info.port,
        tokenPrefix: info.token.substring(0, 8) + '...',
      });
    }
    
    return info;
  } catch (error) {
    console.error('[ApiClient] Failed to discover embedded API:', error);
    throw error;
  }
}

/**
 * Cached client promise for lazy initialization.
 */
let cachedClientPromise: Promise<HttpClient> | null = null;

/**
 * Reset cached client (used on auth/network errors for retry).
 */
function resetClientCache(): void {
  if (import.meta.env.DEV) {
    console.debug('[ApiClient] Resetting client cache for retry');
  }
  cachedClientPromise = null;
}

/**
 * Build HTTP client from configuration.
 */
function buildClient(config: HttpClientConfig): HttpClient {
  const { baseUrl, token } = config;
  
  /**
   * Get headers for request.
   */
  function getHeaders(includeContentType: boolean): HeadersInit {
    const headers: HeadersInit = {};
    
    if (includeContentType) {
      headers['Content-Type'] = 'application/json';
    }
    
    if (token) {
      headers['Authorization'] = `Bearer ${token}`;
      
      // Debug log in dev mode
      if (import.meta.env.DEV) {
        console.debug('[ApiClient] Request headers:', {
          hasAuth: !!headers['Authorization'],
          tokenPrefix: token.substring(0, 8) + '...',
          contentType: headers['Content-Type'],
        });
      }
    }
    
    return headers;
  }
  
  /**
   * Make HTTP request with automatic retry on 401/network errors.
   */
  async function request<T>(
    path: string, 
    options?: RequestOptions,
    isRetry = false
  ): Promise<T> {
    const { method = 'GET', body } = options || {};
    const hasBody = body !== undefined;
    
    // Include Content-Type header for POST/PUT/DELETE requests (even if body is undefined)
    // Backend may expect application/json header to parse Json<Option<T>> types
    const shouldIncludeContentType = method !== 'GET';
    
    try {
      const response = await fetch(`${baseUrl}${path}`, {
        method,
        headers: getHeaders(shouldIncludeContentType),
        body: hasBody ? JSON.stringify(body) : undefined,
      });
      
      // If 401 and not already retrying, clear cache and retry once
      if (response.status === 401 && !isRetry && isTauri()) {
        console.warn('[ApiClient] 401 Unauthorized - clearing cache and retrying');
        resetClientCache();
        const newClient = await getClient();
        return newClient.request<T>(path, options, true);
      }
      
      return await readData<T>(response);
    } catch (error) {
      // On network error (ECONNREFUSED, etc), clear cache and retry once
      if (!isRetry && isTauri() && error instanceof TypeError) {
        console.warn('[ApiClient] Network error - clearing cache and retrying:', error.message);
        resetClientCache();
        const newClient = await getClient();
        return newClient.request<T>(path, options, true);
      }
      
      throw error;
    }
  }
  
  return { request };
}

/**
 * Get or create HTTP client.
 * 
 * In Tauri mode: discovers embedded API once and caches result.
 * In web mode: uses empty base URL (relative paths).
 * 
 * Automatically retries once on 401 or network errors by clearing cache.
 */
export async function getClient(): Promise<HttpClient> {
  if (cachedClientPromise) {
    return cachedClientPromise;
  }
  
  cachedClientPromise = (async () => {
    try {
      if (isTauri()) {
        const info = await discoverEmbeddedApi();
        const config = {
          baseUrl: `http://127.0.0.1:${info.port}`,
          token: info.token,
        };
        // Set module-level session for SSE and other fetch-based utilities
        setApiSession(config.baseUrl, config.token);
        return buildClient(config);
      } else {
        // Web mode: same-origin, no token
        setApiSession('', undefined);
        return buildClient({ baseUrl: '' });
      }
    } catch (error) {
      // Clear cache on discovery failure to allow retry
      cachedClientPromise = null;
      throw error;
    }
  })();
  
  return cachedClientPromise;
}

/**
 * Helper for GET requests.
 */
export async function get<T>(path: string): Promise<T> {
  const client = await getClient();
  return client.request<T>(path);
}

/**
 * Helper for POST requests.
 */
export async function post<T>(path: string, body?: unknown): Promise<T> {
  const client = await getClient();
  // Some backend handlers use Json<Option<T>>; they require valid JSON even when
  // "no body" is intended. Sending `null` is valid JSON and deserializes to None.
  return client.request<T>(path, { method: 'POST', body: body === undefined ? null : body });
}

/**
 * Helper for PUT requests.
 */
export async function put<T>(path: string, body: unknown): Promise<T> {
  const client = await getClient();
  return client.request<T>(path, { method: 'PUT', body });
}

/**
 * Helper for DELETE requests.
 */
/**
 * DELETE request.
 */
export async function del<T>(path: string, body?: unknown): Promise<T> {
  const client = await getClient();
  return client.request<T>(path, { method: 'DELETE', body: body === undefined ? null : body });
}

/**
 * Get base URL and auth headers for direct fetch calls (e.g., streaming).
 * 
 * @returns Object with baseUrl and headers for authentication
 */
export async function getAuthenticatedFetchConfig(): Promise<{
  baseUrl: string;
  headers: HeadersInit;
}> {
  if (isTauri()) {
    const info = await discoverEmbeddedApi();
    return {
      baseUrl: `http://127.0.0.1:${info.port}`,
      headers: {
        'Authorization': `Bearer ${info.token}`,
      },
    };
  } else {
    return {
      baseUrl: '',
      headers: {},
    };
  }
}
