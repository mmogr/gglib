/**
 * Tests for useMcpServers hook.
 * 
 * Tests MCP server CRUD operations, lifecycle management, and error handling.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { useMcpServers } from '../../../src/hooks/useMcpServers';
import { McpServerInfo, McpServerConfig, McpTool } from '../../../src/services/mcp';

// Mock the McpService
vi.mock('../../../src/services/mcp', () => ({
  McpService: {
    listServers: vi.fn(),
    addServer: vi.fn(),
    updateServer: vi.fn(),
    removeServer: vi.fn(),
    startServer: vi.fn(),
    stopServer: vi.fn(),
    getAllToolsFlat: vi.fn(),
    callTool: vi.fn(),
  },
}));

// Mock syncAllMcpTools
vi.mock('../../../src/services/tools', () => ({
  syncAllMcpTools: vi.fn().mockResolvedValue(undefined),
}));

import { McpService } from '../../../src/services/mcp';
import { syncAllMcpTools } from '../../../src/services/tools';

// ==========================================================================
// Test Fixtures
// ==========================================================================

const mockServerConfig: McpServerConfig = {
  id: 1,
  name: 'Test Server',
  type: 'stdio',
  enabled: true,
  auto_start: false,
  command: 'npx',
  args: ['-y', 'test-server'],
  env: [],
};

const mockServerInfo: McpServerInfo = {
  config: mockServerConfig,
  status: 'stopped',
  tools: [],
};

const mockRunningServer: McpServerInfo = {
  config: { ...mockServerConfig, id: 2, name: 'Running Server' },
  status: 'running',
  tools: [
    { name: 'tool1', description: 'First tool' },
    { name: 'tool2', description: 'Second tool' },
  ],
};

const mockTools: (McpTool & { server_id: string })[] = [
  { name: 'search', description: 'Search web', server_id: '1' },
  { name: 'fetch', description: 'Fetch URL', server_id: '1' },
  { name: 'read_file', description: 'Read file', server_id: '2' },
];

describe('useMcpServers', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(McpService.listServers).mockResolvedValue([mockServerInfo]);
  });

  describe('initial state and loading', () => {
    it('starts with loading state', async () => {
      const { result } = renderHook(() => useMcpServers());

      expect(result.current.loading).toBe(true);
      expect(result.current.servers).toEqual([]);
      expect(result.current.error).toBeNull();
    });

    it('loads servers on mount', async () => {
      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      expect(result.current.servers).toEqual([mockServerInfo]);
      expect(McpService.listServers).toHaveBeenCalledTimes(1);
    });

    it('handles loading error', async () => {
      vi.mocked(McpService.listServers).mockRejectedValue(new Error('Network error'));

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      expect(result.current.error).toBe('Network error');
      expect(result.current.servers).toEqual([]);
    });

    it('uses default error message for non-Error throws', async () => {
      vi.mocked(McpService.listServers).mockRejectedValue('string error');

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.error).toBe('Failed to load MCP servers');
      });
    });
  });

  describe('refresh', () => {
    it('reloads server list', async () => {
      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      expect(McpService.listServers).toHaveBeenCalledTimes(1);

      await act(async () => {
        await result.current.refresh();
      });

      expect(McpService.listServers).toHaveBeenCalledTimes(2);
    });

    it('clears error on successful refresh', async () => {
      vi.mocked(McpService.listServers)
        .mockRejectedValueOnce(new Error('First error'))
        .mockResolvedValueOnce([mockServerInfo]);

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.error).toBe('First error');
      });

      await act(async () => {
        await result.current.refresh();
      });

      expect(result.current.error).toBeNull();
      expect(result.current.servers).toEqual([mockServerInfo]);
    });
  });

  describe('addServer', () => {
    it('adds server and refreshes list', async () => {
      const newConfig = { name: 'New', type: 'stdio' as const, enabled: true, auto_start: false, env: [] };
      const savedConfig = { id: 2, ...newConfig };
      vi.mocked(McpService.addServer).mockResolvedValue(savedConfig);

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await act(async () => {
        const added = await result.current.addServer(newConfig);
        expect(added).toEqual(savedConfig);
      });

      expect(McpService.addServer).toHaveBeenCalledWith(newConfig);
      expect(McpService.listServers).toHaveBeenCalledTimes(2);
    });

    it('throws on add failure', async () => {
      vi.mocked(McpService.addServer).mockRejectedValue(new Error('Invalid config'));

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await expect(
        act(async () => {
          await result.current.addServer({ name: 'Bad', type: 'stdio', enabled: true, auto_start: false, env: [] });
        })
      ).rejects.toThrow('Invalid config');
    });
  });

  describe('updateServer', () => {
    it('updates server and refreshes list', async () => {
      vi.mocked(McpService.updateServer).mockResolvedValue(mockServerConfig);

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await act(async () => {
        await result.current.updateServer('1', { ...mockServerConfig, name: 'Updated' });
      });

      expect(McpService.updateServer).toHaveBeenCalledWith('1', expect.objectContaining({ name: 'Updated' }));
      expect(McpService.listServers).toHaveBeenCalledTimes(2);
    });

    it('throws on update failure', async () => {
      vi.mocked(McpService.updateServer).mockRejectedValue(new Error('Update failed'));

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await expect(
        act(async () => {
          await result.current.updateServer('1', mockServerConfig);
        })
      ).rejects.toThrow('Update failed');
    });
  });

  describe('removeServer', () => {
    it('removes server and refreshes list', async () => {
      vi.mocked(McpService.removeServer).mockResolvedValue(undefined);

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await act(async () => {
        await result.current.removeServer('1');
      });

      expect(McpService.removeServer).toHaveBeenCalledWith('1');
      expect(McpService.listServers).toHaveBeenCalledTimes(2);
    });

    it('throws on remove failure', async () => {
      vi.mocked(McpService.removeServer).mockRejectedValue(new Error('Cannot remove'));

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await expect(
        act(async () => {
          await result.current.removeServer('1');
        })
      ).rejects.toThrow('Cannot remove');
    });
  });

  describe('startServer', () => {
    it('starts server, syncs tools, and returns tool list', async () => {
      const tools = [{ name: 'tool1' }, { name: 'tool2' }];
      vi.mocked(McpService.startServer).mockResolvedValue(tools);

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await act(async () => {
        const returnedTools = await result.current.startServer('1');
        expect(returnedTools).toEqual(tools);
      });

      expect(McpService.startServer).toHaveBeenCalledWith('1');
      expect(McpService.listServers).toHaveBeenCalledTimes(2);
      expect(syncAllMcpTools).toHaveBeenCalled();
    });

    it('throws on start failure', async () => {
      vi.mocked(McpService.startServer).mockRejectedValue(new Error('Command not found'));

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await expect(
        act(async () => {
          await result.current.startServer('1');
        })
      ).rejects.toThrow('Command not found');
    });
  });

  describe('stopServer', () => {
    it('stops server and syncs tools', async () => {
      vi.mocked(McpService.stopServer).mockResolvedValue(undefined);

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await act(async () => {
        await result.current.stopServer('1');
      });

      expect(McpService.stopServer).toHaveBeenCalledWith('1');
      expect(McpService.listServers).toHaveBeenCalledTimes(2);
      expect(syncAllMcpTools).toHaveBeenCalled();
    });

    it('throws on stop failure', async () => {
      vi.mocked(McpService.stopServer).mockRejectedValue(new Error('Stop failed'));

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await expect(
        act(async () => {
          await result.current.stopServer('1');
        })
      ).rejects.toThrow('Stop failed');
    });
  });

  describe('error handling', () => {
    it('throws on first failure and succeeds on second call', async () => {
      vi.mocked(McpService.addServer).mockRejectedValueOnce(new Error('First error'));
      vi.mocked(McpService.addServer).mockResolvedValueOnce({ id: 2, name: 'New', type: 'stdio', enabled: true, auto_start: false, env: [] });

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      // First call throws
      await expect(
        act(async () => {
          await result.current.addServer({ name: 'New', type: 'stdio', enabled: true, auto_start: false, env: [] });
        })
      ).rejects.toThrow('First error');

      // Second call succeeds
      await act(async () => {
        const server = await result.current.addServer({ name: 'New', type: 'stdio', enabled: true, auto_start: false, env: [] });
        expect(server.id).toBe(2);
      });
    });
  });
});

// Note: useMcpTools tests are in a separate file (useMcpTools.test.ts)
// to avoid mock interference between the two hooks
