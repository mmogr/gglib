/**
 * Tests for useDrawingSettings — the default image model and the `/mcp`
 * drawing switch.
 */

import { describe, it, expect } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useDrawingSettings } from '../../../src/components/SettingsModal/useDrawingSettings';
import { appSettings } from '../fixtures/settings';

describe('useDrawingSettings', () => {
  it('reads an unset switch as off and an unset model as none', () => {
    // Built once, outside the render: a new object every render would re-seed
    // the state on every render, without end.
    const settings = appSettings();
    const { result } = renderHook(() => useDrawingSettings(settings));

    expect(result.current.values).toEqual({ defaultImageModel: '', mcpDrawing: false });
    expect(result.current.updates).toEqual({});
  });

  it('is off before any settings have loaded', () => {
    const { result } = renderHook(() => useDrawingSettings(null));

    expect(result.current.values.mcpDrawing).toBe(false);
  });

  it('seeds from persisted settings', () => {
    const settings = appSettings({ defaultImageModelId: 7, mcpDrawing: true });

    const { result } = renderHook(() => useDrawingSettings(settings));

    expect(result.current.values).toEqual({ defaultImageModel: '7', mcpDrawing: true });
  });

  it('sends only what changed, and a blank model as a clear', () => {
    const settings = appSettings({ defaultImageModelId: 7 });
    const { result } = renderHook(() => useDrawingSettings(settings));

    act(() => result.current.setValue('mcpDrawing', true));
    expect(result.current.updates).toEqual({ mcpDrawing: true });

    act(() => result.current.setValue('defaultImageModel', '9'));
    expect(result.current.updates).toEqual({ mcpDrawing: true, defaultImageModelId: 9 });

    act(() => result.current.setValue('defaultImageModel', ''));
    expect(result.current.updates).toEqual({ mcpDrawing: true, defaultImageModelId: null });
  });
});
