/**
 * The library's one selection: a far pick and a local pick clear each other,
 * the local pick is the native menu's, a far pick tells the menu nothing here
 * is selected, and a far pick does not outlive the rows it was made from.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';

const setSelectedModel = vi.hoisted(() => vi.fn(async (_id: number | null) => {}));
vi.mock('../../../src/services/platform', () => ({
  setSelectedModel,
  appLogger: { warn: vi.fn(), error: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

import { useLibrarySelection } from '../../../src/hooks/useLibrarySelection';
import type { PairedGroup } from '../../../src/hooks/usePairedModels';
import type { ModelRef } from '../../../src/types/generated/ModelRef';
import { FAR_FINGERPRINT, farEntry, pairedModels } from '../fixtures/fakeFarDaemon';

const group: PairedGroup = pairedModels([farEntry('qwen3', 3)]);
const far: ModelRef = { machine: { kind: 'paired', fingerprint: FAR_FINGERPRINT }, id: 3 };

function mount(initial: PairedGroup | null = group) {
  return renderHook(({ rows }) => useLibrarySelection(rows), { initialProps: { rows: initial } });
}

describe('useLibrarySelection', () => {
  beforeEach(() => setSelectedModel.mockClear());

  it('a local pick is held here and told to the native menu', () => {
    const hook = mount();

    act(() => hook.result.current.pickLocal(3));

    expect(hook.result.current.localPick).toBe(3);
    expect(setSelectedModel).toHaveBeenLastCalledWith(3);
  });

  it('a removed model drops the local pick only when it was the one picked', () => {
    const hook = mount();
    act(() => hook.result.current.pickLocal(3));

    act(() => hook.result.current.dropLocal(4));
    expect(hook.result.current.localPick).toBe(3);
    expect(setSelectedModel).toHaveBeenLastCalledWith(3);

    act(() => hook.result.current.dropLocal(3));
    expect(hook.result.current.localPick).toBeNull();
    expect(setSelectedModel).toHaveBeenLastCalledWith(null);
  });

  it('a far pick clears this machine’s, which the native menu then holds as none', () => {
    const hook = mount();
    act(() => hook.result.current.pickLocal(3));

    act(() => hook.result.current.pickFar(far));

    expect(hook.result.current.farPick).toEqual(far);
    expect(hook.result.current.localPick).toBeNull();
    expect(setSelectedModel).toHaveBeenLastCalledWith(null);
  });

  it('a local pick clears the far one', () => {
    const hook = mount();
    act(() => hook.result.current.pickFar(far));

    act(() => hook.result.current.pickLocal(3));

    expect(hook.result.current.farPick).toBeNull();
    expect(hook.result.current.localPick).toBe(3);
  });

  it('a far pick is dropped when its rows go, and does not come back with them', () => {
    const hook = mount();
    act(() => hook.result.current.pickFar(far));

    hook.rerender({ rows: null });
    expect(hook.result.current.farPick).toBeNull();

    hook.rerender({ rows: group });
    expect(hook.result.current.farPick).toBeNull();
  });

  it('a far pick is dropped when the rows are another machine’s', () => {
    const hook = mount();
    act(() => hook.result.current.pickFar(far));

    hook.rerender({ rows: { ...group, machine: { kind: 'paired', fingerprint: 'ffee11223344' } } });

    expect(hook.result.current.farPick).toBeNull();
  });
});
