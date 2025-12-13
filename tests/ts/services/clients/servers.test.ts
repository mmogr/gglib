/**
 * Tests for servers client module.
 *
 * Verifies that the client delegates to Transport with no platform branching.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  serveModel,
  stopServer,
  listServers,
} from '../../../../src/services/clients/servers';
import { getTransport, _resetTransport } from '../../../../src/services/transport';
import type { ServerInfo } from '../../../../src/types';

// Mock the transport module
vi.mock('../../../../src/services/transport', () => {
  const mockTransport = {
    serveModel: vi.fn(),
    stopServer: vi.fn(),
    listServers: vi.fn(),
  };

  return {
    getTransport: vi.fn(() => mockTransport),
    _resetTransport: vi.fn(),
  };
});

describe('services/clients/servers', () => {
  const mockTransport = getTransport();

  const mockServerInfo: ServerInfo = {
    model_id: 1,
    model_name: 'Test Model',
    port: 8080,
    status: 'running',
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    _resetTransport();
  });

  describe('serveModel', () => {
    it('delegates to transport.serveModel()', async () => {
      const config = { model_id: 1, port: 8080 };
      const mockResponse = { port: 8080, message: 'Server started' };
      vi.mocked(mockTransport.serveModel).mockResolvedValue(mockResponse);

      const result = await serveModel(config);

      expect(mockTransport.serveModel).toHaveBeenCalledWith(config);
      expect(result).toEqual(mockResponse);
    });
  });

  describe('stopServer', () => {
    it('delegates to transport.stopServer()', async () => {
      const modelId = 1;
      vi.mocked(mockTransport.stopServer).mockResolvedValue(undefined);

      await stopServer(modelId);

      expect(mockTransport.stopServer).toHaveBeenCalledWith(modelId);
    });
  });

  describe('listServers', () => {
    it('delegates to transport.listServers()', async () => {
      vi.mocked(mockTransport.listServers).mockResolvedValue([mockServerInfo]);

      const result = await listServers();

      expect(mockTransport.listServers).toHaveBeenCalledTimes(1);
      expect(result).toEqual([mockServerInfo]);
    });
  });

  describe('no platform branching', () => {
    it('client module delegates all calls through transport', async () => {
      vi.mocked(mockTransport.serveModel).mockResolvedValue({ port: 8080, message: 'ok' });
      vi.mocked(mockTransport.stopServer).mockResolvedValue(undefined);
      vi.mocked(mockTransport.listServers).mockResolvedValue([]);

      await serveModel({ model_id: 1 });
      await stopServer(1);
      await listServers();

      expect(mockTransport.serveModel).toHaveBeenCalled();
      expect(mockTransport.stopServer).toHaveBeenCalled();
      expect(mockTransport.listServers).toHaveBeenCalled();
    });
  });
});
