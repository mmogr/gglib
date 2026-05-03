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
      contextLength: 4096,
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
      contextLength: 4096,
      port: MOCK_PROXY_PORT,
      mlock: false,
      jinja: true,
      reasoningFormat: undefined,
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
      contextLength: undefined,
      port: undefined,
      mlock: false, // Default value
      jinja: undefined,
      reasoningFormat: undefined,
    });
  });

  it('should handle fully-populated config', () => {
    const fullConfig: ServeConfig = {
      id: 789,
      contextLength: 8192,
      port: 9090,
      mlock: true,
      jinja: false,
      temperature: 0.8,
      topP: 0.92,
      topK: 64,
      maxTokens: 1024,
      repeatPenalty: 1.05,
      stop: ['<|im_end|>', '</s>'],
    };

    const payload = {
      id: fullConfig.id,
      request: toStartServerRequest(fullConfig),
    };

    expect(payload.request).toEqual({
      contextLength: 8192,
      port: 9090,
      mlock: true,
      jinja: false,
      reasoningFormat: undefined,
      inferenceParams: {
        temperature: 0.8,
        topP: 0.92,
        topK: 64,
        maxTokens: 1024,
        repeatPenalty: 1.05,
        stop: ['<|im_end|>', '</s>'],
      },
    });
  });

  it('should include stop-only inferenceParams when only stop is provided', () => {
    const config: ServeConfig = {
      id: 790,
      stop: ['<|im_end|>'],
    };

    const payload = {
      id: config.id,
      request: toStartServerRequest(config),
    };

    expect(payload.request).toEqual({
      contextLength: undefined,
      port: undefined,
      mlock: false,
      jinja: undefined,
      reasoningFormat: undefined,
      inferenceParams: {
        temperature: undefined,
        topP: undefined,
        topK: undefined,
        maxTokens: undefined,
        repeatPenalty: undefined,
        stop: ['<|im_end|>'],
      },
    });
  });

  it('should use camelCase field names matching Rust serde (rename_all = "camelCase")', () => {
    const config: ServeConfig = {
      id: 1,
      contextLength: 2048,
    };

    const payload = {
      id: config.id,
      request: toStartServerRequest(config),
    };

    // Verify camelCase (Rust #[serde(rename_all = "camelCase")] sends camelCase over IPC)
    expect('contextLength' in payload.request).toBe(true);
    expect('reasoningFormat' in payload.request).toBe(true);
    
    // Should NOT have snake_case variants
    expect('context_length' in payload.request).toBe(false);
    expect('reasoning_format' in payload.request).toBe(false);
  });

  it('should omit id from request object', () => {
    const config: ServeConfig = {
      id: 999,
      contextLength: 1024,
    };

    const request = toStartServerRequest(config);

    // id should NOT be in the request object
    expect('id' in request).toBe(false);
  });

  it('should not include legacy ctx_size field', () => {
    // Test that the type system prevents ctx_size from being used
    const config: ServeConfig = {
      id: 100,
      contextLength: 4096,
      // ctx_size is no longer a valid field
    };

    const request = toStartServerRequest(config);

    // ctx_size should NOT appear in request
    expect('ctx_size' in request).toBe(false);
    expect('ctxSize' in request).toBe(false);
    
    // contextLength (camelCase) should be present
    expect(request.contextLength).toBe(4096);
  });
});
