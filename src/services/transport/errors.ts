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
  | 'LLAMA_SERVER_NOT_INSTALLED'; // llama-server binary not found

/**
 * Metadata for llama-server not installed error.
 */
export interface LlamaServerNotInstalledMetadata {
  expectedPath: string;
  legacyPath?: string;
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
      legacyPath: (details.legacyPath || details.legacy_path) as string | undefined,
      suggestedCommand: (details.suggestedCommand || details.suggested_command || 'gglib llama install') as string,
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
 * Map a raw error to TransportError.
 */
export function mapError(error: unknown): TransportError {
  // Already a TransportError
  if (TransportError.isTransportError(error)) {
    return error;
  }

  // Fetch Response error (shouldn't happen, but handle it)
  if (error instanceof Response) {
    return new TransportError(
      httpStatusToCode(error.status),
      error.statusText || `HTTP ${error.status}`,
      { status: error.status }
    );
  }

  // Standard Error
  if (error instanceof Error) {
    // Network errors
    if (error.name === 'TypeError' && error.message.includes('fetch')) {
      return new TransportError('NETWORK', 'Network request failed', error);
    }
    if (error.name === 'AbortError') {
      return new TransportError('TIMEOUT', 'Request was aborted', error);
    }

    // Tauri invoke errors come as Error with message - try to parse JSON
    try {
      const parsed = JSON.parse(error.message);
      if (parsed && typeof parsed === 'object' && parsed.type === 'LLAMA_SERVER_NOT_INSTALLED') {
        return new TransportError('LLAMA_SERVER_NOT_INSTALLED', parsed.message || 'llama-server not installed', parsed.metadata);
      }
    } catch {
      // Not JSON, fall through
    }

    return new TransportError('INTERNAL', error.message, error);
  }

  // String error - try to parse as JSON (Tauri structured error)
  if (typeof error === 'string') {
    try {
      const parsed = JSON.parse(error);
      if (parsed && typeof parsed === 'object' && parsed.type === 'LLAMA_SERVER_NOT_INSTALLED') {
        return new TransportError('LLAMA_SERVER_NOT_INSTALLED', parsed.message || 'llama-server not installed', parsed.metadata);
      }
    } catch {
      // Not JSON
    }
    return new TransportError('INTERNAL', error);
  }

  // Object with type field (structured error)
  if (error && typeof error === 'object' && 'type' in error) {
    const typed = error as { type: string; message?: string; metadata?: unknown };
    if (typed.type === 'LLAMA_SERVER_NOT_INSTALLED') {
      return new TransportError('LLAMA_SERVER_NOT_INSTALLED', typed.message || 'llama-server not installed', typed.metadata);
    }
  }

  // Unknown error
  return new TransportError('INTERNAL', 'An unknown error occurred', error);
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

    if (!body.success && body.error) {
      throw new TransportError('INTERNAL', body.error);
    }

    // Return data, defaulting to the entire body if no data field
    return (body.data ?? body) as T;
  } catch (error) {
    appLogger.error('transport.error', '[readData] JSON parse error', { error });
    throw error;
  }
}

/**
 * Wrap a Tauri invoke call with error mapping.
 */
export async function wrapInvoke<T>(
  invokePromise: Promise<T>
): Promise<T> {
  try {
    return await invokePromise;
  } catch (error) {
    throw mapError(error);
  }
}
