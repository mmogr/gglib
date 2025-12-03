/**
 * Tests for proxy service functions.
 * 
 * Tests multi-model proxy management in both Tauri and Web modes.
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
  getProxyStatus,
  startProxy,
  stopProxy,
} from '../../../../src/services/tauri';
import { mockInvoke } from '../../setup';

describe('Proxy Service', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockIsTauriApp = true;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('getProxyStatus', () => {
    it('invokes get_proxy_status in Tauri mode', async () => {
      mockIsTauriApp = true;
      const status = { running: true, port: 8080, host: '127.0.0.1' };
      mockInvoke.mockResolvedValueOnce(status);

      const result = await getProxyStatus();

      expect(mockInvoke).toHaveBeenCalledWith('get_proxy_status', undefined);
      expect(result).toEqual(status);
    });

    it('uses fetch in Web mode', async () => {
      mockIsTauriApp = false;
      const status = { running: false };
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: status }),
      });

      const result = await getProxyStatus();

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/proxy/status'),
        undefined
      );
      expect(result).toEqual(status);
    });
  });

  describe('startProxy', () => {
    it('invokes start_proxy in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Proxy started');

      const result = await startProxy({
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

    it('uses POST in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: 'Started' }),
      });

      await startProxy({
        host: '0.0.0.0',
        port: 8888,
        start_port: 9000,
        default_context: 2048,
      });

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/proxy/start'),
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({
            host: '0.0.0.0',
            port: 8888,
            start_port: 9000,
            default_context: 2048,
          }),
        })
      );
    });

    it('handles minimal config', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Started');

      await startProxy({ host: '127.0.0.1', port: 8080 });

      expect(mockInvoke).toHaveBeenCalledWith('start_proxy', {
        host: '127.0.0.1',
        port: 8080,
        startPort: undefined,
        defaultContext: undefined,
      });
    });
  });

  describe('stopProxy', () => {
    it('invokes stop_proxy in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce('Proxy stopped');

      const result = await stopProxy();

      expect(mockInvoke).toHaveBeenCalledWith('stop_proxy', undefined);
      expect(result).toBe('Proxy stopped');
    });

    it('uses POST in Web mode', async () => {
      mockIsTauriApp = false;
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: () => Promise.resolve({ success: true, data: 'Stopped' }),
      });

      await stopProxy();

      expect(mockFetch).toHaveBeenCalledWith(
        expect.stringContaining('/proxy/stop'),
        expect.objectContaining({ method: 'POST' })
      );
    });
  });
});
