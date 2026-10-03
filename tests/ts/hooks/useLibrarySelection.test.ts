/**
 * The library's one selection: a far pick and a local pick clear each other,
 * a far pick tells the native menu nothing here is selected, and a far pick
 * does not outlive the rows it was made from.
 */

import { describe, it, expect, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';

import { useLibrarySelection } from '../../../src/hooks/useLibrarySelection';
import type { PairedGroup } from '../../../src/hooks/usePairedModels';
import type { ModelRef } from '../../../src/types/generated/ModelRef';
import { FAR_FINGERPRINT, farEntry, pairedModels } from '../fixtures/fakeFarDaemon';

const group: PairedGroup = pairedModels([farEntry('qwen3', 3)]);
const far: ModelRef = { machine: { kind: 'paired', fingerprint: FAR_FINGERPRINT }, id: 3 };

function mount(initial: PairedGroup | null = group) {
  const selectModel = vi.fn();
  const hook = renderHook(({ rows }) => useLibrarySelection(selectModel, rows), {
    initialProps: { rows: initial },
  });
  return { hook, selectModel };
}

describe('useLibrarySelection', () => {
  it('a far pick clears this machine’s, which the native menu then holds as none', () => {
    const { hook, selectModel } = mount();
    act(() => hook.result.current.pickLocal(3));
    expect(selectModel).toHaveBeenLastCalledWith(3);

    act(() => hook.result.current.pickFar(far));

    expect(hook.result.current.farPick).toEqual(far);
    expect(selectModel).toHaveBeenLastCalledWith(null);
  });

  it('a local pick clears the far one', () => {
    const { hook, selectModel } = mount();
    act(() => hook.result.current.pickFar(far));

    act(() => hook.result.current.pickLocal(3));

    expect(hook.result.current.farPick).toBeNull();
    expect(selectModel).toHaveBeenLastCalledWith(3);
  });

  it('a far pick is dropped when its rows go, and does not come back with them', () => {
    const { hook } = mount();
    act(() => hook.result.current.pickFar(far));

    hook.rerender({ rows: null });
    expect(hook.result.current.farPick).toBeNull();

    hook.rerender({ rows: group });
    expect(hook.result.current.farPick).toBeNull();
  });

  it('a far pick is dropped when the rows are another machine’s', () => {
    const { hook } = mount();
    act(() => hook.result.current.pickFar(far));

    hook.rerender({ rows: { ...group, machine: { kind: 'paired', fingerprint: 'ffee11223344' } } });

    expect(hook.result.current.farPick).toBeNull();
  });
});
