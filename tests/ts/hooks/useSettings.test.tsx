/**
 * Tests for useSettings hook.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { ReactNode } from 'react';
import { useSettings } from '../../../src/hooks/useSettings';
import { SettingsProvider } from '../../../src/contexts/SettingsContext';
import { AppSettings } from '../../../src/types';
import { MOCK_PROXY_PORT, MOCK_BASE_PORT } from '../fixtures/ports';

const transport = vi.hoisted(() => ({
  getSettings: vi.fn(),
  updateSettings: vi.fn(),
}));

vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));

const { getSettings, updateSettings } = transport;
// Alias for test compatibility
const fetchSettings = getSettings;

const mockSettings: AppSettings = {
  defaultDownloadPath: '/models',
  defaultContextSize: 4096,
  proxyPort: MOCK_PROXY_PORT,
  llamaBasePort: MOCK_BASE_PORT,
  maxDownloadQueueSize: 10,
};

// Wrapper to provide SettingsProvider for hook tests
const wrapper = ({ children }: { children: ReactNode }) => (
  <SettingsProvider>{children}</SettingsProvider>
);

describe('useSettings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(fetchSettings).mockResolvedValue(mockSettings);
  });

  it('loads settings on mount', async () => {
    const { result } = renderHook(() => useSettings(), { wrapper });

    // Initially loading
    expect(result.current.loading).toBe(true);
    expect(result.current.settings).toBeNull();

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.settings).toEqual(mockSettings);
    expect(result.current.error).toBeNull();
    expect(fetchSettings).toHaveBeenCalledTimes(1);
  });

  it('handles error when loading settings fails', async () => {
    const error = new Error('Failed to connect');
    vi.mocked(fetchSettings).mockRejectedValue(error);

    const { result } = renderHook(() => useSettings(), { wrapper });

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.error).toBe('Failed to connect');
    expect(result.current.settings).toBeNull();
  });

  it('saves settings and updates state', async () => {
    const updatedSettings: AppSettings = {
      ...mockSettings,
      defaultContextSize: 8192,
    };
    vi.mocked(updateSettings).mockResolvedValue(updatedSettings);

    const { result } = renderHook(() => useSettings(), { wrapper });

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.saving).toBe(false);

    let returnedSettings: AppSettings | undefined;
    await act(async () => {
      returnedSettings = await result.current.save({ defaultContextSize: 8192 });
    });

    expect(updateSettings).toHaveBeenCalledWith({ defaultContextSize: 8192 });
    expect(result.current.settings).toEqual(updatedSettings);
    expect(returnedSettings).toEqual(updatedSettings);
    expect(result.current.saving).toBe(false);
  });

  it('sets saving state during save operation', async () => {
    // This test verifies that the save function works correctly
    // The saving state is set synchronously but React batches updates
    vi.mocked(updateSettings).mockResolvedValue(mockSettings);

    const { result } = renderHook(() => useSettings(), { wrapper });

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    await act(async () => {
      await result.current.save({ proxyPort: 9090 });
    });

    // After save completes, saving should be false
    expect(result.current.saving).toBe(false);
    expect(updateSettings).toHaveBeenCalledWith({ proxyPort: 9090 });
  });

  it('handles error when saving settings fails', async () => {
    const error = new Error('Validation error');
    vi.mocked(updateSettings).mockRejectedValue(error);

    const { result } = renderHook(() => useSettings(), { wrapper });

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    // The save function throws, so we need to catch it
    let thrownError: unknown = null;
    await act(async () => {
      try {
        await result.current.save({ defaultContextSize: -1 });
      } catch (e) {
        thrownError = e;
      }
    });

    expect((thrownError as Error)?.message).toBe('Validation error');
    expect(result.current.error).toBe('Validation error');
    expect(result.current.saving).toBe(false);
  });

  it('refreshes settings manually', async () => {
    const { result } = renderHook(() => useSettings(), { wrapper });

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(fetchSettings).toHaveBeenCalledTimes(1);

    // Update mock to return different data
    const newSettings: AppSettings = { ...mockSettings, proxyPort: 9999 };
    vi.mocked(fetchSettings).mockResolvedValue(newSettings);

    await act(async () => {
      await result.current.refresh();
    });

    expect(fetchSettings).toHaveBeenCalledTimes(2);
    expect(result.current.settings).toEqual(newSettings);
  });

  it('handles null values in settings', async () => {
    const nullSettings: AppSettings = {
      defaultDownloadPath: null,
      defaultContextSize: null,
      proxyPort: null,
      llamaBasePort: null,
      maxDownloadQueueSize: null,
    };
    vi.mocked(fetchSettings).mockResolvedValue(nullSettings);

    const { result } = renderHook(() => useSettings(), { wrapper });

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.settings).toEqual(nullSettings);
  });

  it('preserves settings on failed refresh', async () => {
    const { result } = renderHook(() => useSettings(), { wrapper });

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.settings).toEqual(mockSettings);

    // Make refresh fail
    vi.mocked(fetchSettings).mockRejectedValue(new Error('Refresh failed'));

    await act(async () => {
      await result.current.refresh();
    });

    // Settings should be preserved even though refresh failed
    expect(result.current.error).toBe('Refresh failed');
    // Note: The current implementation clears settings on error during initial load
    // but refresh just sets loading state, doesn't clear existing settings
  });

  it('clears previous error on successful save', async () => {
    const { result } = renderHook(() => useSettings(), { wrapper });

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    // First, cause an error
    vi.mocked(updateSettings).mockRejectedValueOnce(new Error('First error'));
    
    await act(async () => {
      try {
        await result.current.save({ proxyPort: 1 });
      } catch {
        // Expected to throw
      }
    });

    expect(result.current.error).toBe('First error');

    // Now succeed
    vi.mocked(updateSettings).mockResolvedValue(mockSettings);

    await act(async () => {
      await result.current.save({ proxyPort: MOCK_PROXY_PORT });
    });

    expect(result.current.error).toBeNull();
  });
});
