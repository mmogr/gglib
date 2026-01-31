/**
 * Contract test for Tauri serve_model IPC command.
 * 
 * Validates that TypeScript payload matches Rust command signature expectations.
 * This test prevents parameter naming and nesting drift between frontend and backend.
 */

import { describe, it, expect } from 'vitest';
import type { ServeConfig } from '../../../src/types';
import { toStartServerRequest } from '../../../src/services/transport/mappers';
import { MOCK_PROXY_PORT } from '../fixtures/ports';

describe('Tauri serve_model IPC Contract', () => {
  it('should construct exact payload shape { id, request }', () => {
    const config: ServeConfig = {
      id: 123,
      context_length: 4096,
      port: MOCK_PROXY_PORT,
      mlock: false,
      jinja: true,
    };

    // Simulate what tauri.ts does
    const payload = {
      id: config.id,
      request: toStartServerRequest(config),
    };

    // Assert top-level keys are exactly { id, request }
    expect(Object.keys(payload).sort()).toEqual(['id', 'request']);
    
    // Assert id is passed separately
    expect(payload.id).toBe(123);
    
    // Assert request has correct structure (matches StartServerRequest)
    expect(payload.request).toEqual({
      context_length: 4096,
      port: MOCK_PROXY_PORT,
      mlock: false,
      jinja: true,
      reasoning_format: undefined,
    });
  });

  it('should handle minimal config (only required fields)', () => {
    const minimalConfig: ServeConfig = {
      id: 456,
      // All other fields optional
    };

    const payload = {
      id: minimalConfig.id,
      request: toStartServerRequest(minimalConfig),
    };

    expect(payload.id).toBe(456);
    expect(payload.request).toEqual({
      context_length: undefined,
      port: undefined,
      mlock: false, // Default value
      jinja: undefined,
      reasoning_format: undefined,
    });
  });

  it('should handle fully-populated config', () => {
    const fullConfig: ServeConfig = {
      id: 789,
      context_length: 8192,
      port: 9090,
      mlock: true,
      jinja: false,
    };

    const payload = {
      id: fullConfig.id,
      request: toStartServerRequest(fullConfig),
    };

    expect(payload.request).toEqual({
      context_length: 8192,
      port: 9090,
      mlock: true,
      jinja: false,
      reasoning_format: undefined,
    });
  });

  it('should use snake_case field names matching Rust serde', () => {
    const config: ServeConfig = {
      id: 1,
      context_length: 2048,
    };

    const payload = {
      id: config.id,
      request: toStartServerRequest(config),
    };

    // Verify snake_case (not camelCase)
    expect('context_length' in payload.request).toBe(true);
    expect('reasoning_format' in payload.request).toBe(true);
    
    // Should NOT have camelCase variants
    expect('contextLength' in payload.request).toBe(false);
    expect('reasoningFormat' in payload.request).toBe(false);
  });

  it('should omit id from request object', () => {
    const config: ServeConfig = {
      id: 999,
      context_length: 1024,
    };

    const request = toStartServerRequest(config);

    // id should NOT be in the request object
    expect('id' in request).toBe(false);
  });

  it('should not include legacy ctx_size field', () => {
    // Test that the type system prevents ctx_size from being used
    const config: ServeConfig = {
      id: 100,
      context_length: 4096,
      // ctx_size is no longer a valid field
    };

    const request = toStartServerRequest(config);

    // ctx_size should NOT appear in request
    expect('ctx_size' in request).toBe(false);
    expect('ctxSize' in request).toBe(false);
    
    // Only context_length should be present
    expect(request.context_length).toBe(4096);
  });
});
