/**
 * Test port constants for TypeScript tests.
 * 
 * Centralized port definitions to prevent hardcoded values and
 * enable easier adjustments for CI environments or dynamic allocation.
 */

/**
 * Mock proxy port used in test fixtures and API responses.
 */
export const MOCK_PROXY_PORT = 8080;

/**
 * Mock llama-server base port for test configurations.
 */
export const MOCK_BASE_PORT = 9000;

/**
 * CORS origin for test requests (e.g., mock server origin checking).
 */
export const MOCK_CORS_ORIGIN = 'http://localhost:3000';
