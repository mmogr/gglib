/**
 * Tests for useMcpServers hook.
 * 
 * Tests MCP server CRUD operations, lifecycle management, and error handling.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { useMcpServers } from '../../../src/hooks/useMcpServers';
import type { McpServerInfo } from '../../../src/services/transport';

const transport = vi.hoisted(() => ({
  listMcpServers: vi.fn(),
  addMcpServer: vi.fn(),
  updateMcpServer: vi.fn(),
  removeMcpServer: vi.fn(),
  startMcpServer: vi.fn(),
  stopMcpServer: vi.fn(),
  callMcpTool: vi.fn(),
}));

vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));

// Mock syncAllMcpTools and syncBuiltinTools
vi.mock('../../../src/services/tools', () => ({
  syncAllMcpTools: vi.fn().mockResolvedValue(undefined),
  syncBuiltinTools: vi.fn().mockResolvedValue(undefined),
}));

const {
  listMcpServers,
  addMcpServer,
  updateMcpServer,
  removeMcpServer,
  startMcpServer,
  stopMcpServer,
} = transport;
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
    lifecycle: 'lazy' as const,
    is_valid: true,
    env: [],
    created_at: '2024-01-01T00:00:00Z',
  },
  status: 'stopped',
  tools: [],
};



describe('useMcpServers', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listMcpServers).mockResolvedValue([mockServerInfo]);
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
        lifecycle: 'lazy' as const,
        is_valid: true,
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
            lifecycle: 'lazy' as const,
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
    /**
     * `POST /api/mcp/servers/{id}/start` answers with the whole
     * `McpServerInfo` — server, status and tools — not a bare tool array.
     * This test used to mock a tool array, so it asserted against the
     * declaration rather than the wire and passed while the hook handed an
     * object onward as `McpTool[]`. Only a later `syncAllMcpTools()` doing
     * the real work kept that invisible.
     */
    it('starts server, syncs tools, and returns the tools from the server info', async () => {
      const tools = [{ name: 'tool1' }, { name: 'tool2' }];
      vi.mocked(startMcpServer).mockResolvedValue({
        ...mockServerInfo,
        status: 'running',
        tools,
      });

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
      // `/stop` answers with the server info too. The hook ignores it, but a
      // mock that resolves `undefined` against a `Promise<McpServerInfo>`
      // declaration is the same shape of fiction this commit removed.
      vi.mocked(stopMcpServer).mockResolvedValue({
        ...mockServerInfo,
        status: 'stopped',
      });

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
      vi.mocked(addMcpServer).mockResolvedValueOnce({ id: 2, name: 'New', type: 'stdio', enabled: true, lifecycle: 'lazy' as const, env: [] });

      const { result } = renderHook(() => useMcpServers());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      // First call throws
      await expect(
        act(async () => {
          await result.current.addServer({ name: 'New', server_type: 'stdio', config: {}, enabled: true, lifecycle: 'lazy' as const, env: [] });
        })
      ).rejects.toThrow('First error');

      // Second call succeeds
      await act(async () => {
        const server = await result.current.addServer({ name: 'New', server_type: 'stdio', config: {}, enabled: true, lifecycle: 'lazy' as const, env: [] });
        expect(server.id).toBe(2);
      });
    });
  });
});

// Note: useMcpTools tests are in a separate file (useMcpTools.test.ts)
// to avoid mock interference between the two hooks
