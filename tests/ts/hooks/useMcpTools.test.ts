/**
 * Tests for useMcpTools hook.
 * 
 * This is in a separate file from useMcpServers.test.ts to avoid
 * mock interference between the two hooks when running tests.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';

// Mock the client functions BEFORE importing the hook
const mockListMcpServers = vi.fn();
const mockCallMcpTool = vi.fn();

vi.mock('../../../src/services/clients/mcp', () => ({
  listMcpServers: (...args: any[]) => mockListMcpServers(...args),
  callMcpTool: (...args: any[]) => mockCallMcpTool(...args),
}));

import { useMcpTools } from '../../../src/hooks/useMcpServers';

// Type for test data
interface MockTool {
  name: string;
  description: string;
  server_id: number;
}

// Mock server info structure matching what listMcpServers returns
const mockServerInfos = [
  {
    server: { id: 1, name: 'Server 1', config: {} },
    status: 'running',
    tools: [
      { name: 'search', description: 'Search web' },
      { name: 'fetch', description: 'Fetch URL' },
    ],
  },
  {
    server: { id: 2, name: 'Server 2', config: {} },
    status: 'running',
    tools: [
      { name: 'read_file', description: 'Read file' },
    ],
  },
];

// Expected flattened tools with server_id
const expectedTools: MockTool[] = [
  { name: 'search', description: 'Search web', server_id: 1 },
  { name: 'fetch', description: 'Fetch URL', server_id: 1 },
  { name: 'read_file', description: 'Read file', server_id: 2 },
];

describe('useMcpTools', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListMcpServers.mockResolvedValue(mockServerInfos);
  });

  describe('initial state and loading', () => {
    it('starts with loading state', () => {
      const { result } = renderHook(() => useMcpTools());
      
      expect(result.current).toBeDefined();
      expect(result.current.loading).toBe(true);
      expect(result.current.tools).toEqual([]);
    });

    it('loads tools on mount', async () => {
      const { result } = renderHook(() => useMcpTools());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      expect(result.current.tools).toEqual(expectedTools);
      expect(mockListMcpServers).toHaveBeenCalledTimes(1);
    });

    it('handles loading error', async () => {
      mockListMcpServers.mockRejectedValue(new Error('Network error'));

      const { result } = renderHook(() => useMcpTools());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      expect(result.current.error).toBe('Network error');
      expect(result.current.tools).toEqual([]);
    });

    it('uses default error message for non-Error throws', async () => {
      mockListMcpServers.mockRejectedValue('string error');

      const { result } = renderHook(() => useMcpTools());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      expect(result.current.error).toBe('Failed to load MCP tools');
    });
  });

  describe('refresh', () => {
    it('reloads tools', async () => {
      const { result } = renderHook(() => useMcpTools());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      expect(mockListMcpServers).toHaveBeenCalledTimes(1);

      await act(async () => {
        await result.current.refresh();
      });

      expect(mockListMcpServers).toHaveBeenCalledTimes(2);
    });

    it('clears error on successful refresh', async () => {
      mockListMcpServers
        .mockRejectedValueOnce(new Error('First error'))
        .mockResolvedValueOnce(mockServerInfos);

      const { result } = renderHook(() => useMcpTools());

      await waitFor(() => {
        expect(result.current.error).toBe('First error');
      });

      await act(async () => {
        await result.current.refresh();
      });

      expect(result.current.error).toBeNull();
      expect(result.current.tools).toEqual(expectedTools);
    });
  });

  describe('callTool', () => {
    it('calls callMcpTool with correct arguments', async () => {
      const toolResult = { success: true, data: 'result' };
      mockCallMcpTool.mockResolvedValue(toolResult);

      const { result } = renderHook(() => useMcpTools());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      const callResult = await result.current.callTool(1, 'search', { query: 'test' });

      expect(mockCallMcpTool).toHaveBeenCalledWith(1, 'search', { query: 'test' });
      expect(callResult).toEqual(toolResult);
    });

    it('can be called before loading completes', async () => {
      const toolResult = { success: true };
      mockCallMcpTool.mockResolvedValue(toolResult);

      const { result } = renderHook(() => useMcpTools());

      // Don't wait for loading - call immediately
      const callResult = await result.current.callTool(1, 'tool1', {});

      expect(mockCallMcpTool).toHaveBeenCalledWith(1, 'tool1', {});
      expect(callResult).toEqual(toolResult);
    });
  });
});
