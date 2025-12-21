/**
 * Tests for ReasoningTimingTracker.
 * 
 * Uses a fake clock for deterministic timing tests.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { ReasoningTimingTracker } from '../../../../src/hooks/useGglibRuntime/reasoningTiming';
import type { Clock } from '../../../../src/hooks/useGglibRuntime/clock';

/** Fake clock for deterministic timing tests */
class FakeClock implements Clock {
  private time = 0;

  now(): number {
    return this.time;
  }

  advance(ms: number): void {
    this.time += ms;
  }

  set(ms: number): void {
    this.time = ms;
  }
}

describe('ReasoningTimingTracker', () => {
  let clock: FakeClock;
  let tracker: ReasoningTimingTracker;

  beforeEach(() => {
    clock = new FakeClock();
    tracker = new ReasoningTimingTracker(clock);
  });

  it('starts a segment on first reasoning', () => {
    clock.set(1000);
    tracker.onReasoning('msg1');

    const state = tracker.getState();
    expect(state.open.get('msg1')).toEqual({
      segmentIndex: 0,
      startMs: 1000,
    });
  });

  it('does not start multiple segments for same message', () => {
    clock.set(1000);
    tracker.onReasoning('msg1');
    
    clock.set(1500);
    tracker.onReasoning('msg1'); // Should be ignored

    const state = tracker.getState();
    expect(state.open.get('msg1')).toEqual({
      segmentIndex: 0,
      startMs: 1000, // Still original start time
    });
  });

  it('ends segment on boundary', () => {
    clock.set(1000);
    tracker.onReasoning('msg1');
    
    clock.set(3200);
    tracker.onBoundary('msg1');

    const state = tracker.getState();
    expect(state.open.has('msg1')).toBe(false);
    expect(state.completed.get('msg1')).toEqual([
      {
        segmentIndex: 0,
        startMs: 1000,
        endMs: 3200,
      },
    ]);
  });

  it('calculates duration for completed segment', () => {
    clock.set(1000);
    tracker.onReasoning('msg1');
    
    clock.set(6200);
    tracker.onBoundary('msg1');

    const duration = tracker.getDurationSec('msg1', 0);
    expect(duration).toBe(5.2); // (6200 - 1000) / 1000
  });

  it('returns null for incomplete segment duration', () => {
    clock.set(1000);
    tracker.onReasoning('msg1');

    const duration = tracker.getDurationSec('msg1', 0);
    expect(duration).toBeNull();
  });

  it('calculates elapsed time for open segment', () => {
    clock.set(1000);
    tracker.onReasoning('msg1');
    
    clock.set(3500);
    const elapsed = tracker.getElapsedMs('msg1', 0);
    expect(elapsed).toBe(2500); // 3500 - 1000
  });

  it('returns null for closed segment elapsed time', () => {
    clock.set(1000);
    tracker.onReasoning('msg1');
    
    clock.set(3200);
    tracker.onBoundary('msg1');

    const elapsed = tracker.getElapsedMs('msg1', 0);
    expect(elapsed).toBeNull();
  });

  it('tracks multiple segments for same message', () => {
    // First segment
    clock.set(1000);
    tracker.onReasoning('msg1');
    clock.set(3000);
    tracker.onBoundary('msg1'); // Tool call boundary

    // Second segment
    clock.set(3500);
    tracker.onReasoning('msg1');
    clock.set(7200);
    tracker.onBoundary('msg1'); // Text boundary

    const state = tracker.getState();
    expect(state.completed.get('msg1')).toEqual([
      { segmentIndex: 0, startMs: 1000, endMs: 3000 },
      { segmentIndex: 1, startMs: 3500, endMs: 7200 },
    ]);

    expect(tracker.getDurationSec('msg1', 0)).toBe(2.0);
    expect(tracker.getDurationSec('msg1', 1)).toBe(3.7);
  });

  it('handles end of message', () => {
    clock.set(1000);
    tracker.onReasoning('msg1');
    
    clock.set(4500);
    tracker.onEndOfMessage('msg1');

    const state = tracker.getState();
    expect(state.open.has('msg1')).toBe(false);
    expect(state.completed.get('msg1')).toEqual([
      {
        segmentIndex: 0,
        startMs: 1000,
        endMs: 4500,
      },
    ]);
  });

  it('tracks multiple messages independently', () => {
    // Message 1
    clock.set(1000);
    tracker.onReasoning('msg1');
    
    // Message 2
    clock.set(1500);
    tracker.onReasoning('msg2');
    
    // End message 1
    clock.set(3000);
    tracker.onBoundary('msg1');
    
    // End message 2
    clock.set(5000);
    tracker.onBoundary('msg2');

    expect(tracker.getDurationSec('msg1', 0)).toBe(2.0);
    expect(tracker.getDurationSec('msg2', 0)).toBe(3.5);
  });

  it('handles boundary with no open segment gracefully', () => {
    tracker.onBoundary('msg1'); // No open segment

    const state = tracker.getState();
    expect(state.open.has('msg1')).toBe(false);
    expect(state.completed.get('msg1')).toBeUndefined();
  });

  it('clears message data', () => {
    clock.set(1000);
    tracker.onReasoning('msg1');
    clock.set(3000);
    tracker.onBoundary('msg1');

    tracker.clearMessage('msg1');

    const state = tracker.getState();
    expect(state.open.has('msg1')).toBe(false);
    expect(state.completed.has('msg1')).toBe(false);
  });

  it('handles complex scenario: reasoning → tool → reasoning → text', () => {
    clock.set(0);
    
    // First reasoning segment
    tracker.onReasoning('msg1');
    clock.advance(1200);
    
    // Tool call boundary (segment 0 ends)
    tracker.onBoundary('msg1');
    clock.advance(500);
    
    // Second reasoning segment
    tracker.onReasoning('msg1');
    clock.advance(2300);
    
    // Text boundary (segment 1 ends)
    tracker.onBoundary('msg1');

    const state = tracker.getState();
    expect(state.completed.get('msg1')).toEqual([
      { segmentIndex: 0, startMs: 0, endMs: 1200 },
      { segmentIndex: 1, startMs: 1700, endMs: 4000 },
    ]);
    
    expect(tracker.getDurationSec('msg1', 0)).toBe(1.2);
    expect(tracker.getDurationSec('msg1', 1)).toBe(2.3);
  });

  it('segment index increments with each new segment', () => {
    // Segment 0
    tracker.onReasoning('msg1');
    clock.advance(1000);
    tracker.onBoundary('msg1');
    
    // Segment 1
    tracker.onReasoning('msg1');
    clock.advance(1000);
    tracker.onBoundary('msg1');
    
    // Segment 2
    tracker.onReasoning('msg1');
    clock.advance(1000);
    tracker.onBoundary('msg1');

    const state = tracker.getState();
    const segments = state.completed.get('msg1')!;
    expect(segments.map(s => s.segmentIndex)).toEqual([0, 1, 2]);
  });
});
