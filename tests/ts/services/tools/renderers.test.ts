import { describe, it, expect } from 'vitest';
import { fallbackRenderer } from '../../../../src/services/tools/renderers/FallbackRenderer';
import { timeRenderer } from '../../../../src/services/tools/renderers/TimeRenderer';
import { ToolRegistry } from '../../../../src/services/tools/registry';
import type { ToolResultRenderer } from '../../../../src/services/tools/types';

// ---------------------------------------------------------------------------
// fallbackRenderer
// ---------------------------------------------------------------------------

describe('fallbackRenderer.renderSummary', () => {
  it('returns a string for a primitive string value', () => {
    const result = fallbackRenderer.renderSummary!('hello', 'tool');
    expect(typeof result).toBe('string');
    expect(result).toBe('"hello"');
  });

  it('returns a string for a number', () => {
    const result = fallbackRenderer.renderSummary!(42, 'tool');
    expect(result).toBe('42');
  });

  it('returns a JSON string for a plain object', () => {
    const result = fallbackRenderer.renderSummary!({ key: 'value' }, 'tool');
    expect(typeof result).toBe('string');
    expect(result).toContain('key');
  });

  it('returns a JSON string for an array', () => {
    const result = fallbackRenderer.renderSummary!([1, 2, 3], 'tool');
    expect(typeof result).toBe('string');
    expect(result).toContain('1');
  });

  it('returns a string for null', () => {
    const result = fallbackRenderer.renderSummary!(null, 'tool');
    expect(result).toBe('null');
  });

  it('returns a safe fallback string for circular references without throwing', () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;

    // JSON.stringify throws on circular references; renderSummary must not.
    expect(() => {
      const result = fallbackRenderer.renderSummary!(circular, 'tool');
      expect(typeof result).toBe('string');
      expect(result.length).toBeGreaterThan(0);
    }).not.toThrow();

    const result = fallbackRenderer.renderSummary!(circular, 'tool');
    expect(result).toBe('(result)');
  });

  it('truncates very long JSON to at most 80 characters plus an ellipsis', () => {
    const big = { data: 'x'.repeat(200) };
    const result = fallbackRenderer.renderSummary!(big, 'tool');
    // The raw JSON is far longer than 80 chars; summary must be truncated.
    expect(result.length).toBeLessThanOrEqual(81); // 80 chars + 1 ellipsis char
  });
});

// ---------------------------------------------------------------------------
// timeRenderer
// ---------------------------------------------------------------------------

describe('timeRenderer.renderSummary', () => {
  it('returns the time string for a valid time result shape', () => {
    const data = { time: 'Sunday, 10:00 AM', timezone: 'UTC', format: 'human' };
    const result = timeRenderer.renderSummary!(data, 'get_current_time');
    expect(result).toBe('Sunday, 10:00 AM');
  });

  it('returns a string representation of a UNIX timestamp', () => {
    const data = { time: 1672531200, timezone: 'UTC', format: 'unix' };
    const result = timeRenderer.renderSummary!(data, 'get_current_time');
    expect(result).toBe('1672531200');
  });

  it('returns "(unknown)" for an empty object without throwing', () => {
    expect(() => {
      const result = timeRenderer.renderSummary!({}, 'get_current_time');
      expect(result).toBe('(unknown)');
    }).not.toThrow();
  });

  it('returns "(unknown)" for null without throwing', () => {
    expect(() => {
      const result = timeRenderer.renderSummary!(null, 'get_current_time');
      expect(result).toBe('(unknown)');
    }).not.toThrow();
  });
});

// ---------------------------------------------------------------------------
// ToolRegistry.getRenderer
// ---------------------------------------------------------------------------

describe('ToolRegistry.getRenderer', () => {
  const mockRenderer: ToolResultRenderer = {
    renderResult: (data) => String(data),
    renderSummary: (data) => String(data),
  };

  it('returns the registered renderer for a tool registered with one', () => {
    const registry = new ToolRegistry();
    registry.registerFunction(
      'dummy',
      'A dummy tool',
      undefined,
      () => ({ success: true, data: 'ok' }),
      'builtin',
      mockRenderer,
    );
    expect(registry.getRenderer('dummy')).toBe(mockRenderer);
  });

  it('returns undefined for a tool registered without a renderer', () => {
    const registry = new ToolRegistry();
    registry.registerFunction(
      'no_renderer',
      'A tool without a renderer',
      undefined,
      () => ({ success: true, data: 'ok' }),
    );
    expect(registry.getRenderer('no_renderer')).toBeUndefined();
  });

  it('returns undefined for an unknown tool name', () => {
    const registry = new ToolRegistry();
    expect(registry.getRenderer('unknown_tool')).toBeUndefined();
  });
});
