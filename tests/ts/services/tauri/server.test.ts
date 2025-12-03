/**
 * Tests for server service functions.
 * 
 * Tests llama.cpp server start/stop operations in both Tauri and Web modes.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

// Mock platform detection - must be before importing service functions
let mockIsTauriApp = true;
vi.mock('../../../../src/utils/platform', () => ({
  get isTauriApp() {
    return mockIsTauriApp;
  },
}));

// Mock apiBase
vi.mock('../../../../src/utils/apiBase', () => ({
  getApiBase: vi.fn(() => Promise.resolve('http://localhost:9887/api')),
}));

// Mock fetch globally
const mockFetch = vi.fn();
vi.stubGlobal('fetch', mockFetch);

// Import after mocks are set up
import {
  serveModel,
  stopServer,
  listServers,
} from '../../../../src/services/tauri';
import { mockInvoke } from '../../setup';

describe('Server Service', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockIsTauriApp = true;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('serveModel', () => {
    it('invokes serve_model in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce({ port: 8080, message: 'Server started' });

      const result = await serveModel({
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

      const result = await serveModel({ id: 1, context_length: 4096 });

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:9887/api/models/1/start',
        expect.objectContaining({ method: 'POST' })
      );
      expect(result).toEqual({ port: 8080, message: 'Started' });
    });

    it('handles optional parameters', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce({ port: 9000, message: 'Server started' });

      await serveModel({ id: 1 });

      expect(mockInvoke).toHaveBeenCalledWith('serve_model', {
        id: 1,
        ctxSize: undefined,
        contextLength: undefined,
        mlock: false,
        port: undefined,
        jinja: undefined,
      });
    });
  });

  describe('stopServer', () => {
    it('invokes stop_server in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Server stopped');

      const result = await stopServer(1);

      expect(mockInvoke).toHaveBeenCalledWith('stop_server', { modelId: 1 });
      expect(result).toBe('Server stopped');
    });

    it('uses POST in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: 'Stopped' }),
      });

      await stopServer(1);

      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:9887/api/models/1/stop',
        expect.objectContaining({ method: 'POST' })
      );
    });
  });

  describe('listServers', () => {
    it('invokes list_servers in Tauri mode', async () => {
      mockIsTauriApp = true;
      const servers = [
        { model_id: 1, port: 8080, status: 'running' },
        { model_id: 2, port: 8081, status: 'running' },
      ];
      mockInvoke.mockResolvedValueOnce(servers);

      const result = await listServers();

      expect(mockInvoke).toHaveBeenCalledWith('list_servers', undefined);
      expect(result).toEqual(servers);
    });

    it('uses fetch in Web mode', async () => {
      mockIsTauriApp = false;
      const servers = [{ model_id: 1, port: 8080 }];
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: servers }),
      });

      const result = await listServers();

      expect(mockFetch).toHaveBeenCalledWith('http://localhost:9887/api/servers', undefined);
      expect(result).toEqual(servers);
    });

    it('returns empty array when data is undefined', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true }),
      });

      const result = await listServers();
      expect(result).toEqual([]);
    });
  });
});
