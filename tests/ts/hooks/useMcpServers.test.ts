/**
 * Tests for useMcpServers hook.
 * 
 * Tests MCP server CRUD operations, lifecycle management, and error handling.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { useMcpServers } from '../../../src/hooks/useMcpServers';
import type { McpServerInfo, McpTool } from '../../../src/services/clients/mcp';

// Mock the MCP client functions
vi.mock('../../../src/services/clients/mcp', () => ({
  listMcpServers: vi.fn(),
  addMcpServer: vi.fn(),
  updateMcpServer: vi.fn(),
  removeMcpServer: vi.fn(),
  startMcpServer: vi.fn(),
  stopMcpServer: vi.fn(),
  callMcpTool: vi.fn(),
}));

// Mock syncAllMcpTools
vi.mock('../../../src/services/tools', () => ({
  syncAllMcpTools: vi.fn().mockResolvedValue(undefined),
}));

import {
  listMcpServers,
  addMcpServer,
  updateMcpServer,
  removeMcpServer,
  startMcpServer,
  stopMcpServer,
  callMcpTool,
} from '../../../src/services/clients/mcp';
import { syncAllMcpTools } from '../../../src/services/tools';

// ==========================================================================
// Test Fixtures
// ==========================================================================

const mockServerInfo: McpServerInfo = {
  server: {
    id: 1,
    name: 'Test Server',
    server_type: 'stdio',
    config: {
      command: 'npx',
      args: ['-y', 'test-server'],
    },
    enabled: true,
    auto_start: false,
    env: [],
    created_at: '2024-01-01T00:00:00Z',
  },
  status: 'stopped',
  tools: [],
};

const mockRunningServer: McpServerInfo = {
  server: {
    id: 2,
    name: 'Running Server',
    server_type: 'stdio',
    config: {
      command: 'npx',
      args: ['-y', 'running-server'],
    },
    enabled: true,
    auto_start: false,
    env: [],
    created_at: '2024-01-01T00:00:00Z',
  },
  status: 'running',
  tools: [
    { name: 'tool1', description: 'First tool' },
    { name: 'tool2', description: 'Second tool' },
  ],
};

const mockTools: (McpTool & { server_id: number })[] = [
  { name: 'search', description: 'Search web', server_id: 1 },
  { name: 'fetch', description: 'Fetch URL', server_id: 1 },
  { name: 'read_file', description: 'Read file', server_id: 2 },
];

describe('useMcpServers', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listMcpServers).mockResolvedValue([mockServerInfo]);
  });

  describe('initial state and loading', () => {
    it('starts with loading state', async () => {
      // Prevent the mount effect from resolving after the test ends.
      // This avoids React "not wrapped in act(...)" warnings for this specific test.
      vi.mocked(listMcpServers).mockImplementation(
        () => new Promise<McpServerInfo[]>(() => {})
      );

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
      expect(listMcpServers).toHaveBeenCalledTimes(1);
    });

    it('handles loading error', async () => {
      vi.mocked(listMcpServers).mockRejectedValue(new Error('Network error'));

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      expect(result.current.error).toBe('Network error');
      expect(result.current.servers).toEqual([]);
    });

    it('uses default error message for non-Error throws', async () => {
      vi.mocked(listMcpServers).mockRejectedValue('string error');

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

      expect(listMcpServers).toHaveBeenCalledTimes(1);

      await act(async () => {
        await result.current.refresh();
      });

      expect(listMcpServers).toHaveBeenCalledTimes(2);
    });

    it('clears error on successful refresh', async () => {
      vi.mocked(listMcpServers)
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
      const newServer = {
        name: 'New',
        server_type: 'stdio' as const,
        config: { command: 'test' },
        enabled: true,
        auto_start: false,
        env: [],
      };
      const savedServer = { ...newServer, id: 2, created_at: '2024-01-01T00:00:00Z' };
      vi.mocked(addMcpServer).mockResolvedValue(savedServer);

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await act(async () => {
        const added = await result.current.addServer(newServer);
        expect(added).toEqual(savedServer);
      });

      expect(addMcpServer).toHaveBeenCalledWith(newServer);
      expect(listMcpServers).toHaveBeenCalledTimes(2);
    });

    it('throws on add failure', async () => {
      vi.mocked(addMcpServer).mockRejectedValue(new Error('Invalid config'));

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await expect(
        act(async () => {
          await result.current.addServer({
            name: 'Bad',
            server_type: 'stdio',
            config: {},
            enabled: true,
            auto_start: false,
            env: [],
          });
        })
      ).rejects.toThrow('Invalid config');
    });
  });

  describe('updateServer', () => {
    it('updates server and refreshes list', async () => {
      vi.mocked(updateMcpServer).mockResolvedValue(mockServerInfo.server);

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await act(async () => {
        await result.current.updateServer(1, { name: 'Updated' });
      });

      expect(updateMcpServer).toHaveBeenCalledWith(1, { name: 'Updated' });
      expect(listMcpServers).toHaveBeenCalledTimes(2);
    });

    it('throws on update failure', async () => {
      vi.mocked(updateMcpServer).mockRejectedValue(new Error('Update failed'));

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await expect(
        act(async () => {
          await result.current.updateServer(1, { name: 'Updated' });
        })
      ).rejects.toThrow('Update failed');
    });
  });

  describe('removeServer', () => {
    it('removes server and refreshes list', async () => {
      vi.mocked(removeMcpServer).mockResolvedValue(undefined);

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await act(async () => {
        await result.current.removeServer(1);
      });

      expect(removeMcpServer).toHaveBeenCalledWith(1);
      expect(listMcpServers).toHaveBeenCalledTimes(2);
    });

    it('throws on remove failure', async () => {
      vi.mocked(removeMcpServer).mockRejectedValue(new Error('Cannot remove'));

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await expect(
        act(async () => {
          await result.current.removeServer(1);
        })
      ).rejects.toThrow('Cannot remove');
    });
  });

  describe('startServer', () => {
    it('starts server, syncs tools, and returns tool list', async () => {
      const tools = [{ name: 'tool1' }, { name: 'tool2' }];
      vi.mocked(startMcpServer).mockResolvedValue(tools);

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await act(async () => {
        const returnedTools = await result.current.startServer(1);
        expect(returnedTools).toEqual(tools);
      });

      expect(startMcpServer).toHaveBeenCalledWith(1);
      expect(listMcpServers).toHaveBeenCalledTimes(2);
      expect(syncAllMcpTools).toHaveBeenCalled();
    });

    it('throws on start failure', async () => {
      vi.mocked(startMcpServer).mockRejectedValue(new Error('Command not found'));

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await expect(
        act(async () => {
          await result.current.startServer(1);
        })
      ).rejects.toThrow('Command not found');
    });
  });

  describe('stopServer', () => {
    it('stops server and syncs tools', async () => {
      vi.mocked(stopMcpServer).mockResolvedValue(undefined);

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await act(async () => {
        await result.current.stopServer(1);
      });

      expect(stopMcpServer).toHaveBeenCalledWith(1);
      expect(listMcpServers).toHaveBeenCalledTimes(2);
      expect(syncAllMcpTools).toHaveBeenCalled();
    });

    it('throws on stop failure', async () => {
      vi.mocked(stopMcpServer).mockRejectedValue(new Error('Stop failed'));

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      await expect(
        act(async () => {
          await result.current.stopServer(1);
        })
      ).rejects.toThrow('Stop failed');
    });
  });

  describe('error handling', () => {
    it('throws on first failure and succeeds on second call', async () => {
      vi.mocked(addMcpServer).mockRejectedValueOnce(new Error('First error'));
      vi.mocked(addMcpServer).mockResolvedValueOnce({ id: 2, name: 'New', type: 'stdio', enabled: true, auto_start: false, env: [] });

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      // First call throws
      let firstError: unknown;
      await act(async () => {
        try {
          await result.current.addServer({ name: 'New', type: 'stdio', enabled: true, auto_start: false, env: [] });
        } catch (error) {
          firstError = error;
        }
      });
      expect(firstError).toBeInstanceOf(Error);
      expect((firstError as Error).message).toBe('First error');

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
