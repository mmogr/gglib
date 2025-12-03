/**
 * Tests for McpService.
 * 
 * Tests both Tauri (invoke) and Web (fetch) code paths by mocking the platform detection.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Mock platform detection - must be before importing McpService
let mockIsTauriApp = true;
vi.mock('../../../src/utils/platform', () => ({
  get isTauriApp() {
    return mockIsTauriApp;
  },
}));

// Mock apiBase - return the value directly since the service awaits it
vi.mock('../../../src/utils/apiBase', () => ({
  getApiBase: vi.fn(() => Promise.resolve('http://localhost:9887/api')),
}));

// Mock fetch globally
const mockFetch = vi.fn();
vi.stubGlobal('fetch', mockFetch);

// Import after mocks are set up
import { McpService, McpServerConfig, McpServerInfo, McpTool } from '../../../src/services/mcp';
import { mockInvoke } from '../setup';

// ==========================================================================
// Test Fixtures
// ==========================================================================

const mockStdioConfig: Omit<McpServerConfig, 'id'> = {
  name: 'Test Server',
  type: 'stdio',
  enabled: true,
  auto_start: false,
  command: 'npx',
  args: ['-y', '@modelcontextprotocol/server-test'],
  env: [['API_KEY', 'test-key']],
};

const mockSseConfig: Omit<McpServerConfig, 'id'> = {
  name: 'SSE Server',
  type: 'sse',
  enabled: true,
  auto_start: false,
  url: 'http://localhost:3000/sse',
  env: [],
};

const mockServerInfo: McpServerInfo = {
  config: { id: 1, ...mockStdioConfig },
  status: 'running',
  tools: [
    { name: 'read_file', description: 'Read a file' },
    { name: 'write_file', description: 'Write a file' },
  ],
};

const mockTools: McpTool[] = [
  { name: 'search', description: 'Search the web' },
  { name: 'fetch', description: 'Fetch a URL' },
];

describe('McpService', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockIsTauriApp = true;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  // ==========================================================================
  // Configuration CRUD
  // ==========================================================================

  describe('addServer', () => {
    it('invokes add_mcp_server in Tauri mode', async () => {
      mockIsTauriApp = true;
      const savedConfig = { id: 1, ...mockStdioConfig };
      mockInvoke.mockResolvedValueOnce(savedConfig);

      const result = await McpService.addServer(mockStdioConfig);

      expect(mockInvoke).toHaveBeenCalledWith('add_mcp_server', { config: mockStdioConfig });
      expect(result).toEqual(savedConfig);
    });

    it('posts to API in Web mode', async () => {
      mockIsTauriApp = false;
      const savedConfig = { id: 1, ...mockStdioConfig };
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: savedConfig }),
      });

      const result = await McpService.addServer(mockStdioConfig);

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:9887/api/mcp/servers',
        expect.objectContaining({
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(mockStdioConfig),
        })
      );
      expect(result).toEqual(savedConfig);
    });

    it('throws error when Web API fails', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: false,
        json: () => Promise.resolve({ error: 'Invalid configuration' }),
      });

      await expect(McpService.addServer(mockStdioConfig)).rejects.toThrow('Invalid configuration');
    });

    it('throws when no data returned in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true }),
      });

      await expect(McpService.addServer(mockStdioConfig)).rejects.toThrow(
        'No data returned from server'
      );
    });
  });

  describe('listServers', () => {
    it('invokes list_mcp_servers in Tauri mode', async () => {
      mockIsTauriApp = true;
      const servers = [mockServerInfo];
      mockInvoke.mockResolvedValueOnce(servers);

      const result = await McpService.listServers();

      expect(mockInvoke).toHaveBeenCalledWith('list_mcp_servers');
      expect(result).toEqual(servers);
    });

    it('fetches from API in Web mode', async () => {
      mockIsTauriApp = false;
      const servers = [mockServerInfo];
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: servers }),
      });

      const result = await McpService.listServers();

      expect(mockFetch).toHaveBeenCalledWith('http://localhost:9887/api/mcp/servers', undefined);
      expect(result).toEqual(servers);
    });

    it('returns empty array when data is undefined', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true }),
      });

      const result = await McpService.listServers();
      expect(result).toEqual([]);
    });
  });

  describe('updateServer', () => {
    it('invokes update_mcp_server in Tauri mode', async () => {
      mockIsTauriApp = true;
      const updatedConfig = { id: 1, ...mockStdioConfig, name: 'Updated Server' };
      mockInvoke.mockResolvedValueOnce(updatedConfig);

      const result = await McpService.updateServer('1', updatedConfig);

      expect(mockInvoke).toHaveBeenCalledWith('update_mcp_server', {
        id: '1',
        config: updatedConfig,
      });
      expect(result).toEqual(updatedConfig);
    });

    it('uses PUT in Web mode', async () => {
      mockIsTauriApp = false;
      const updatedConfig = { id: 1, ...mockStdioConfig, name: 'Updated' };
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: updatedConfig }),
      });

      await McpService.updateServer('1', updatedConfig);

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:9887/api/mcp/servers/1',
        expect.objectContaining({ method: 'PUT' })
      );
    });
  });

  describe('removeServer', () => {
    it('invokes remove_mcp_server in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(undefined);

      await McpService.removeServer('1');

      expect(mockInvoke).toHaveBeenCalledWith('remove_mcp_server', { id: '1' });
    });

    it('uses DELETE in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({ ok: true, json: () => Promise.resolve({ success: true }) });

      await McpService.removeServer('1');

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:9887/api/mcp/servers/1',
        expect.objectContaining({ method: 'DELETE' })
      );
    });
  });

  // ==========================================================================
  // Server Lifecycle
  // ==========================================================================

  describe('startServer', () => {
    it('invokes start_mcp_server and returns tools in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(mockTools);

      const result = await McpService.startServer('1');

      expect(mockInvoke).toHaveBeenCalledWith('start_mcp_server', { id: '1' });
      expect(result).toEqual(mockTools);
    });

    it('posts to start endpoint in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: mockTools }),
      });

      const result = await McpService.startServer('1');

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:9887/api/mcp/servers/1/start',
        expect.objectContaining({ method: 'POST' })
      );
      expect(result).toEqual(mockTools);
    });

    it('returns empty array when no tools returned', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true }),
      });

      const result = await McpService.startServer('1');
      expect(result).toEqual([]);
    });

    it('throws error on failure', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: false,
        json: () => Promise.resolve({ error: 'Command not found' }),
      });

      await expect(McpService.startServer('1')).rejects.toThrow('Command not found');
    });
  });

  describe('stopServer', () => {
    it('invokes stop_mcp_server in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(undefined);

      await McpService.stopServer('1');

      expect(mockInvoke).toHaveBeenCalledWith('stop_mcp_server', { id: '1' });
    });

    it('posts to stop endpoint in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({ ok: true, json: () => Promise.resolve({ success: true }) });

      await McpService.stopServer('1');

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:9887/api/mcp/servers/1/stop',
        expect.objectContaining({ method: 'POST' })
      );
    });
  });

  // ==========================================================================
  // Tool Operations
  // ==========================================================================

  describe('listAllTools', () => {
    it('returns Map of tools by server in Tauri mode', async () => {
      mockIsTauriApp = true;
      const toolsData: [string, McpTool[]][] = [
        ['server1', [{ name: 'tool1' }]],
        ['server2', [{ name: 'tool2' }, { name: 'tool3' }]],
      ];
      mockInvoke.mockResolvedValueOnce(toolsData);

      const result = await McpService.listAllTools();

      expect(mockInvoke).toHaveBeenCalledWith('list_mcp_tools');
      expect(result).toBeInstanceOf(Map);
      expect(result.get('server1')).toEqual([{ name: 'tool1' }]);
      expect(result.get('server2')?.length).toBe(2);
    });

    it('fetches from API in Web mode', async () => {
      mockIsTauriApp = false;
      const toolsData = [
        ['server1', [{ name: 'tool1' }]],
      ];
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: toolsData }),
      });

      const result = await McpService.listAllTools();

      expect(mockFetch).toHaveBeenCalledWith('http://localhost:9887/api/mcp/tools', undefined);
      expect(result.get('server1')).toEqual([{ name: 'tool1' }]);
    });
  });

  describe('getAllToolsFlat', () => {
    it('returns flat array with server_id attached', async () => {
      mockIsTauriApp = true;
      const toolsData: [string, McpTool[]][] = [
        ['server1', [{ name: 'tool1', description: 'Tool 1' }]],
        ['server2', [{ name: 'tool2' }]],
      ];
      mockInvoke.mockResolvedValueOnce(toolsData);

      const result = await McpService.getAllToolsFlat();

      expect(result).toEqual([
        { name: 'tool1', description: 'Tool 1', server_id: 'server1' },
        { name: 'tool2', server_id: 'server2' },
      ]);
    });
  });

  describe('callTool', () => {
    it('invokes call_mcp_tool in Tauri mode', async () => {
      mockIsTauriApp = true;
      const toolResult = { success: true, data: { content: 'result' } };
      mockInvoke.mockResolvedValueOnce(toolResult);

      const result = await McpService.callTool('server1', 'read_file', { path: '/test.txt' });

      expect(mockInvoke).toHaveBeenCalledWith('call_mcp_tool', {
        serverId: 'server1',
        toolName: 'read_file',
        arguments: { path: '/test.txt' },
      });
      expect(result).toEqual(toolResult);
    });

    it('posts to tool endpoint in Web mode', async () => {
      mockIsTauriApp = false;
      const toolResult = { success: true, data: { output: 'done' } };
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: toolResult }),
      });

      const result = await McpService.callTool('server1', 'write_file', { content: 'hello' });

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:9887/api/mcp/servers/server1/tools/write_file',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ arguments: { content: 'hello' } }),
        })
      );
      expect(result).toEqual(toolResult);
    });
  });

  // ==========================================================================
  // Utility Methods
  // ==========================================================================

  describe('createStdioConfig', () => {
    it('creates stdio config with required fields', () => {
      const config = McpService.createStdioConfig('Test', 'npx', ['-y', 'test']);

      expect(config).toEqual({
        name: 'Test',
        type: 'stdio',
        enabled: true,
        auto_start: false,
        command: 'npx',
        args: ['-y', 'test'],
        env: [],
      });
    });

    it('includes custom env vars', () => {
      const config = McpService.createStdioConfig('Test', 'cmd', [], [['KEY', 'value']]);

      expect(config.env).toEqual([['KEY', 'value']]);
    });
  });

  describe('createSseConfig', () => {
    it('creates SSE config with required fields', () => {
      const config = McpService.createSseConfig('SSE Test', 'http://localhost:3000/sse');

      expect(config).toEqual({
        name: 'SSE Test',
        type: 'sse',
        enabled: true,
        auto_start: false,
        url: 'http://localhost:3000/sse',
        env: [],
      });
    });
  });

  describe('isRunning', () => {
    it('returns true for running status', () => {
      const info: McpServerInfo = { ...mockServerInfo, status: 'running' };
      expect(McpService.isRunning(info)).toBe(true);
    });

    it('returns false for stopped status', () => {
      const info: McpServerInfo = { ...mockServerInfo, status: 'stopped' };
      expect(McpService.isRunning(info)).toBe(false);
    });

    it('returns false for starting status', () => {
      const info: McpServerInfo = { ...mockServerInfo, status: 'starting' };
      expect(McpService.isRunning(info)).toBe(false);
    });

    it('returns false for error status', () => {
      const info: McpServerInfo = { ...mockServerInfo, status: { error: 'Failed' } };
      expect(McpService.isRunning(info)).toBe(false);
    });
  });

  describe('hasError', () => {
    it('returns true for error status', () => {
      const info: McpServerInfo = { ...mockServerInfo, status: { error: 'Connection failed' } };
      expect(McpService.hasError(info)).toBe(true);
    });

    it('returns false for non-error status', () => {
      expect(McpService.hasError({ ...mockServerInfo, status: 'running' })).toBe(false);
      expect(McpService.hasError({ ...mockServerInfo, status: 'stopped' })).toBe(false);
      expect(McpService.hasError({ ...mockServerInfo, status: 'starting' })).toBe(false);
    });
  });

  describe('getErrorMessage', () => {
    it('returns error message for error status', () => {
      const info: McpServerInfo = { ...mockServerInfo, status: { error: 'Connection refused' } };
      expect(McpService.getErrorMessage(info)).toBe('Connection refused');
    });

    it('returns null for non-error status', () => {
      expect(McpService.getErrorMessage({ ...mockServerInfo, status: 'running' })).toBeNull();
      expect(McpService.getErrorMessage({ ...mockServerInfo, status: 'stopped' })).toBeNull();
    });
  });
});
