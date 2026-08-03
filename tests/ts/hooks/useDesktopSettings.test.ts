/**
 * Tests for useDesktopSettings — the always-on proxy toggle group.
 */

import { describe, it, expect } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useDesktopSettings } from '../../../src/components/SettingsModal/useDesktopSettings';
import type { AppSettings } from '../../../src/types';

describe('useDesktopSettings', () => {
  it('defaults every toggle to off when no settings have loaded yet', () => {
    const { result } = renderHook(() => useDesktopSettings(null));

    expect(result.current.values).toEqual({
      proxyAutostart: false,
      closeToTray: false,
      startAtLogin: false,
    });
  });

  it('seeds from persisted settings', () => {
    const settings = {
      proxyAutostart: true,
      closeToTray: false,
      startAtLogin: true,
    } as AppSettings;

    const { result } = renderHook(() => useDesktopSettings(settings));

    expect(result.current.values).toEqual({
      proxyAutostart: true,
      closeToTray: false,
      startAtLogin: true,
    });
  });

  /**
   * Each setting is tri-state on the wire but binary in the UI. An unset
   * value has to read as off rather than as `undefined`, or the checkbox
   * would flip from uncontrolled to controlled on first save.
   */
  it('treats an unset setting as off', () => {
    const settings = { proxyAutostart: null } as AppSettings;

    const { result } = renderHook(() => useDesktopSettings(settings));

    expect(result.current.values.proxyAutostart).toBe(false);
    expect(result.current.values.closeToTray).toBe(false);
  });

  it('updates a single toggle without disturbing the others', () => {
    const { result } = renderHook(() => useDesktopSettings(null));

    act(() => result.current.setValue('closeToTray', true));

    expect(result.current.values).toEqual({
      proxyAutostart: false,
      closeToTray: true,
      startAtLogin: false,
    });
  });

  it('exposes the current values as the update payload', () => {
    const { result } = renderHook(() => useDesktopSettings(null));

    act(() => result.current.setValue('proxyAutostart', true));

    expect(result.current.updates).toEqual({
      proxyAutostart: true,
      closeToTray: false,
      startAtLogin: false,
    });
  });

  it('reset returns every toggle to off', () => {
    const settings = {
      proxyAutostart: true,
      closeToTray: true,
      startAtLogin: true,
    } as AppSettings;

    const { result } = renderHook(() => useDesktopSettings(settings));
    act(() => result.current.reset());

    expect(result.current.values).toEqual({
      proxyAutostart: false,
      closeToTray: false,
      startAtLogin: false,
    });
  });
});
