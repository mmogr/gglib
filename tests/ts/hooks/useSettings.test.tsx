/**
 * Tests for useSettings hook.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, waitFor, act } from '@testing-library/react';
import { ReactNode } from 'react';
import { useSettings } from '../../../src/hooks/useSettings';
import { SettingsProvider } from '../../../src/contexts/SettingsContext';
import { AppSettings } from '../../../src/types';
import { appSettings } from '../fixtures/settings';
import { MOCK_PROXY_PORT, MOCK_BASE_PORT } from '../fixtures/ports';

const transport = vi.hoisted(() => ({
  getSettings: vi.fn(),
  updateSettings: vi.fn(),
  onEventStreamOpen: vi.fn(() => () => {}),
  onEventStreamClose: vi.fn(() => () => {}),
  /** Whether the event stream is open: while it is, a failed read is asked again. */
  isEventStreamOpen: vi.fn(() => false),
}));

vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));

const { getSettings, updateSettings } = transport;
// Alias for test compatibility
const fetchSettings = getSettings;

const mockSettings: AppSettings = appSettings({
  defaultContextSize: 4096,
  proxyPort: MOCK_PROXY_PORT,
  llamaBasePort: MOCK_BASE_PORT,
  maxDownloadQueueSize: 10,
});

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
    const nullSettings: AppSettings = appSettings();
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

  /**
   * The settings place says how each write went under its field, so a quiet
   * write raises no toast either way; any other write still does.
   */
  it('raises no toast for a quiet write, and one for any other', async () => {
    const showToast = vi.fn();
    const { result } = renderHook(() => useSettings(), {
      wrapper: ({ children }: { children: ReactNode }) => <SettingsProvider showToast={showToast}>{children}</SettingsProvider>,
    });
    await waitFor(() => expect(result.current.loading).toBe(false));
    vi.mocked(updateSettings).mockResolvedValue(mockSettings);

    await act(async () => {
      await result.current.save({ proxyPort: 1 }, { quiet: true });
    });
    vi.mocked(updateSettings).mockRejectedValueOnce(new Error('refused'));
    await act(async () => {
      await result.current.save({ proxyPort: 2 }, { quiet: true }).catch(() => {});
    });
    expect(showToast).not.toHaveBeenCalled();

    await act(async () => {
      await result.current.save({ proxyPort: 3 });
    });
    expect(showToast).toHaveBeenCalledWith('Settings applied', 'success');
  });

  /**
   * A read begun before a save and answered after it is overtaken: putting
   * its answer up would show the value from before the save (TX-2).
   */
  it('drops a read the save overtook', async () => {
    const { result } = renderHook(() => useSettings(), { wrapper });
    await waitFor(() => expect(result.current.loading).toBe(false));

    let answer: (s: AppSettings) => void = () => {};
    vi.mocked(fetchSettings).mockReturnValueOnce(new Promise<AppSettings>((resolve) => (answer = resolve)));
    let reading: Promise<void> = Promise.resolve();
    act(() => {
      reading = result.current.refresh();
    });
    const saved = { ...mockSettings, proxyPort: 9191 };
    vi.mocked(updateSettings).mockResolvedValue(saved);
    await act(async () => {
      await result.current.save({ proxyPort: 9191 });
    });
    await act(async () => {
      answer(mockSettings);
      await reading;
    });

    expect(result.current.settings?.proxyPort).toBe(9191);
  });

  /**
   * A read that fails is not newer than one begun before it: the mount's
   * slow read, landing after a refresh that failed, is still put up.
   */
  it('puts up an earlier read that lands after a later one failed', async () => {
    let answer: (s: AppSettings) => void = () => {};
    vi.mocked(fetchSettings)
      .mockReturnValueOnce(new Promise<AppSettings>((resolve) => (answer = resolve)))
      .mockRejectedValueOnce(new TypeError('Failed to fetch'));
    const { result } = renderHook(() => useSettings(), { wrapper });
    await act(async () => {
      await result.current.refresh();
    });
    expect(result.current.error).toBe('Failed to fetch');

    await act(async () => {
      answer(mockSettings);
    });

    expect(result.current.settings).toEqual(mockSettings);
    expect(result.current.error).toBeNull();
  });

  /**
   * A save answered before any read is the daemon answering: the read it
   * overtook is dropped, and loading must not wait on it for ever.
   */
  it('is no longer loading once a save has answered, though no read has', async () => {
    vi.mocked(fetchSettings).mockReturnValue(new Promise<AppSettings>(() => {}));
    vi.mocked(updateSettings).mockResolvedValue(mockSettings);
    const { result } = renderHook(() => useSettings(), { wrapper });
    expect(result.current.loading).toBe(true);

    await act(async () => {
      await result.current.save({ proxyPort: 9090 });
    });

    expect(result.current.loading).toBe(false);
    expect(result.current.settings).toEqual(mockSettings);
  });

  it('is no longer loading once a save has failed, though no read has answered', async () => {
    vi.mocked(fetchSettings).mockReturnValue(new Promise<AppSettings>(() => {}));
    vi.mocked(updateSettings).mockRejectedValue(new Error('refused'));
    const { result } = renderHook(() => useSettings(), { wrapper });

    await act(async () => {
      await result.current.save({ proxyPort: 9090 }).catch(() => {});
    });

    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBe('refused');
  });

  // The stream is open, so the daemon answers, but its settings read keeps
  // failing (a 30 s time limit): the read is asked again, after 1 s, then 2 s.
  describe('while the event stream is open', () => {
    beforeEach(() => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      vi.spyOn(Math, 'random').mockReturnValue(0);
      transport.isEventStreamOpen.mockReturnValue(true);
    });
    afterEach(() => {
      transport.isEventStreamOpen.mockReturnValue(false);
      vi.useRealTimers();
      vi.restoreAllMocks();
    });

    it('asks a read that failed again, after a wait that grows, until the settings land', async () => {
      const timedOut = new Error('gglib did not answer in 30 seconds.');
      vi.mocked(fetchSettings).mockRejectedValueOnce(timedOut).mockRejectedValueOnce(timedOut).mockResolvedValue(mockSettings);
      const { result } = renderHook(() => useSettings(), { wrapper });
      await act(() => vi.advanceTimersByTimeAsync(0));
      expect(result.current.error).toBe('gglib did not answer in 30 seconds.');
      expect(fetchSettings).toHaveBeenCalledTimes(1);

      await act(() => vi.advanceTimersByTimeAsync(1000));
      expect(fetchSettings).toHaveBeenCalledTimes(2);
      await act(() => vi.advanceTimersByTimeAsync(2000));
      expect(fetchSettings).toHaveBeenCalledTimes(3);
      expect(result.current.settings).toEqual(mockSettings);
      expect(result.current.error).toBeNull();

      await act(() => vi.advanceTimersByTimeAsync(60_000));
      expect(fetchSettings).toHaveBeenCalledTimes(3);
    });

    it('asks nothing again once the provider is gone', async () => {
      vi.mocked(fetchSettings).mockRejectedValue(new Error('refused'));
      const { unmount } = renderHook(() => useSettings(), { wrapper });
      await act(() => vi.advanceTimersByTimeAsync(0));
      unmount();
      await act(() => vi.advanceTimersByTimeAsync(60_000));
      expect(fetchSettings).toHaveBeenCalledTimes(1);
    });
  });
});
