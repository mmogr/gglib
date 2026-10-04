/**
 * Whether the composer offers images, by one rule (`canSee`) for each kind
 * of chat: this machine's model by its catalogue entry, the paired
 * machine's by the row that machine lists, read only for a chat on its
 * model, and a far chat always, its model being chosen and judged there.
 * The cost's context follows the same model, and the answer is one object
 * until it changes.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';

vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import { FAR_FINGERPRINT, FakeFarDaemon, farEntry } from '../fixtures/fakeFarDaemon';
import { useImageInput, type ImageInputModel } from '../../../src/hooks/useImageInput';
import { IDLE_STATUS, applyRemoteStatus, resetRemoteState } from '../../../src/services/remoteRegistry';
import type { ModelRef } from '../../../src/types/generated/ModelRef';
import { imageCost } from '../../../src/components/ChatMessagesPanel/components/imageCost';

const CANNOT_SEE = 'This model cannot read images: it has no projector.';
const paired: ModelRef = { machine: { kind: 'paired', fingerprint: FAR_FINGERPRINT }, id: 3 };
const connected = {
  port: 41234,
  base_url: 'http://127.0.0.1:41234/v1',
  ticket_fingerprint: FAR_FINGERPRINT,
  path: 'direct',
  away_for_s: null,
};

let daemons: FakeFarDaemon;

beforeEach(() => {
  daemons = new FakeFarDaemon();
  vi.stubGlobal('fetch', vi.fn(daemons.fetch));
  resetRemoteState();
});
afterEach(() => {
  vi.unstubAllGlobals();
});

const input = (model: Partial<ImageInputModel>) =>
  renderHook(() => useImageInput({ far: false, sees: null, contextLength: null, ...model }));

describe('useImageInput', () => {
  it('offers images to this machine\'s model that sees, with its context', () => {
    expect(input({ sees: true, contextLength: 32_768 }).result.current).toEqual({
      offered: true, reason: null, contextLength: 32_768,
    });
  });

  it('does not offer them to one that cannot, and says why', () => {
    expect(input({ sees: false, contextLength: 32_768 }).result.current).toEqual({
      offered: false, reason: CANNOT_SEE, contextLength: 32_768,
    });
  });

  it('a far chat always offers them, its cost in tokens alone', () => {
    expect(input({ far: true, sees: false, contextLength: 32_768 }).result.current).toEqual({
      offered: true, reason: null, contextLength: null,
    });
  });

  it('judges the paired machine\'s model by the row that machine lists for it', async () => {
    daemons.entries = [farEntry('qwen3', 3, { capabilities: ['vision'], context_window: 40_960 })];
    act(() => applyRemoteStatus({ ...IDLE_STATUS, connected, paired_name: 'desk' }));
    const seeing = input({ paired, sees: false });
    await waitFor(() => expect(seeing.result.current.contextLength).toBe(40_960));
    expect(seeing.result.current).toEqual({ offered: true, reason: null, contextLength: 40_960 });

    daemons.entries = [farEntry('qwen3', 3, { context_window: 40_960 })];
    resetRemoteState();
    act(() => applyRemoteStatus({ ...IDLE_STATUS, connected, paired_name: 'desk' }));
    const blind = input({ paired, sees: true });
    await waitFor(() => expect(blind.result.current.contextLength).toBe(40_960));
    expect(blind.result.current).toEqual({ offered: false, reason: CANNOT_SEE, contextLength: 40_960 });
  });

  it('reads the paired machine\'s rows only for a chat on its model, never for a local or a far one', async () => {
    act(() => applyRemoteStatus({ ...IDLE_STATUS, connected, paired_name: 'desk' }));
    input({ sees: true });
    input({ far: true });
    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });
    act(() => {
      window.dispatchEvent(new Event('focus'));
    });
    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });
    expect(daemons.farCount('GET', '/api/remote/models')).toBe(0);

    input({ paired });
    await waitFor(() => expect(daemons.farCount('GET', '/api/remote/models')).toBe(1));
    act(() => {
      window.dispatchEvent(new Event('focus'));
    });
    await waitFor(() => expect(daemons.farCount('GET', '/api/remote/models')).toBe(2));
    await act(async () => {
      await new Promise((r) => setTimeout(r, 10));
    });
    expect(daemons.farCount('GET', '/api/remote/models')).toBe(2);
  });

  it('gives the same answer, as one object, until it changes', () => {
    const hook = renderHook((model: ImageInputModel) => useImageInput(model), {
      initialProps: { far: false, sees: true, contextLength: 32_768 },
    });
    const first = hook.result.current;
    hook.rerender({ far: false, sees: true, contextLength: 32_768 });
    expect(hook.result.current).toBe(first);

    hook.rerender({ far: false, sees: true, contextLength: 8_192 });
    expect(hook.result.current).not.toBe(first);
    expect(hook.result.current.contextLength).toBe(8_192);
  });
});

describe('imageCost', () => {
  it('says the tokens, and their share of a known context', () => {
    expect(imageCost(1_200, null)).toBe('~1,200 tokens');
    expect(imageCost(1_200, 32_000)).toBe('~1,200 tokens · 4% of context');
    expect(imageCost(100, 32_000)).toBe('~100 tokens · <1% of context');
  });
});
