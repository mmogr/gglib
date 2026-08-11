/**
 * Tests for MCP server status predicates.
 */

import { describe, it, expect } from 'vitest';
import {
  isServerRunning,
  hasServerError,
  getServerErrorMessage,
} from '../../../src/utils/mcp';
import type { McpServerInfo } from '../../../src/services/transport/types/mcp';

const serverInfo = (status: McpServerInfo['status']): McpServerInfo => ({
  server: {
    id: 1,
    name: 'Test',
    server_type: 'stdio',
    config: {},
    enabled: true,
    lifecycle: 'lazy' as const,
    is_valid: true,
    env: [],
    created_at: '2024-01-01',
  },
  status,
  tools: [],
});

describe('utils/mcp', () => {
  describe('isServerRunning', () => {
    it('returns true when status is running', () => {
      expect(isServerRunning(serverInfo('running'))).toBe(true);
    });

    it('returns false when status is stopped', () => {
      expect(isServerRunning(serverInfo('stopped'))).toBe(false);
    });
  });

  describe('hasServerError', () => {
    it('returns true when status is an error object', () => {
      expect(hasServerError(serverInfo({ error: 'Connection failed' }))).toBe(true);
    });

    it('returns false when status is a string', () => {
      expect(hasServerError(serverInfo('running'))).toBe(false);
    });
  });

  describe('getServerErrorMessage', () => {
    it('returns error message when server has error', () => {
      expect(getServerErrorMessage(serverInfo({ error: 'Connection failed' }))).toBe(
        'Connection failed'
      );
    });

    it('returns null when server has no error', () => {
      expect(getServerErrorMessage(serverInfo('running'))).toBeNull();
    });
  });
});
