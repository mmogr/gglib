/**
 * Tests for batchWithinWindow utility.
 * 
 * Pure function with timer-based behavior - tests verify batching logic
 * and proper cleanup.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createBatchWithinWindow, DEFAULT_BATCH_WINDOW_MS } from '../../../src/utils/batchWithinWindow';

describe('batchWithinWindow', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('exports default window duration of 350ms', () => {
    expect(DEFAULT_BATCH_WINDOW_MS).toBe(350);
  });

  describe('push and auto-flush', () => {
    it('flushes single item after window expires', () => {
      const onFlush = vi.fn();
      const batcher = createBatchWithinWindow(100, onFlush);

      batcher.push('a');
      expect(onFlush).not.toHaveBeenCalled();

      vi.advanceTimersByTime(100);
      expect(onFlush).toHaveBeenCalledTimes(1);
      expect(onFlush).toHaveBeenCalledWith(['a']);
    });

    it('batches multiple items pushed within window', () => {
      const onFlush = vi.fn();
      const batcher = createBatchWithinWindow(100, onFlush);

      batcher.push('a');
      vi.advanceTimersByTime(30);
      batcher.push('b');
      vi.advanceTimersByTime(30);
      batcher.push('c');
      
      expect(onFlush).not.toHaveBeenCalled();

      vi.advanceTimersByTime(40); // Total: 100ms from first push
      expect(onFlush).toHaveBeenCalledTimes(1);
      expect(onFlush).toHaveBeenCalledWith(['a', 'b', 'c']);
    });

    it('starts new window after flush', () => {
      const onFlush = vi.fn();
      const batcher = createBatchWithinWindow(100, onFlush);

      // First batch
      batcher.push('a');
      vi.advanceTimersByTime(100);
      expect(onFlush).toHaveBeenCalledTimes(1);
      expect(onFlush).toHaveBeenLastCalledWith(['a']);

      // Second batch - new window
      batcher.push('b');
      vi.advanceTimersByTime(100);
      expect(onFlush).toHaveBeenCalledTimes(2);
      expect(onFlush).toHaveBeenLastCalledWith(['b']);
    });

    it('items across windows result in separate flushes', () => {
      const onFlush = vi.fn();
      const batcher = createBatchWithinWindow(100, onFlush);

      batcher.push('a');
      vi.advanceTimersByTime(100);
      expect(onFlush).toHaveBeenCalledWith(['a']);

      batcher.push('b');
      vi.advanceTimersByTime(100);
      expect(onFlush).toHaveBeenCalledWith(['b']);

      expect(onFlush).toHaveBeenCalledTimes(2);
    });
  });

  describe('flush', () => {
    it('manually flushes pending items immediately', () => {
      const onFlush = vi.fn();
      const batcher = createBatchWithinWindow(100, onFlush);

      batcher.push('a');
      batcher.push('b');
      expect(onFlush).not.toHaveBeenCalled();

      batcher.flush();
      expect(onFlush).toHaveBeenCalledTimes(1);
      expect(onFlush).toHaveBeenCalledWith(['a', 'b']);
    });

    it('does not call onFlush if no items pending', () => {
      const onFlush = vi.fn();
      const batcher = createBatchWithinWindow(100, onFlush);

      batcher.flush();
      expect(onFlush).not.toHaveBeenCalled();
    });

    it('clears timer after manual flush', () => {
      const onFlush = vi.fn();
      const batcher = createBatchWithinWindow(100, onFlush);

      batcher.push('a');
      batcher.flush();
      expect(onFlush).toHaveBeenCalledTimes(1);

      // Advancing time should not cause another flush
      vi.advanceTimersByTime(100);
      expect(onFlush).toHaveBeenCalledTimes(1);
    });
  });

  describe('dispose', () => {
    it('clears pending timer without flushing', () => {
      const onFlush = vi.fn();
      const batcher = createBatchWithinWindow(100, onFlush);

      batcher.push('a');
      batcher.dispose();

      // Advancing time should not cause flush
      vi.advanceTimersByTime(100);
      expect(onFlush).not.toHaveBeenCalled();
    });

    it('clears items without calling onFlush', () => {
      const onFlush = vi.fn();
      const batcher = createBatchWithinWindow(100, onFlush);

      batcher.push('a');
      batcher.push('b');
      batcher.dispose();

      // Manual flush after dispose should do nothing
      batcher.flush();
      expect(onFlush).not.toHaveBeenCalled();
    });

    it('is safe to call multiple times', () => {
      const onFlush = vi.fn();
      const batcher = createBatchWithinWindow(100, onFlush);

      batcher.push('a');
      batcher.dispose();
      batcher.dispose();
      batcher.dispose();

      expect(onFlush).not.toHaveBeenCalled();
    });
  });

  describe('edge cases', () => {
    it('handles rapid pushes correctly', () => {
      const onFlush = vi.fn();
      const batcher = createBatchWithinWindow(100, onFlush);

      // Push many items rapidly
      for (let i = 0; i < 100; i++) {
        batcher.push(i);
      }

      expect(onFlush).not.toHaveBeenCalled();

      vi.advanceTimersByTime(100);
      expect(onFlush).toHaveBeenCalledTimes(1);
      expect(onFlush).toHaveBeenCalledWith(Array.from({ length: 100 }, (_, i) => i));
    });

    it('uses default window when not specified', () => {
      const onFlush = vi.fn();
      const batcher = createBatchWithinWindow(undefined, onFlush);

      batcher.push('a');
      vi.advanceTimersByTime(DEFAULT_BATCH_WINDOW_MS - 1);
      expect(onFlush).not.toHaveBeenCalled();

      vi.advanceTimersByTime(1);
      expect(onFlush).toHaveBeenCalledWith(['a']);
    });
  });
});
