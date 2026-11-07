/**
 * `inspector/local/useExplain`: a read that a newer one overtook (the
 * profile changed while it was out) never replaces the newer answer, and a
 * daemon that answers nothing is a failure, said once, never a wait.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';

const explainModel = vi.hoisted(() => vi.fn());
vi.mock('../../../src/services/transport/api/models/local', () => ({ explainModel }));

import { useExplain } from '../../../src/inspector/local/useExplain';
import { launchExplanation, samplingExplanation } from '../fixtures/explain';

/** A read that answers when the test says so. */
function held() {
  let answer!: (value: unknown) => void;
  explainModel.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
  return (value: unknown) => act(async () => answer(value));
}

describe('useExplain', () => {
  beforeEach(() => explainModel.mockReset());

  it("keeps the newer profile's answer when the older read lands after it", async () => {
    const first = held();
    const second = held();
    const { result, rerender } = renderHook(({ profile }) => useExplain(9, profile), {
      initialProps: { profile: null as string | null },
    });
    rerender({ profile: 'coding' });
    expect(explainModel).toHaveBeenLastCalledWith(9, 'coding');

    const coding = launchExplanation(samplingExplanation({ profile: 'coding' }));
    await second(coding);
    await first(launchExplanation(samplingExplanation({ profile: null })));
    expect(result.current.explanation).toBe(coding);
    expect(result.current.failed).toBe(false);
  });

  it('keeps the newer answer when an overtaken read fails after it', async () => {
    // The first read (the older profile) fails late; the second answers first.
    explainModel.mockImplementationOnce(() => new Promise((_resolve, reject) => setTimeout(() => reject(new Error('gone')), 0)));
    const second = held();
    const { result, rerender } = renderHook(({ profile }) => useExplain(9, profile), {
      initialProps: { profile: 'coding' as string | null },
    });
    rerender({ profile: null });
    const fresh = launchExplanation(samplingExplanation({ profile: null }));
    await second(fresh);
    await act(async () => new Promise((r) => setTimeout(r, 5)));
    expect(result.current.explanation).toBe(fresh);
    expect(result.current.failed).toBe(false);
  });

  it('is a failure when the daemon has no explanation', async () => {
    explainModel.mockResolvedValueOnce(null);
    const { result } = renderHook(() => useExplain(9, null));
    await waitFor(() => expect(result.current.failed).toBe(true));
    expect(result.current.explanation).toBeNull();
  });
});
