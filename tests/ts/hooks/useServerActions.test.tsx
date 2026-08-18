/**
 * Tests for useServerActions hook — specifically the `handleSave` null-clearing
 * regression for `serverDefaults`.
 *
 * Bug history: `handleSave` used to do `editedServerDefaults ?? undefined`,
 * which coerces `null` (the "clear override" sentinel) into `undefined`.
 * Since the request body is later serialized with plain `JSON.stringify`,
 * keys with an `undefined` value are dropped entirely — so "clear override"
 * silently became a no-op. These tests assert the update payload passed to
 * `onUpdateModel` contains a literal `serverDefaults: null` (not `undefined`,
 * not omitted) when the user clears an override.
 */

import { describe, it, expect, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { ReactNode } from 'react';
import { useServerActions, ServerActionsConfig } from '../../../src/components/ModelInspectorPanel/hooks/useServerActions';
import { ToastProvider } from '../../../src/contexts/ToastContext';
import { guiModel } from '../fixtures/model';

const serveModel = vi.fn();

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({ serveModel }),
}));

const wrapper = ({ children }: { children: ReactNode }) => (
  <ToastProvider>{children}</ToastProvider>
);

const baseModel = guiModel({ serverDefaults: { contextLength: 8192 } });

/** Build a minimal ServerActionsConfig with sensible no-op defaults. */
function makeConfig(overrides: Partial<ServerActionsConfig>): ServerActionsConfig {
  return {
    model: baseModel,
    settings: null,
    servers: [],
    editedName: baseModel.name,
    editedQuantization: '',
    editedFilePath: baseModel.filePath,
    editedInferenceDefaults: undefined,
    customContext: '',
    customPort: '',
    jinjaOverride: null,
    hasAgentTag: false,
    hasMtpTag: false,
    pinProxy: false,
    mtpNMaxOverride: null,
    mtpPMinOverride: null,
    inferenceParams: undefined,
    editedServerDefaults: undefined,
    onStopServer: vi.fn(),
    onRemoveModel: vi.fn(),
    onUpdateModel: vi.fn().mockResolvedValue(undefined),
    onStartServer: vi.fn(),
    setIsServing: vi.fn(),
    setIsDeleting: vi.fn(),
    closeServeModal: vi.fn(),
    closeDeleteModal: vi.fn(),
    resetEditState: vi.fn(),
    ...overrides,
  };
}

describe('useServerActions handleSave — serverDefaults null-clearing', () => {
  it('emits a literal serverDefaults: null when the override is cleared', async () => {
    const onUpdateModel = vi.fn().mockResolvedValue(undefined);
    const config = makeConfig({
      // User cleared the override in the edit form.
      editedServerDefaults: null,
      onUpdateModel,
    });

    const { result } = renderHook(() => useServerActions(config), { wrapper });

    await act(async () => {
      await result.current.handleSave();
    });

    expect(onUpdateModel).toHaveBeenCalledTimes(1);
    const [, updates] = onUpdateModel.mock.calls[0];

    // Key must be present and literally null — not dropped, not undefined.
    expect(updates).toHaveProperty('serverDefaults');
    expect(updates.serverDefaults).toBeNull();
    expect(updates.serverDefaults).not.toBeUndefined();

    // Guard against the exact regression: JSON.stringify must retain the key.
    expect(JSON.stringify(updates)).toContain('"serverDefaults":null');
  });

  it('omits serverDefaults when the override was not touched', async () => {
    const onUpdateModel = vi.fn().mockResolvedValue(undefined);
    const config = makeConfig({
      // Untouched: matches the model's current value.
      editedServerDefaults: baseModel.serverDefaults,
      onUpdateModel,
    });

    const { result } = renderHook(() => useServerActions(config), { wrapper });

    await act(async () => {
      await result.current.handleSave();
    });

    // Nothing changed at all, so onUpdateModel should not be called.
    expect(onUpdateModel).not.toHaveBeenCalled();
  });

  it('emits the new object when the override is set to a new value', async () => {
    const onUpdateModel = vi.fn().mockResolvedValue(undefined);
    const config = makeConfig({
      editedServerDefaults: { contextLength: 32768 },
      onUpdateModel,
    });

    const { result } = renderHook(() => useServerActions(config), { wrapper });

    await act(async () => {
      await result.current.handleSave();
    });

    expect(onUpdateModel).toHaveBeenCalledTimes(1);
    const [, updates] = onUpdateModel.mock.calls[0];
    expect(updates.serverDefaults).toEqual({ contextLength: 32768 });
  });
});

describe('useServerActions handleStartServer — the sampling half of a serve', () => {
  /**
   * The serve modal renders the whole `InferenceParametersForm`, which offers
   * all eighteen sampling parameters. `handleStartServer` used to copy nine of
   * them into the `ServeConfig` by name, so the other nine — the four DRY
   * knobs, both dynatemp knobs, `frequencyPenalty`, `topNSigma` and `seed` —
   * were dropped between the form and the request, on a surface that had just
   * offered them. `toStartServerRequest` had the same nine-name list, so the
   * value was discarded twice over.
   */
  it('carries every parameter the form offers, not the nine it used to name', async () => {
    serveModel.mockReset();
    serveModel.mockResolvedValue(undefined);

    const offered = {
      temperature: 0.7,
      dryMultiplier: 0.8,
      dryBase: 1.75,
      dryAllowedLength: 2,
      dryPenaltyLastN: 64,
      frequencyPenalty: 0.1,
      dynatempRange: 0.5,
      dynatempExponent: 1,
      topNSigma: 3,
    };

    const { result } = renderHook(
      () => useServerActions(makeConfig({ inferenceParams: offered })),
      { wrapper },
    );

    await act(async () => {
      await result.current.handleStartServer();
    });

    expect(serveModel).toHaveBeenCalledTimes(1);
    expect(serveModel.mock.calls[0][0]).toMatchObject(offered);
  });
});
