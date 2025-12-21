/**
 * Tests for settings client module.
 *
 * Verifies that the client delegates to Transport with no platform branching.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { getSettings, updateSettings } from '../../../../src/services/clients/settings';
import { getTransport, _resetTransport } from '../../../../src/services/transport';

// Mock the transport module
vi.mock('../../../../src/services/transport', () => {
  const mockTransport = {
    getSettings: vi.fn(),
    updateSettings: vi.fn(),
  };

  return {
    getTransport: vi.fn(() => mockTransport),
    _resetTransport: vi.fn(),
  };
});

describe('services/clients/settings', () => {
  const mockTransport = getTransport();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    _resetTransport();
  });

  describe('getSettings', () => {
    it('delegates to transport.getSettings()', async () => {
      const mockSettings = {
        default_context_size: 4096,
        theme: 'dark' as const,
      };
      vi.mocked(mockTransport.getSettings).mockResolvedValue(mockSettings);

      const result = await getSettings();

      expect(mockTransport.getSettings).toHaveBeenCalledTimes(1);
      expect(result).toEqual(mockSettings);
    });
  });

  describe('updateSettings', () => {
    it('delegates to transport.updateSettings()', async () => {
      const updates = { default_context_size: 8192 };
      const mockUpdatedSettings = {
        default_context_size: 8192,
        theme: 'dark' as const,
      };
      vi.mocked(mockTransport.updateSettings).mockResolvedValue(mockUpdatedSettings);

      const result = await updateSettings(updates);

      expect(mockTransport.updateSettings).toHaveBeenCalledWith(updates);
      expect(result).toEqual(mockUpdatedSettings);
    });
  });

  describe('no platform branching', () => {
    it('client module delegates all calls through transport', async () => {
      const mockSettings = { default_context_size: 4096, theme: 'dark' as const };
      vi.mocked(mockTransport.getSettings).mockResolvedValue(mockSettings);
      vi.mocked(mockTransport.updateSettings).mockResolvedValue(mockSettings);

      await getSettings();
      await updateSettings({ theme: 'light' });

      expect(mockTransport.getSettings).toHaveBeenCalled();
      expect(mockTransport.updateSettings).toHaveBeenCalled();
    });
  });
});
