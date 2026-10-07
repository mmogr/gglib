/**
 * HTTP client for API transport.
 * 
 * Handles embedded API discovery in Tauri mode, bearer token authentication,
 * and recoverable error handling with single retry on 401/network errors.
 *
 * Two doors reach the daemon: `request`, behind `get`/`post`/`put`/`patch`/
 * `del`, for a JSON call, and `apiFetch` for what that cannot carry.
 */

import { readData } from '../errors';
import { appLogger } from '../../platform';
import { isDesktop } from '../../platform/detect';
import { isDaemonTokenRefusal, serviceRestarted, takeDaemonToken } from './daemonToken';

/**
 * Module-level API session context: where the daemon is and the token that
 * opens it. Empty until `getClient` has resolved, so nothing outside this
 * module reads it.
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
 * Embedded API info discovered from Tauri.
 */
interface EmbeddedApiInfo {
  port: number;
  token?: string | null; // the daemon's token, when the desktop can read it
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
 * Discover embedded API info from Tauri.
 * Throws on failure and clears cache to allow retry.
 */
async function discoverEmbeddedApi(): Promise<EmbeddedApiInfo> {
  try {
    appLogger.debug('transport.api', '[ApiClient] Discovering embedded API...');
    
    // Only reached on the desktop, where the bridge is always there.
    const info: EmbeddedApiInfo = await window.__TAURI_INTERNALS__!.invoke('get_embedded_api_info');
    
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
 * /api/* takes its key. A loopback daemon takes only its own token
 * (daemonToken.ts) and never asks for this — on either surface.
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
export function resetClientCache(): void {
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
      
      // A 401 means a LAN-shared daemon wants its key, unless it is a daemon
      // asking for its token, which no key opens: its body says how to get it.
      // Only a newly entered key earns the retry — the desktop app used to
      // rebuild an identically tokenless client here and fail again.
      const refused = await isDaemonTokenRefusal(response);
      if (refused) resetClientCache(); // the next call rereads a token minted since
      if (refused && isDesktop()) { // the desktop rereads the file: retry once, never the sentence
        if (isRetry) throw serviceRestarted();
        return (await getClient()).request<T>(path, options, true);
      }
      if (response.status === 401 && !isRetry && !refused && promptForApiKey()) {
        appLogger.warn('transport.api', '[ApiClient] 401 Unauthorized - retrying with entered API key');
        resetClientCache();
        const newClient = await getClient();
        return newClient.request<T>(path, options, true);
      }
      
      return await readData<T>(response);
    } catch (error) {
      // On network error (ECONNREFUSED, etc), clear cache and retry once
      if (!isRetry && isDesktop() && error instanceof TypeError) {
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
      // Every `/api/*` call takes the daemon's token: the desktop app reads it
      // from the daemon's file, a page from the link `gglib web` prints. A
      // daemon started with `--share-lan` also takes its key, which a page on
      // another machine can only be given by hand, so the stored-key path
      // stays for it, on both surfaces.
      const token = takeDaemonToken() ?? readStoredApiKey() ?? apiAuthToken;

      if (isDesktop()) {
        const info = await discoverEmbeddedApi();
        const config = { baseUrl: `http://127.0.0.1:${info.port}`, token: info.token ?? token };
        // Set the module-level session `apiFetch` reads
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
export async function del<T>(path: string, body?: unknown): Promise<T> {
  const client = await getClient();
  return client.request<T>(path, { method: 'DELETE', body: body === undefined ? null : body });
}

/**
 * `fetch` against this session's daemon, for what `request` cannot carry: a
 * stream, a body that is not JSON, a reply that is not JSON.
 *
 * `path` is joined to the daemon's base URL and the session's credential is
 * added, over any `Authorization` the caller gave. Both are the client's,
 * once it has resolved them: the desktop app asks Tauri where the daemon is
 * once, not per call, and a call made before any other still gets the token.
 * A refusal throws the `TransportError` `readData` builds from it, so the
 * response returned is one the daemon accepted. Nothing is retried: a caller
 * that reconnects renews the credential itself (`renew.ts`).
 */
export async function apiFetch(
  path: string,
  init: Omit<RequestInit, 'headers'> & { headers?: Record<string, string> } = {},
): Promise<Response> {
  await getClient();
  const credential: Record<string, string> = apiAuthToken ? { Authorization: `Bearer ${apiAuthToken}` } : {};
  const response = await fetch(`${apiBaseUrl}${path}`, {
    ...init,
    headers: { ...init.headers, ...credential },
  });
  if (!response.ok) await readData(response);
  return response;
}
