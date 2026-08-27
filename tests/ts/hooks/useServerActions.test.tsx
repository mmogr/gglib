/**
 * Tests for the `useServerActions` hook: `handleSave`'s null-clearing
 * regression for `serverDefaults`, and what `handleStartServer` puts on the
 * wire — the sampling half it used to drop, and the context it used to invent.
 *
 * Bug history: `handleSave` used to do `editedServerDefaults ?? undefined`,
 * which coerces `null` (the "clear override" sentinel) into `undefined`.
 * Since the request body is later serialized with plain `JSON.stringify`,
 * keys with an `undefined` value are dropped entirely — so "clear override"
 * silently became a no-op. These tests assert the update payload passed to
 * `onUpdateModel` contains a literal `serverDefaults: null` (not `undefined`,
 * not omitted) when the user clears an override.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { ReactNode } from 'react';
import { useServerActions, ServerActionsConfig } from '../../../src/components/ModelInspectorPanel/hooks/useServerActions';

/**
 * `useServerActions` must not be able to see the stored context default.
 *
 * It used to read `settings.defaultContextSize` and send it as an explicit
 * `contextLength`, which outranked the model's own
 * `server_defaults.context_length` one rung below — see ADR 0009. Removing the
 * field is what makes that unwritable rather than merely unwritten, and this
 * is what fails if the field comes back.
 */
type ConfigCannotSeeSettings = 'settings' extends keyof ServerActionsConfig ? never : true;
const _configCannotSeeSettings: ConfigCannotSeeSettings = true;
void _configCannotSeeSettings;
import { ToastProvider } from '../../../src/contexts/ToastContext';
import { guiModel } from '../fixtures/model';

const serveModel = vi.fn();
const startPinnedProxy = vi.fn();

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => ({ serveModel, startPinnedProxy }),
}));

const wrapper = ({ children }: { children: ReactNode }) => (
  <ToastProvider>{children}</ToastProvider>
);

const baseModel = guiModel({ serverDefaults: { contextLength: 8192 } });

/** Build a minimal ServerActionsConfig with sensible no-op defaults. */
function makeConfig(overrides: Partial<ServerActionsConfig>): ServerActionsConfig {
  return {
    model: baseModel,
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
   * seventeen of the eighteen sampling parameters — all but `seed`, which has
   * no control. `handleStartServer` used to copy nine of them into the
   * `ServeConfig` by name, so the other nine went missing: the eight the form
   * offers (the four DRY knobs, both dynatemp knobs, `frequencyPenalty` and
   * `topNSigma`) were dropped between the form and the request, and `seed`
   * had no way through at all. `toStartServerRequest` kept the same nine-name
   * list, so each value was discarded twice over.
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

describe('useServerActions handleStartServer — the context it sends', () => {
  /**
   * The Serve action used to resolve the context itself and send the answer as
   * an explicit `contextLength`, which is the ladder's top rung. Two rungs
   * were defeated by that. The GGUF's *trained* window outranked the fit
   * gglib computes from this machine's memory, so a GUI user serving a model
   * with nothing configured could be handed a context that will not load; and
   * the stored global default outranked the model's own
   * `server_defaults.context_length` one rung below it, so a per-model
   * override was silently ignored. The daemon reads the same setting into
   * `globals.default_ctx` itself, so neither bought anything. See ADR 0009.
   */
  const trained = guiModel({ contextLength: 262144 });

  beforeEach(() => {
    serveModel.mockReset();
    serveModel.mockResolvedValue(undefined);
    startPinnedProxy.mockReset();
    startPinnedProxy.mockResolvedValue({ port: 11434 });
  });

  it('sends nothing when nothing is typed, so admission can fit it', async () => {
    const { result } = renderHook(
      () => useServerActions(makeConfig({ model: trained, customContext: '' })),
      { wrapper },
    );

    await act(async () => {
      await result.current.handleStartServer();
    });

    expect(serveModel).toHaveBeenCalledTimes(1);
    const sent = serveModel.mock.calls[0][0];
    // Absent, not 262144. The key must be missing rather than present: the
    // request is serialised with plain `JSON.stringify`, which drops an
    // `undefined` value, and `StartServerRequest::context_length` is an
    // `Option` with no `#[serde(default)]`, so a missing key deserialises to
    // `None` and leaves every rung below `Explicit` reachable.
    expect(sent.contextLength).toBeUndefined();
    expect(JSON.stringify(sent)).not.toContain('contextLength');
  });

  // The stored default gets no sibling runtime case: `settings` is not a field
  // of `ServerActionsConfig`, so this hook cannot read one and a test cannot
  // supply one. A draft of this file carried a test whose name claimed to
  // cover that rung and whose body was a copy of the one above — it passed
  // with the fallthrough restored, which is the worst kind of green.
  //
  // The guard is the assertion below instead, which is a compile-time one.
  // Restoring `else if (settings?.defaultContextSize)` means putting `settings`
  // back on the config, and that is what this refuses to typecheck.

  it('sends a typed value, which is the one thing that belongs on the explicit rung', async () => {
    const { result } = renderHook(
      () => useServerActions(makeConfig({ model: trained, customContext: '16384' })),
      { wrapper },
    );

    await act(async () => {
      await result.current.handleStartServer();
    });

    expect(serveModel.mock.calls[0][0].contextLength).toBe(16384);
  });

  it('refuses a non-positive typed value rather than putting it on the explicit rung', async () => {
    // `0` and `-1` parse fine, so only the `> 0` guard stops them. Without it
    // they reach `contextLength` and land on the ladder's TOP rung, and
    // llama-server is launched with `--ctx-size 0`. The pinned branch's
    // equivalent guard is covered below; this is the bare path's.
    for (const typed of ['0', '-1']) {
      serveModel.mockClear();
      const { result } = renderHook(
        () => useServerActions(makeConfig({ model: trained, customContext: typed })),
        { wrapper },
      );

      await act(async () => {
        await result.current.handleStartServer();
      });

      expect(serveModel.mock.calls[0][0].contextLength).toBeUndefined();
    }
  });

  it('sends only what the user typed on the pinned path', async () => {
    // The pinned branch has always overridden `contextLength` with the typed
    // value alone. The shared computation now agrees with it, so this asserts
    // the two paths stay agreed rather than a correction being applied.
    const { result } = renderHook(
      () => useServerActions(makeConfig({ model: trained, customContext: '', pinProxy: true })),
      { wrapper },
    );

    await act(async () => {
      await result.current.handleStartServer();
    });

    expect(serveModel).not.toHaveBeenCalled();
    expect(startPinnedProxy).toHaveBeenCalledTimes(1);
    expect(startPinnedProxy.mock.calls[0][0].options.contextLength).toBeUndefined();
  });

  it('sends a typed value on the pinned path', async () => {
    // The other direction of the same branch. Without this, inverting the
    // `Number.isFinite(customCtx) && customCtx > 0` guard would make every
    // pinned serve ignore what the user typed, and the suite would stay green.
    const { result } = renderHook(
      () =>
        useServerActions(
          makeConfig({ model: trained, customContext: '16384', pinProxy: true }),
        ),
      { wrapper },
    );

    await act(async () => {
      await result.current.handleStartServer();
    });

    expect(startPinnedProxy.mock.calls[0][0].options.contextLength).toBe(16384);
  });
});
