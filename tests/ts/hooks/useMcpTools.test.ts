/**
 * Tests for useMcpTools hook.
 * 
 * This is in a separate file from useMcpServers.test.ts to avoid
 * mock interference between the two hooks when running tests.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';

// Mock the McpService BEFORE importing the hook
vi.mock('../../../src/services/mcp', () => ({
  McpService: {
    getAllToolsFlat: vi.fn(),
    callTool: vi.fn(),
  },
}));

import { useMcpTools } from '../../../src/hooks/useMcpServers';
import { McpService } from '../../../src/services/mcp';

// Type for test data
interface MockTool {
  name: string;
  description: string;
  server_id: string;
}

const mockTools: MockTool[] = [
  { name: 'search', description: 'Search web', server_id: '1' },
  { name: 'fetch', description: 'Fetch URL', server_id: '1' },
  { name: 'read_file', description: 'Read file', server_id: '2' },
];

describe('useMcpTools', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(McpService.getAllToolsFlat).mockResolvedValue(mockTools);
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

      expect(result.current.tools).toEqual(mockTools);
      expect(McpService.getAllToolsFlat).toHaveBeenCalledTimes(1);
    });

    it('handles loading error', async () => {
      vi.mocked(McpService.getAllToolsFlat).mockRejectedValue(new Error('Network error'));

      const { result } = renderHook(() => useMcpTools());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      expect(result.current.error).toBe('Network error');
      expect(result.current.tools).toEqual([]);
    });

    it('uses default error message for non-Error throws', async () => {
      vi.mocked(McpService.getAllToolsFlat).mockRejectedValue('string error');

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

      expect(McpService.getAllToolsFlat).toHaveBeenCalledTimes(1);

      await act(async () => {
        await result.current.refresh();
      });

      expect(McpService.getAllToolsFlat).toHaveBeenCalledTimes(2);
    });

    it('clears error on successful refresh', async () => {
      vi.mocked(McpService.getAllToolsFlat)
        .mockRejectedValueOnce(new Error('First error'))
        .mockResolvedValueOnce(mockTools);

      const { result } = renderHook(() => useMcpTools());

      await waitFor(() => {
        expect(result.current.error).toBe('First error');
      });

      await act(async () => {
        await result.current.refresh();
      });

      expect(result.current.error).toBeNull();
      expect(result.current.tools).toEqual(mockTools);
    });
  });

  describe('callTool', () => {
    it('calls McpService.callTool with correct arguments', async () => {
      const toolResult = { success: true, data: 'result' };
      vi.mocked(McpService.callTool).mockResolvedValue(toolResult);

      const { result } = renderHook(() => useMcpTools());

      await waitFor(() => {
        expect(result.current.loading).toBe(false);
      });

      const callResult = await result.current.callTool('server1', 'search', { query: 'test' });

      expect(McpService.callTool).toHaveBeenCalledWith('server1', 'search', { query: 'test' });
      expect(callResult).toEqual(toolResult);
    });

    it('can be called before loading completes', async () => {
      const toolResult = { success: true };
      vi.mocked(McpService.callTool).mockResolvedValue(toolResult);

      const { result } = renderHook(() => useMcpTools());

      // Don't wait for loading - call immediately
      const callResult = await result.current.callTool('server1', 'tool1', {});

      expect(McpService.callTool).toHaveBeenCalledWith('server1', 'tool1', {});
      expect(callResult).toEqual(toolResult);
    });
  });
});
