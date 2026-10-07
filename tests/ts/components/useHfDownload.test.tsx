/**
 * Whether another download may be queued is the daemon's answer, not a count.
 *
 * The inspector once worked it out as running plus waiting against the
 * capacity, which is not the daemon's rule: the running download takes no
 * place in the queue. The two disagreed with one running and nine waiting at
 * a capacity of ten, where the daemon accepts another and the button was off.
 */

import { describe, it, expect, vi } from 'vitest';
import { renderHook } from '@testing-library/react';
import { useHfDownload } from '../../../src/components/ModelInspectorPanel/hooks/useHfDownload';
import { queueSnapshot, runningRow, waitingRow } from '../fixtures/downloads';

vi.mock('../../../src/contexts/ToastContext', () => ({
  useToastContext: () => ({ showToast: vi.fn() }),
}));

describe('useHfDownload', () => {
  it('turns the download button off when the snapshot says the queue is full', () => {
    const { result } = renderHook(() => useHfDownload(queueSnapshot({ full: true })));

    expect(result.current.downloadsDisabled).toBe(true);
    expect(result.current.disabledReason).toBe('Download queue is full');
  });

  it('leaves it on while the snapshot says there is room, whatever the rows add up to', () => {
    const waiting = Array.from({ length: 9 }, (_, i) => waitingRow(`owner/m${i}`, i + 2));
    const snapshot = queueSnapshot({ active: runningRow(), waiting, max_size: 10, full: false });

    const { result } = renderHook(() => useHfDownload(snapshot));

    expect(result.current.downloadsDisabled).toBe(false);
    expect(result.current.disabledReason).toBeUndefined();
  });

  it('leaves it on before the queue is known', () => {
    expect(renderHook(() => useHfDownload(null)).result.current.downloadsDisabled).toBe(false);
  });
});
