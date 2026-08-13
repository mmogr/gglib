/**
 * Transport error handling.
 * 
 * Both TauriTransport and HttpTransport throw only TransportError.
 * This ensures consistent error handling across platforms.
 */

import { appLogger } from '../platform';

/**
 * Standardized error codes for transport operations.
 */
export type TransportErrorCode =
  | 'NOT_FOUND'      // Resource not found (404)
  | 'VALIDATION'     // Invalid input (400)
  | 'CONFLICT'       // Resource conflict (409)
  | 'UNAUTHORIZED'   // Not authorized (401/403)
  | 'NETWORK'        // Network/connection error
  | 'TIMEOUT'        // Request timeout
  | 'NOT_SUPPORTED'  // Operation not supported on this transport
  | 'INTERNAL'       // Server error (500)
  | 'DECODE'         // Response body could not be decoded as JSON
  | 'LLAMA_SERVER_NOT_INSTALLED'; // llama-server binary not found

/**
 * Metadata for llama-server not installed error.
 */
export interface LlamaServerNotInstalledMetadata {
  expectedPath: string;
  suggestedCommand: string;
  reason: string;
}

/**
 * Unified transport error.
 */
export class TransportError extends Error {
  readonly code: TransportErrorCode;
  readonly details?: unknown;

  constructor(code: TransportErrorCode, message: string, details?: unknown) {
    super(message);
    this.name = 'TransportError';
    this.code = code;
    this.details = details;

    // Maintains proper stack trace in V8 environments
    const ErrorWithCapture = Error as typeof Error & {
      captureStackTrace?: (target: object, constructor: Function) => void;
    };
    if (typeof ErrorWithCapture.captureStackTrace === 'function') {
      ErrorWithCapture.captureStackTrace(this, TransportError);
    }
  }

  /**
   * Check if an error is a TransportError.
   */
  static isTransportError(error: unknown): error is TransportError {
    return error instanceof TransportError;
  }

  /**
   * Check if error matches a specific code.
   */
  static hasCode(error: unknown, code: TransportErrorCode): boolean {
    return TransportError.isTransportError(error) && error.code === code;
  }

  /**
   * Extract llama-server metadata from error if applicable.
   */
  static getLlamaServerMetadata(error: unknown): LlamaServerNotInstalledMetadata | null {
    if (!TransportError.isTransportError(error)) return null;
    if (error.code !== 'LLAMA_SERVER_NOT_INSTALLED') return null;
    
    const details = error.details as Record<string, unknown> | undefined;
    if (!details || typeof details !== 'object') return null;
    
    return {
      expectedPath: (details.expectedPath || details.expected_path || '') as string,
      suggestedCommand: (details.suggestedCommand || details.suggested_command || 'gglib config llama install') as string,
      reason: (details.reason || 'not found') as string,
    };
  }
}

/**
 * Map HTTP status codes to TransportErrorCode.
 */
function httpStatusToCode(status: number): TransportErrorCode {
  if (status === 400) return 'VALIDATION';
  if (status === 401 || status === 403) return 'UNAUTHORIZED';
  if (status === 404) return 'NOT_FOUND';
  if (status === 409) return 'CONFLICT';
  if (status === 408) return 'TIMEOUT';
  if (status >= 500) return 'INTERNAL';
  return 'INTERNAL';
}


/**
 * Standard API response shape from our backend.
 */
export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

/**
 * Extended API response with error type and metadata.
 */
interface ErrorApiResponse {
  error: string;
  status: number;
  type?: string;
  metadata?: Record<string, unknown>;
}

/**
 * Read and unwrap response data from a fetch Response.
 * Throws TransportError on failure.
 */
export async function readData<T>(response: Response): Promise<T> {
  appLogger.debug('transport.error', '[readData] called', { status: response.status, ok: response.ok, contentType: response.headers.get('content-type') });
  
  if (!response.ok) {
    appLogger.debug('transport.error', '[readData] response not ok, extracting error');
    let errorMessage = response.statusText || `HTTP ${response.status}`;
    let errorCode = httpStatusToCode(response.status);
    let details: unknown = { status: response.status };
    
    // Try to extract structured error from response body
    try {
      const body = await response.json() as ErrorApiResponse;
      if (body.error) {
        errorMessage = body.error;
      }
      
      // Check for llama-server not installed error
      if (body.type === 'LLAMA_SERVER_NOT_INSTALLED') {
        errorCode = 'LLAMA_SERVER_NOT_INSTALLED';
        details = body.metadata || {};
      }
    } catch {
      // Ignore JSON parse errors, use status text
    }

    throw new TransportError(errorCode, errorMessage, details);
  }

  // Handle 204 No Content or empty responses for void operations
  if (response.status === 204 || response.headers.get('content-length') === '0') {
    appLogger.debug('transport.error', '[readData] empty response, returning undefined', { status: response.status });
    return undefined as T;
  }

  appLogger.debug('transport.error', '[readData] parsing response body');
  try {
    // Check if there's actually content to parse
    const text = await response.text();
    appLogger.debug('transport.error', '[readData] response text', { textPreview: text.slice(0, 200) });
    
    if (!text || text.trim() === '') {
      appLogger.debug('transport.error', '[readData] empty text body, returning undefined');
      return undefined as T;
    }
    
    const body = JSON.parse(text) as ApiResponse<T>;
    appLogger.debug('transport.error', '[readData] parsed body', { body });

    // Guard against endpoints that return JSON `null` (unit responses serialised
    // by Axum's Json(()) before 204 migration). Treat as a successful void.
    if (body === null || body === undefined) {
      return undefined as T;
    }

    // Legacy `{success, error}` envelope. `success` must actually be present:
    // non-2xx responses already returned above, so any `error` key reaching
    // here belongs to a normal result DTO. Testing `body.error` alone made
    // every DTO with an `error` field unreturnable — a 200 carrying
    // `{ok: false, error: "..."}` was thrown as a transport failure instead of
    // being handed to the caller that models failure as data.
    if (typeof body === 'object' && 'success' in body && !body.success && body.error) {
      throw new TransportError('INTERNAL', body.error);
    }

    // Return data, defaulting to the entire body if no data field
    return (body.data ?? body) as T;
  } catch (error) {
    // Body-level application errors thrown above are not parse failures.
    if (error instanceof TransportError) {
      throw error;
    }

    // A non-JSON body on an /api route means the route doesn't exist on this
    // backend — an older server falls through to the SPA fallback and returns
    // 200 text/html. Surface as NOT_FOUND so feature probes degrade the same
    // way as an explicit 404 instead of throwing a raw SyntaxError.
    const contentType = response.headers.get('content-type') ?? '';
    const code: TransportErrorCode =
      response.status === 404 || contentType.includes('text/html') ? 'NOT_FOUND' : 'DECODE';
    appLogger.warn('transport.error', '[readData] failed to decode response body', {
      status: response.status,
      contentType,
      error: error instanceof Error ? error.message : String(error),
    });
    throw new TransportError(code, `Failed to decode response body (HTTP ${response.status})`, {
      status: response.status,
      contentType,
      cause: error instanceof Error ? error.message : String(error),
    });
  }
}

