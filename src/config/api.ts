/**
 * API configuration for frontend-backend communication.
 * 
 * Provides production-safe URL generation that respects the environment:
 * - Production: Uses relative paths (same-origin)
 * - Development: Uses configurable localhost with port from environment
 * - Tauri: Not used (dynamic discovery via Tauri commands)
 */

/**
 * Get the backend port from environment or default.
 * 
 * Reads VITE_GGLIB_WEB_PORT from environment variable set in .env file.
 * Falls back to 9887 if not set.
 * 
 * Note: import.meta.env values are always strings, even for numeric-looking values.
 */
export function getBackendPort(): string {
  return import.meta.env.VITE_GGLIB_WEB_PORT || '9887';
}

/**
 * Get the API base URL appropriate for the current environment.
 * 
 * Returns:
 * - Production: Empty string (relative URLs for same-origin requests)
 * - Development: Full localhost URL with configurable port
 * 
 * This function should be used by utilities that need to construct
 * full URLs in development mode (e.g., WebSocket connections, fetch
 * outside the HTTP client).
 * 
 * The main HTTP client (src/services/transport/api/client.ts) handles
 * its own URL construction and should NOT use this function.
 * 
 * @example
 * ```typescript
 * // Development mode with VITE_GGLIB_WEB_PORT=9999
 * getApiBaseUrl() // => 'http://localhost:9999'
 * 
 * // Production mode
 * getApiBaseUrl() // => ''
 * ```
 */
export function getApiBaseUrl(): string {
  // Production builds use same-origin relative paths
  if (import.meta.env.PROD) {
    return '';
  }
  
  // Development mode: construct localhost URL with configurable port
  const port = getBackendPort();
  return `http://localhost:${port}`;
}
