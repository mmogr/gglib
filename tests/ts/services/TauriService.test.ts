/**
 * Tests for TauriService.
 * 
 * Tests both Tauri (invoke) and Web (fetch) code paths by mocking the platform detection.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Mock platform detection - must be before importing TauriService
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
import { TauriService } from '../../../src/services/tauri';
import { mockInvoke } from '../setup';

describe('TauriService', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockIsTauriApp = true;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  // ==========================================================================
  // Model Operations
  // ==========================================================================

  describe('listModels', () => {
    const mockModels = [
      { id: 1, name: 'llama-7b', file_path: '/models/llama.gguf' },
      { id: 2, name: 'mistral-7b', file_path: '/models/mistral.gguf' },
    ];

    it('uses invoke in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(mockModels);

      const result = await TauriService.listModels();

      expect(mockInvoke).toHaveBeenCalledWith('list_models');
      expect(result).toEqual(mockModels);
    });

    it('uses fetch in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: mockModels }),
      });

      const result = await TauriService.listModels();

      expect(mockFetch).toHaveBeenCalledWith('http://localhost:9887/api/models', undefined);
      expect(result).toEqual(mockModels);
    });

    it('throws error on fetch failure in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: false,
        statusText: 'Internal Server Error',
      });

      await expect(TauriService.listModels()).rejects.toThrow('Failed to fetch models');
    });

    it('returns empty array when data is undefined in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true }),
      });

      const result = await TauriService.listModels();
      expect(result).toEqual([]);
    });
  });

  describe('getModel', () => {
    it('finds model by id in Tauri mode', async () => {
      mockIsTauriApp = true;
      const models = [
        { id: 1, name: 'llama-7b' },
        { id: 2, name: 'mistral-7b' },
      ];
      mockInvoke.mockResolvedValueOnce(models);

      const result = await TauriService.getModel(2);

      expect(result).toEqual({ id: 2, name: 'mistral-7b' });
    });

    it('throws when model not found in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce([]);

      await expect(TauriService.getModel(999)).rejects.toThrow('Model 999 not found');
    });

    it('uses REST API in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: { id: 1, name: 'llama' } }),
      });

      const result = await TauriService.getModel(1);

      expect(mockFetch).toHaveBeenCalledWith('http://localhost:9887/api/models/1', undefined);
      expect(result).toEqual({ id: 1, name: 'llama' });
    });
  });

  describe('addModel', () => {
    it('invokes add_model in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Model added successfully');

      const result = await TauriService.addModel('/path/to/model.gguf');

      expect(mockInvoke).toHaveBeenCalledWith('add_model', { filePath: '/path/to/model.gguf' });
      expect(result).toBe('Model added successfully');
    });

    it('posts to API in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: { name: 'new-model' } }),
      });

      const result = await TauriService.addModel('/path/to/model.gguf');

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:9887/api/models',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ file_path: '/path/to/model.gguf' }),
        })
      );
      expect(result).toBe('Model added: new-model');
    });
  });

  describe('removeModel', () => {
    it('invokes remove_model in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Model removed');

      await TauriService.removeModel('1', true);

      expect(mockInvoke).toHaveBeenCalledWith('remove_model', { identifier: '1', force: true });
    });

    it('defaults force to false', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Model removed');

      await TauriService.removeModel('1');

      expect(mockInvoke).toHaveBeenCalledWith('remove_model', { identifier: '1', force: false });
    });
  });

  describe('updateModel', () => {
    it('invokes update_model in Tauri mode', async () => {
      mockIsTauriApp = true;
      const updatedModel = { id: 1, name: 'updated-name' };
      mockInvoke.mockResolvedValueOnce(updatedModel);

      const result = await TauriService.updateModel(1, { name: 'updated-name' });

      expect(mockInvoke).toHaveBeenCalledWith('update_model', {
        id: 1,
        updates: { name: 'updated-name' },
      });
      expect(result).toEqual(updatedModel);
    });

    it('uses PUT in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: { id: 1, name: 'updated' } }),
      });

      await TauriService.updateModel(1, { name: 'updated' });

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:9887/api/models/1',
        expect.objectContaining({ method: 'PUT' })
      );
    });
  });

  // ==========================================================================
  // Server Operations
  // ==========================================================================

  describe('serveModel', () => {
    it('invokes serve_model in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce({ port: 8080, message: 'Server started' });

      const result = await TauriService.serveModel({
        id: 1,
        context_length: 4096,
        mlock: true,
        port: 8080,
        jinja: true,
      });

      expect(mockInvoke).toHaveBeenCalledWith('serve_model', {
        id: 1,
        ctxSize: undefined,
        contextLength: 4096,
        mlock: true,
        port: 8080,
        jinja: true,
      });
      expect(result).toEqual({ port: 8080, message: 'Server started' });
    });

    it('uses POST in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: { port: 8080, message: 'Started' } }),
      });

      const result = await TauriService.serveModel({ id: 1, context_length: 4096 });

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:9887/api/models/1/start',
        expect.objectContaining({ method: 'POST' })
      );
      expect(result).toEqual({ port: 8080, message: 'Started' });
    });
  });

  describe('stopServer', () => {
    it('invokes stop_server in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Server stopped');

      const result = await TauriService.stopServer(1);

      expect(mockInvoke).toHaveBeenCalledWith('stop_server', { modelId: 1 });
      expect(result).toBe('Server stopped');
    });

    it('uses POST in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: 'Stopped' }),
      });

      await TauriService.stopServer(1);

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:9887/api/models/1/stop',
        expect.objectContaining({ method: 'POST' })
      );
    });
  });

  describe('listServers', () => {
    it('invokes list_servers in Tauri mode', async () => {
      mockIsTauriApp = true;
      const servers = [{ model_id: 1, port: 8080 }];
      mockInvoke.mockResolvedValueOnce(servers);

      const result = await TauriService.listServers();

      expect(mockInvoke).toHaveBeenCalledWith('list_servers');
      expect(result).toEqual(servers);
    });
  });

  // ==========================================================================
  // Download Operations
  // ==========================================================================

  describe('downloadModel', () => {
    it('invokes download_model in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Download started');

      const result = await TauriService.downloadModel({
        repo_id: 'TheBloke/Llama-2-7B-GGUF',
        quantization: 'Q4_K_M',
      });

      expect(mockInvoke).toHaveBeenCalledWith('download_model', {
        modelId: 'TheBloke/Llama-2-7B-GGUF',
        quantization: 'Q4_K_M',
      });
      expect(result).toBe('Download started');
    });

    it('handles Web mode errors gracefully', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: false,
        json: () => Promise.resolve({ error: 'Model not found' }),
      });

      await expect(
        TauriService.downloadModel({ repo_id: 'unknown/model', quantization: 'Q4' })
      ).rejects.toThrow('Model not found');
    });
  });

  describe('cancelDownload', () => {
    it('invokes cancel_download in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Cancelled');

      const result = await TauriService.cancelDownload('model-id');

      expect(mockInvoke).toHaveBeenCalledWith('cancel_download', { modelId: 'model-id' });
      expect(result).toBe('Cancelled');
    });
  });

  describe('queueDownload', () => {
    it('invokes queue_download in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce({ position: 1, shard_count: 1 });

      const result = await TauriService.queueDownload('model-id', 'Q4_K_M');

      expect(mockInvoke).toHaveBeenCalledWith('queue_download', {
        modelId: 'model-id',
        quantization: 'Q4_K_M',
      });
      expect(result).toEqual({ position: 1, shard_count: 1 });
    });
  });

  describe('getDownloadQueue', () => {
    it('invokes get_download_queue in Tauri mode', async () => {
      mockIsTauriApp = true;
      const queue = { pending: [], failed: [], max_size: 10 };
      mockInvoke.mockResolvedValueOnce(queue);

      const result = await TauriService.getDownloadQueue();

      expect(mockInvoke).toHaveBeenCalledWith('get_download_queue');
      expect(result).toEqual(queue);
    });

    it('returns default queue in Web mode when data is undefined', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true }),
      });

      const result = await TauriService.getDownloadQueue();
      expect(result).toEqual({ pending: [], failed: [], max_size: 10 });
    });
  });

  // ==========================================================================
  // Proxy Operations
  // ==========================================================================

  describe('getProxyStatus', () => {
    it('invokes get_proxy_status in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce({ running: true, port: 8080 });

      const result = await TauriService.getProxyStatus();

      expect(mockInvoke).toHaveBeenCalledWith('get_proxy_status');
      expect(result).toEqual({ running: true, port: 8080 });
    });
  });

  describe('startProxy', () => {
    it('invokes start_proxy in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Proxy started');

      const result = await TauriService.startProxy({
        host: '127.0.0.1',
        port: 8080,
        start_port: 9000,
        default_context: 4096,
      });

      expect(mockInvoke).toHaveBeenCalledWith('start_proxy', {
        host: '127.0.0.1',
        port: 8080,
        startPort: 9000,
        defaultContext: 4096,
      });
      expect(result).toBe('Proxy started');
    });
  });

  describe('stopProxy', () => {
    it('invokes stop_proxy in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Proxy stopped');

      const result = await TauriService.stopProxy();

      expect(mockInvoke).toHaveBeenCalledWith('stop_proxy');
      expect(result).toBe('Proxy stopped');
    });
  });

  // ==========================================================================
  // Tag Operations
  // ==========================================================================

  describe('listTags', () => {
    it('invokes list_tags in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(['chat', 'code', 'reasoning']);

      const result = await TauriService.listTags();

      expect(mockInvoke).toHaveBeenCalledWith('list_tags');
      expect(result).toEqual(['chat', 'code', 'reasoning']);
    });
  });

  describe('addModelTag', () => {
    it('invokes add_model_tag in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Tag added');

      const result = await TauriService.addModelTag(1, 'chat');

      expect(mockInvoke).toHaveBeenCalledWith('add_model_tag', { modelId: 1, tag: 'chat' });
      expect(result).toBe('Tag added');
    });
  });

  describe('removeModelTag', () => {
    it('invokes remove_model_tag in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Tag removed');

      const result = await TauriService.removeModelTag(1, 'chat');

      expect(mockInvoke).toHaveBeenCalledWith('remove_model_tag', { modelId: 1, tag: 'chat' });
      expect(result).toBe('Tag removed');
    });
  });

  describe('getModelTags', () => {
    it('invokes get_model_tags in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(['chat', 'code']);

      const result = await TauriService.getModelTags(1);

      expect(mockInvoke).toHaveBeenCalledWith('get_model_tags', { modelId: 1 });
      expect(result).toEqual(['chat', 'code']);
    });
  });

  // ==========================================================================
  // HuggingFace Operations
  // ==========================================================================

  describe('browseHfModels', () => {
    it('invokes browse_hf_models in Tauri mode', async () => {
      mockIsTauriApp = true;
      const response = { models: [], total: 0 };
      mockInvoke.mockResolvedValueOnce(response);

      const result = await TauriService.browseHfModels({ query: 'llama' });

      expect(mockInvoke).toHaveBeenCalledWith('browse_hf_models', { request: { query: 'llama' } });
      expect(result).toEqual(response);
    });
  });

  describe('getHfQuantizations', () => {
    it('invokes get_hf_quantizations in Tauri mode', async () => {
      mockIsTauriApp = true;
      const response = { quantizations: [] };
      mockInvoke.mockResolvedValueOnce(response);

      const result = await TauriService.getHfQuantizations('TheBloke/Llama-2-7B-GGUF');

      expect(mockInvoke).toHaveBeenCalledWith('get_hf_quantizations', {
        modelId: 'TheBloke/Llama-2-7B-GGUF',
      });
      expect(result).toEqual(response);
    });

    it('encodes model ID in Web mode URL', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: { quantizations: [] } }),
      });

      await TauriService.getHfQuantizations('TheBloke/Llama-2-7B-GGUF');

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('TheBloke%2FLlama-2-7B-GGUF'),
        undefined
      );
    });
  });

  describe('getHfToolSupport', () => {
    it('invokes get_hf_tool_support in Tauri mode', async () => {
      mockIsTauriApp = true;
      const response = { supports_tools: true };
      mockInvoke.mockResolvedValueOnce(response);

      const result = await TauriService.getHfToolSupport('TheBloke/Llama-2-7B-GGUF');

      expect(mockInvoke).toHaveBeenCalledWith('get_hf_tool_support', {
        modelId: 'TheBloke/Llama-2-7B-GGUF',
      });
      expect(result).toEqual(response);
    });
  });

  // ==========================================================================
  // Utility Operations
  // ==========================================================================

  describe('openUrl', () => {
    it('invokes open_url in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(undefined);

      await TauriService.openUrl('https://example.com');

      expect(mockInvoke).toHaveBeenCalledWith('open_url', { url: 'https://example.com' });
    });

    it('uses window.open in Web mode', async () => {
      mockIsTauriApp = false;
      const mockOpen = vi.fn();
      vi.stubGlobal('open', mockOpen);

      await TauriService.openUrl('https://example.com');

      expect(mockOpen).toHaveBeenCalledWith('https://example.com', '_blank', 'noopener,noreferrer');
    });
  });

  // ==========================================================================
  // Menu Sync Operations
  // ==========================================================================

  describe('setSelectedModel', () => {
    it('invokes set_selected_model in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(undefined);

      await TauriService.setSelectedModel(1);

      expect(mockInvoke).toHaveBeenCalledWith('set_selected_model', { modelId: 1 });
    });

    it('handles null model ID', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(undefined);

      await TauriService.setSelectedModel(null);

      expect(mockInvoke).toHaveBeenCalledWith('set_selected_model', { modelId: null });
    });

    it('is no-op in Web mode', async () => {
      mockIsTauriApp = false;

      await TauriService.setSelectedModel(1);

      expect(mockInvoke).not.toHaveBeenCalled();
    });
  });

  describe('syncMenuState', () => {
    it('invokes sync_menu_state in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(undefined);

      await TauriService.syncMenuState();

      expect(mockInvoke).toHaveBeenCalledWith('sync_menu_state');
    });

    it('is no-op in Web mode', async () => {
      mockIsTauriApp = false;

      await TauriService.syncMenuState();

      expect(mockInvoke).not.toHaveBeenCalled();
    });
  });

  describe('syncMenuStateSilent', () => {
    it('swallows errors silently', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockRejectedValueOnce(new Error('Menu sync failed'));

      // Should not throw
      TauriService.syncMenuStateSilent();

      // Wait for the async operation to complete
      await new Promise((resolve) => setTimeout(resolve, 0));

      expect(mockInvoke).toHaveBeenCalledWith('sync_menu_state');
    });
  });
});
