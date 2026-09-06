/**
 * HTTP client for API transport.
 * 
 * Handles embedded API discovery in Tauri mode, bearer token authentication,
 * and recoverable error handling with single retry on 401/network errors.
 */

import { readData } from '../errors';
import { appLogger } from '../../platform';

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
  
  appLogger.debug('transport.api', '[ApiClient] API session set', {
    baseUrl,
    hasToken: !!authToken,
  });
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
  method?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
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
    appLogger.debug('transport.api', '[ApiClient] Discovering embedded API...');
    
    const info = await invokeTauri<EmbeddedApiInfo>('get_embedded_api_info');
    
    appLogger.debug('transport.api', '[ApiClient] Embedded API discovered', {
      port: info.port,
    });
    
    return info;
  } catch (error) {
    appLogger.error('transport.api', '[ApiClient] Failed to discover embedded API', { error });
    throw error;
  }
}

/**
 * localStorage key holding the daemon's API key.
 *
 * Only relevant when the daemon is shared over the LAN (`--share-lan`), where
 * /api/* requires a bearer token. A loopback daemon needs no key and never
 * touches this — on either surface.
 */
const WEB_API_KEY_STORAGE = 'gglib_api_key';

function readStoredApiKey(): string | undefined {
  try {
    return localStorage.getItem(WEB_API_KEY_STORAGE) ?? undefined;
  } catch {
    return undefined;
  }
}

/**
 * Ask the user for the daemon's API key after a 401.
 * Returns true if a key was provided and stored (so the request can retry).
 */
function promptForApiKey(): boolean {
  const entered = window.prompt(
    'This gglib daemon requires an API key (it was printed when the daemon started).\n' +
      'Enter it to continue:'
  );
  if (!entered || !entered.trim()) {
    return false;
  }
  try {
    localStorage.setItem(WEB_API_KEY_STORAGE, entered.trim());
  } catch {
    // Storage unavailable (private mode) — the retry still works this session
    // because the client cache rebuild re-reads via the in-memory session.
  }
  apiAuthToken = entered.trim();
  return true;
}

/**
 * Cached client promise for lazy initialization.
 */
let cachedClientPromise: Promise<HttpClient> | null = null;

/**
 * Reset cached client (used on auth/network errors for retry).
 */
function resetClientCache(): void {
  appLogger.debug('transport.api', '[ApiClient] Resetting client cache for retry');
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
      
      appLogger.debug('transport.api', '[ApiClient] Request headers', {
        hasAuth: !!headers['Authorization'],
        tokenPrefix: token.substring(0, 8) + '...',
        contentType: headers['Content-Type'],
      });
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
      
      // A 401 means a LAN-shared daemon wants its key. Retrying with the same
      // credential cannot help, so only a newly entered one earns the retry —
      // the desktop app used to rebuild an identically tokenless client here
      // and fail again with nothing asked of the user.
      if (response.status === 401 && !isRetry && promptForApiKey()) {
        appLogger.warn('transport.api', '[ApiClient] 401 Unauthorized - retrying with entered API key');
        resetClientCache();
        const newClient = await getClient();
        return newClient.request<T>(path, options, true);
      }
      
      return await readData<T>(response);
    } catch (error) {
      // On network error (ECONNREFUSED, etc), clear cache and retry once
      if (!isRetry && isTauri() && error instanceof TypeError) {
        appLogger.warn('transport.api', '[ApiClient] Network error - clearing cache and retrying', { errorMessage: error.message });
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
      // A loopback daemon requires no token, which is the usual case for both
      // surfaces. The exception is a daemon started with `--share-lan`: it
      // binds a LAN interface, so it resolves or mints a key and demands it on
      // every `/api/*` call — including from a client on the same machine.
      // The desktop app reaches such a daemon whenever one is already running,
      // so it needs the same stored-key path web mode has rather than a
      // hardcoded empty token it could never recover from.
      const token = readStoredApiKey() ?? apiAuthToken;

      if (isTauri()) {
        const info = await discoverEmbeddedApi();
        const config = { baseUrl: `http://127.0.0.1:${info.port}`, token };
        // Set module-level session for SSE and other fetch-based utilities
        setApiSession(config.baseUrl, config.token);
        return buildClient(config);
      } else {
        // Web mode: same-origin.
        setApiSession('', token);
        return buildClient({ baseUrl: '', token });
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
 * Helper for PATCH requests.
 */
export async function patch<T>(path: string, body: unknown): Promise<T> {
  const client = await getClient();
  return client.request<T>(path, { method: 'PATCH', body });
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
  // `getAuthHeaders` rather than `{}`: these callers stream agent chat and the
  // five benchmark runs, and were the only `/api/*` requests carrying no
  // credential — so against a `--share-lan` daemon they answered 401 even once
  // the user had entered the key. It yields `{}` when there is no token.
  if (isTauri()) {
    const info = await discoverEmbeddedApi();
    return {
      baseUrl: `http://127.0.0.1:${info.port}`,
      headers: getAuthHeaders(),
    };
  } else {
    return {
      baseUrl: '',
      headers: getAuthHeaders(),
    };
  }
}
