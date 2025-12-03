/**
 * Tests for system service functions.
 * 
 * Tests desktop utilities and menu sync operations in both Tauri and Web modes.
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

// Import after mocks are set up
import {
  openUrl,
  setSelectedModel,
  syncMenuState,
  syncMenuStateSilent,
} from '../../../../src/services/tauri';
import { mockInvoke } from '../../setup';

describe('System Service', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockIsTauriApp = true;
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('openUrl', () => {
    it('invokes open_url in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(undefined);

      await openUrl('https://example.com');

      expect(mockInvoke).toHaveBeenCalledWith('open_url', { url: 'https://example.com' });
    });

    it('uses window.open in Web mode', async () => {
      mockIsTauriApp = false;
      const mockOpen = vi.fn();
      vi.stubGlobal('open', mockOpen);

      await openUrl('https://example.com');

      expect(mockOpen).toHaveBeenCalledWith('https://example.com', '_blank', 'noopener,noreferrer');
    });

    it('handles URLs with special characters', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(undefined);

      await openUrl('https://example.com/path?query=test&foo=bar');

      expect(mockInvoke).toHaveBeenCalledWith('open_url', {
        url: 'https://example.com/path?query=test&foo=bar',
      });
    });
  });

  describe('setSelectedModel', () => {
    it('invokes set_selected_model in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(undefined);

      await setSelectedModel(1);

      expect(mockInvoke).toHaveBeenCalledWith('set_selected_model', { modelId: 1 });
    });

    it('handles null model ID', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(undefined);

      await setSelectedModel(null);

      expect(mockInvoke).toHaveBeenCalledWith('set_selected_model', { modelId: null });
    });

    it('is no-op in Web mode', async () => {
      mockIsTauriApp = false;

      await setSelectedModel(1);

      expect(mockInvoke).not.toHaveBeenCalled();
    });

    it('does not throw in Web mode', async () => {
      mockIsTauriApp = false;

      await expect(setSelectedModel(42)).resolves.not.toThrow();
    });
  });

  describe('syncMenuState', () => {
    it('invokes sync_menu_state in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(undefined);

      await syncMenuState();

      expect(mockInvoke).toHaveBeenCalledWith('sync_menu_state', undefined);
    });

    it('is no-op in Web mode', async () => {
      mockIsTauriApp = false;

      await syncMenuState();

      expect(mockInvoke).not.toHaveBeenCalled();
    });

    it('propagates errors in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockRejectedValueOnce(new Error('Menu sync failed'));

      await expect(syncMenuState()).rejects.toThrow('Menu sync failed');
    });
  });

  describe('syncMenuStateSilent', () => {
    it('invokes sync_menu_state in Tauri mode', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockResolvedValueOnce(undefined);

      syncMenuStateSilent();

      // Wait for the async operation to complete
      await new Promise((resolve) => setTimeout(resolve, 0));

      expect(mockInvoke).toHaveBeenCalledWith('sync_menu_state', undefined);
    });

    it('swallows errors silently', async () => {
      mockIsTauriApp = true;
      mockInvoke.mockRejectedValueOnce(new Error('Menu sync failed'));

      // Should not throw
      syncMenuStateSilent();

      // Wait for the async operation to complete
      await new Promise((resolve) => setTimeout(resolve, 0));

      expect(mockInvoke).toHaveBeenCalledWith('sync_menu_state', undefined);
    });

    it('is no-op in Web mode', async () => {
      mockIsTauriApp = false;

      syncMenuStateSilent();

      await new Promise((resolve) => setTimeout(resolve, 0));

      expect(mockInvoke).not.toHaveBeenCalled();
    });
  });
});
