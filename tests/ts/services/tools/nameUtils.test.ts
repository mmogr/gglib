import { describe, it, expect } from 'vitest';
import {
  sanitizeToolName,
  detectCollisions,
  formatToolDisplayName,
} from '../../../../src/services/tools/nameUtils';

// =============================================================================
// sanitizeToolName
// =============================================================================

describe('sanitizeToolName', () => {
  // ── Fallback ───────────────────────────────────────────────────────────────

  it('returns "unnamed_tool" for an empty string', () => {
    expect(sanitizeToolName('')).toBe('unnamed_tool');
  });

  it('returns "unnamed_tool" for a string of only special characters', () => {
    expect(sanitizeToolName('!!!')).toBe('unnamed_tool');
    expect(sanitizeToolName('...')).toBe('unnamed_tool');
    expect(sanitizeToolName('   ')).toBe('unnamed_tool');
  });

  // ── Already-valid names ────────────────────────────────────────────────────

  it('returns an already-valid name unchanged', () => {
    expect(sanitizeToolName('get_current_time')).toBe('get_current_time');
    expect(sanitizeToolName('myTool')).toBe('myTool');
    expect(sanitizeToolName('tool123')).toBe('tool123');
  });

  it('preserves hyphens (valid per OpenAI spec)', () => {
    expect(sanitizeToolName('get-weather')).toBe('get-weather');
    expect(sanitizeToolName('my-mcp-tool')).toBe('my-mcp-tool');
  });

  // ── Dot replacement ────────────────────────────────────────────────────────

  it('replaces dots with underscores', () => {
    expect(sanitizeToolName('file.read')).toBe('file_read');
    expect(sanitizeToolName('a.b.c')).toBe('a_b_c');
  });

  // ── Space replacement ──────────────────────────────────────────────────────

  it('replaces spaces with underscores', () => {
    expect(sanitizeToolName('my tool')).toBe('my_tool');
    expect(sanitizeToolName('get current time')).toBe('get_current_time');
  });

  // ── Underscore collapsing ──────────────────────────────────────────────────

  it('collapses consecutive underscores into one', () => {
    expect(sanitizeToolName('a..b')).toBe('a_b');
    expect(sanitizeToolName('a  b')).toBe('a_b');
    expect(sanitizeToolName('a___b')).toBe('a_b');
  });

  // ── Trim leading/trailing underscores ──────────────────────────────────────

  it('trims leading and trailing underscores', () => {
    expect(sanitizeToolName('_foo_')).toBe('foo');
    expect(sanitizeToolName('__bar__')).toBe('bar');
  });

  // ── Unicode ────────────────────────────────────────────────────────────────

  it('replaces unicode characters with underscores', () => {
    expect(sanitizeToolName('get\u00e9weather')).toBe('get_weather');
    expect(sanitizeToolName('\u4e2d\u6587tool')).toBe('tool');
    expect(sanitizeToolName('emoji\uD83D\uDE00name')).toBe('emoji_name');
  });

  // ── Truncation ─────────────────────────────────────────────────────────────

  it('truncates names longer than 64 characters to exactly 64', () => {
    const long = 'a'.repeat(65);
    const result = sanitizeToolName(long);
    expect(result).toHaveLength(64);
    expect(result).toBe('a'.repeat(64));
  });

  it('truncation does not create a trailing underscore', () => {
    // 63 valid chars + one invalid char that becomes '_' at position 64
    const input = 'a'.repeat(63) + '!';
    const result = sanitizeToolName(input);
    // After replacement: 'a'.repeat(63) + '_', length 64, trim won't fire
    // because we truncate after collapsing but before trimming... wait
    // actually per the implementation: replace -> collapse -> trim -> slice
    // so at slice(0,64): 'a'.repeat(63) followed by '_'
    // the trim happens BEFORE slice, so '_' at end is removed first:
    // 'a'.repeat(63) + '_' -> trim -> 'a'.repeat(63) -> length 63
    expect(result).toHaveLength(63);
    expect(result).toBe('a'.repeat(63));
  });

  it('result from a 65-char valid string is exactly 64 chars', () => {
    // All valid chars, no underscore trimming possible
    const input = 'ab'.repeat(32) + 'a'; // 65 chars
    const result = sanitizeToolName(input);
    expect(result).toHaveLength(64);
    expect(result).toBe(input.slice(0, 64));
  });

  // ── Full namespaced MCP name ───────────────────────────────────────────────

  it('sanitizes a fully-namespaced MCP name with special chars', () => {
    const result = sanitizeToolName('mcp_my.server_get data!');
    expect(result).toBe('mcp_my_server_get_data');
    expect(result).toMatch(/^[a-zA-Z0-9_-]{1,64}$/);
  });

  it('sanitizes a namespaced MCP name with hyphens and keeps them', () => {
    const result = sanitizeToolName('mcp_my-server_get-weather');
    expect(result).toBe('mcp_my-server_get-weather');
    expect(result).toMatch(/^[a-zA-Z0-9_-]{1,64}$/);
  });

  it('result always matches the OpenAI function-calling regex', () => {
    const inputs = [
      'get-weather',
      'file.read',
      'my tool name',
      'mcp_srv.1_do something!',
      '!@#$%^&*()',
      '\u00e9\u00e0\u00fc',
      'a'.repeat(100),
    ];

    for (const input of inputs) {
      const result = sanitizeToolName(input);
      expect(result).toMatch(/^[a-zA-Z0-9_-]{1,64}$/);
    }
  });
});

// =============================================================================
// detectCollisions
// =============================================================================

describe('detectCollisions', () => {
  it('returns an empty map for an empty input array', () => {
    expect(detectCollisions([])).toEqual(new Map());
  });

  it('returns an empty map when no two names collide', () => {
    const names = ['mcp_s_get_weather', 'mcp_s_list_files', 'mcp_s_read_file'];
    expect(detectCollisions(names)).toEqual(new Map());
  });

  it('detects a collision when two names sanitize to the same string', () => {
    const names = ['mcp_s_ab!', 'mcp_s_ab?'];
    const result = detectCollisions(names);
    expect(result.size).toBe(1);
    const [sanitized, originals] = [...result.entries()][0];
    expect(sanitized).toBe('mcp_s_ab');
    expect(originals).toContain('mcp_s_ab!');
    expect(originals).toContain('mcp_s_ab?');
  });

  it('detects multiple independent collisions in the same batch', () => {
    const names = [
      'mcp_s_ab!',  // collides with mcp_s_ab?
      'mcp_s_ab?',
      'mcp_s_xy.',  // collides with mcp_s_xy 
      'mcp_s_xy ',
      'mcp_s_unique',
    ];
    const result = detectCollisions(names);
    expect(result.size).toBe(2);
  });

  it('does not include non-colliding names in the result', () => {
    const names = ['mcp_s_ab!', 'mcp_s_ab?', 'mcp_s_unique'];
    const result = detectCollisions(names);
    expect(result.has('mcp_s_unique')).toBe(false);
  });

  it('detects 64-char truncation collisions', () => {
    // Two names that differ only after position 64
    const base = 'mcp_s_' + 'a'.repeat(58); // 64 chars total after sanitize
    const name1 = base + 'X';
    const name2 = base + 'Y';
    const result = detectCollisions([name1, name2]);
    expect(result.size).toBe(1);
  });
});

// =============================================================================
// formatToolDisplayName
// =============================================================================

describe('formatToolDisplayName', () => {
  it('Title Cases hyphen-separated names', () => {
    expect(formatToolDisplayName('get-weather')).toBe('Get Weather');
    expect(formatToolDisplayName('my-mcp-tool')).toBe('My Mcp Tool');
  });

  it('Title Cases underscore-separated names', () => {
    expect(formatToolDisplayName('get_current_time')).toBe('Get Current Time');
    expect(formatToolDisplayName('file_read')).toBe('File Read');
  });

  it('Title Cases dot-separated names (raw MCP names with dots)', () => {
    expect(formatToolDisplayName('file.read')).toBe('File Read');
    expect(formatToolDisplayName('server.get.data')).toBe('Server Get Data');
  });

  it('Title Cases space-separated names (raw MCP names with spaces)', () => {
    expect(formatToolDisplayName('my tool')).toBe('My Tool');
    expect(formatToolDisplayName('get current time')).toBe('Get Current Time');
  });

  it('handles mixed delimiters', () => {
    expect(formatToolDisplayName('my-tool_name.here')).toBe('My Tool Name Here');
  });

  it('collapses consecutive delimiters', () => {
    expect(formatToolDisplayName('a..b')).toBe('A B');
    expect(formatToolDisplayName('a--b')).toBe('A B');
  });

  it('handles a single word with no delimiters', () => {
    expect(formatToolDisplayName('weather')).toBe('Weather');
    expect(formatToolDisplayName('Weather')).toBe('Weather');
  });

  it('handles an already Title Cased string', () => {
    expect(formatToolDisplayName('Get Weather')).toBe('Get Weather');
  });
});
