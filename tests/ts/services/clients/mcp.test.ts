/**
 * Tests for MCP client.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  listMcpServers,
  addMcpServer,
  updateMcpServer,
  removeMcpServer,
  startMcpServer,
  stopMcpServer,
  listMcpTools,
  callMcpTool,
  createStdioConfig,
  createSseConfig,
  isServerRunning,
  hasServerError,
  getServerErrorMessage,
} from '../../../../src/services/clients/mcp';
import type { McpServerInfo, NewMcpServer } from '../../../../src/services/clients/mcp';
import * as transport from '../../../../src/services/transport';

// Mock the transport module
vi.mock('../../../../src/services/transport', () => ({
  getTransport: vi.fn(),
}));

describe('mcp client', () => {
  const mockTransport = {
    listMcpServers: vi.fn(),
    addMcpServer: vi.fn(),
    updateMcpServer: vi.fn(),
    removeMcpServer: vi.fn(),
    startMcpServer: vi.fn(),
    stopMcpServer: vi.fn(),
    listMcpTools: vi.fn(),
    callMcpTool: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(transport.getTransport).mockReturnValue(mockTransport as any);
  });

  describe('transport wrappers', () => {
    it('listMcpServers delegates to transport', async () => {
      const mockServers: McpServerInfo[] = [
        {
          server: { id: 1, name: 'Test', server_type: 'stdio', config: {}, enabled: true, auto_start: false, env: [], created_at: '2024-01-01' },
          status: 'stopped',
          tools: [],
        },
      ];
      mockTransport.listMcpServers.mockResolvedValue(mockServers);

      const result = await listMcpServers();

      expect(transport.getTransport).toHaveBeenCalled();
      expect(mockTransport.listMcpServers).toHaveBeenCalled();
      expect(result).toBe(mockServers);
    });

    it('addMcpServer delegates to transport', async () => {
      const newServer: NewMcpServer = {
        name: 'New Server',
        server_type: 'stdio',
        config: { command: 'test' },
        enabled: true,
        auto_start: false,
        env: [],
      };
      const mockResult = { ...newServer, id: 1, created_at: '2024-01-01' };
      mockTransport.addMcpServer.mockResolvedValue(mockResult);

      const result = await addMcpServer(newServer);

      expect(mockTransport.addMcpServer).toHaveBeenCalledWith(newServer);
      expect(result).toBe(mockResult);
    });

    it('updateMcpServer delegates to transport', async () => {
      const updates = { name: 'Updated Name' };
      const mockResult = { id: 1, name: 'Updated Name', server_type: 'stdio', config: {}, enabled: true, auto_start: false, env: [], created_at: '2024-01-01' };
      mockTransport.updateMcpServer.mockResolvedValue(mockResult);

      const result = await updateMcpServer(1, updates);

      expect(mockTransport.updateMcpServer).toHaveBeenCalledWith(1, updates);
      expect(result).toBe(mockResult);
    });

    it('removeMcpServer delegates to transport', async () => {
      mockTransport.removeMcpServer.mockResolvedValue(undefined);

      await removeMcpServer(1);

      expect(mockTransport.removeMcpServer).toHaveBeenCalledWith(1);
    });

    it('startMcpServer delegates to transport', async () => {
      const mockTools = [{ name: 'tool1', description: 'A tool' }];
      mockTransport.startMcpServer.mockResolvedValue(mockTools);

      const result = await startMcpServer(1);

      expect(mockTransport.startMcpServer).toHaveBeenCalledWith(1);
      expect(result).toBe(mockTools);
    });

    it('stopMcpServer delegates to transport', async () => {
      mockTransport.stopMcpServer.mockResolvedValue(undefined);

      await stopMcpServer(1);

      expect(mockTransport.stopMcpServer).toHaveBeenCalledWith(1);
    });

    it('listMcpTools delegates to transport', async () => {
      const mockTools = [{ name: 'tool1' }, { name: 'tool2' }];
      mockTransport.listMcpTools.mockResolvedValue(mockTools);

      const result = await listMcpTools();

      expect(mockTransport.listMcpTools).toHaveBeenCalled();
      expect(result).toBe(mockTools);
    });

    it('callMcpTool delegates to transport', async () => {
      const mockResult = { success: true, data: 'result' };
      mockTransport.callMcpTool.mockResolvedValue(mockResult);

      const result = await callMcpTool(1, 'myTool', { arg1: 'value' });

      expect(mockTransport.callMcpTool).toHaveBeenCalledWith(1, 'myTool', { arg1: 'value' });
      expect(result).toBe(mockResult);
    });
  });

  describe('utility functions', () => {
    describe('createStdioConfig', () => {
      it('creates stdio config with required fields', () => {
        const config = createStdioConfig('Test', 'npx', ['-y', 'pkg']);

        expect(config).toEqual({
          name: 'Test',
          server_type: 'stdio',
          config: {
            command: 'npx',
            args: ['-y', 'pkg'],
          },
          enabled: true,
          auto_start: false,
          env: [],
        });
      });

      it('includes custom env vars', () => {
        const config = createStdioConfig('Test', 'cmd', [], [{ key: 'KEY', value: 'value' }]);

        expect(config.env).toEqual([{ key: 'KEY', value: 'value' }]);
      });
    });

    describe('createSseConfig', () => {
      it('creates SSE config with required fields', () => {
        const config = createSseConfig('SSE Test', 'http://localhost:3000');

        expect(config).toEqual({
          name: 'SSE Test',
          server_type: 'sse',
          config: {
            url: 'http://localhost:3000',
          },
          enabled: true,
          auto_start: false,
          env: [],
        });
      });
    });

    describe('isServerRunning', () => {
      it('returns true when status is running', () => {
        const info: McpServerInfo = {
          server: { id: 1, name: 'Test', server_type: 'stdio', config: {}, enabled: true, auto_start: false, env: [], created_at: '2024-01-01' },
          status: 'running',
          tools: [],
        };

        expect(isServerRunning(info)).toBe(true);
      });

      it('returns false when status is stopped', () => {
        const info: McpServerInfo = {
          server: { id: 1, name: 'Test', server_type: 'stdio', config: {}, enabled: true, auto_start: false, env: [], created_at: '2024-01-01' },
          status: 'stopped',
          tools: [],
        };

        expect(isServerRunning(info)).toBe(false);
      });
    });

    describe('hasServerError', () => {
      it('returns true when status is an error object', () => {
        const info: McpServerInfo = {
          server: { id: 1, name: 'Test', server_type: 'stdio', config: {}, enabled: true, auto_start: false, env: [], created_at: '2024-01-01' },
          status: { error: 'Connection failed' },
          tools: [],
        };

        expect(hasServerError(info)).toBe(true);
      });

      it('returns false when status is a string', () => {
        const info: McpServerInfo = {
          server: { id: 1, name: 'Test', server_type: 'stdio', config: {}, enabled: true, auto_start: false, env: [], created_at: '2024-01-01' },
          status: 'running',
          tools: [],
        };

        expect(hasServerError(info)).toBe(false);
      });
    });

    describe('getServerErrorMessage', () => {
      it('returns error message when server has error', () => {
        const info: McpServerInfo = {
          server: { id: 1, name: 'Test', server_type: 'stdio', config: {}, enabled: true, auto_start: false, env: [], created_at: '2024-01-01' },
          status: { error: 'Connection failed' },
          tools: [],
        };

        expect(getServerErrorMessage(info)).toBe('Connection failed');
      });

      it('returns null when server has no error', () => {
        const info: McpServerInfo = {
          server: { id: 1, name: 'Test', server_type: 'stdio', config: {}, enabled: true, auto_start: false, env: [], created_at: '2024-01-01' },
          status: 'running',
          tools: [],
        };

        expect(getServerErrorMessage(info)).toBeNull();
      });
    });
  });
});
