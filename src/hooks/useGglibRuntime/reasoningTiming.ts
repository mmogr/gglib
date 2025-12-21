/**
 * Pure timing tracker for reasoning segments.
 * 
 * Tracks start/end times for reasoning segments without React or assistant-ui dependencies.
 * Segment index corresponds to the order of <think> blocks in the final transcript.
 * 
 * @module reasoningTiming
 */

import type { Clock } from './clock';

/**
 * Timing information for a single reasoning segment.
 */
export type ReasoningSegmentTiming = {
  /** Index of this segment (0-based, order in transcript) */
  segmentIndex: number;
  /** Start time in milliseconds */
  startMs: number;
  /** End time in milliseconds (undefined while streaming) */
  endMs?: number;
};

/**
 * Tracks timing for reasoning segments across messages.
 * 
 * Lifecycle:
 * - onReasoning(): Start a new segment (if not already open)
 * - onBoundary(): End current segment (tool-call or text part)
 * - onEndOfMessage(): End current segment (message complete)
 * 
 * Segment index = number of completed segments when a new segment starts.
 * This matches the order of <think> blocks in the final transcript.
 */
export class ReasoningTimingTracker {
  /** Completed segments by message ID */
  private segmentsByMsg = new Map<string, ReasoningSegmentTiming[]>();
  
  /** Currently open (streaming) segment by message ID */
  private openByMsg = new Map<string, ReasoningSegmentTiming>();

  constructor(private readonly clock: Clock) {}

  /**
   * Called when a reasoning part is appended.
   * Starts a new segment if none is currently open.
   */
  onReasoning(msgId: string): void {
    if (this.openByMsg.has(msgId)) return; // Already have open segment

    const segments = this.segmentsByMsg.get(msgId) ?? [];
    const seg: ReasoningSegmentTiming = {
      segmentIndex: segments.length,
      startMs: this.clock.now(),
    };
    this.openByMsg.set(msgId, seg);
    this.segmentsByMsg.set(msgId, segments);
  }

  /**
   * Called when a boundary is encountered (tool-call or text part).
   * Ends the current reasoning segment if one is open.
   */
  onBoundary(msgId: string): void {
    const open = this.openByMsg.get(msgId);
    if (!open) return; // No open segment to end

    open.endMs = this.clock.now();
    const segments = this.segmentsByMsg.get(msgId)!;
    segments.push(open);
    this.openByMsg.delete(msgId);
  }

  /**
   * Called when the message is complete.
   * Ends any open reasoning segment.
   */
  onEndOfMessage(msgId: string): void {
    this.onBoundary(msgId);
  }

  /**
   * Get elapsed time for a currently streaming segment.
   * Returns null if the segment is not streaming or doesn't exist.
   */
  getElapsedMs(msgId: string, segmentIndex: number): number | null {
    const open = this.openByMsg.get(msgId);
    if (open?.segmentIndex === segmentIndex) {
      return this.clock.now() - open.startMs;
    }
    return null;
  }

  /**
   * Get duration in seconds for a completed segment.
   * Returns null if the segment is not complete or doesn't exist.
   */
  getDurationSec(msgId: string, segmentIndex: number): number | null {
    const seg = (this.segmentsByMsg.get(msgId) ?? []).find(
      s => s.segmentIndex === segmentIndex
    );
    if (!seg?.endMs) return null;
    return (seg.endMs - seg.startMs) / 1000;
  }

  /**
   * Clear all timing data for a message.
   * Useful for cleanup when messages are deleted.
   */
  clearMessage(msgId: string): void {
    this.segmentsByMsg.delete(msgId);
    this.openByMsg.delete(msgId);
  }

  /**
   * Clear all timing data (call when switching conversations).
   * Prevents unbounded memory growth across conversation history.
   */
  clearAll(): void {
    this.segmentsByMsg.clear();
    this.openByMsg.clear();
  }

  /**
   * Get all timing data (for debugging/testing).
   */
  getState() {
    return {
      completed: new Map(this.segmentsByMsg),
      open: new Map(this.openByMsg),
    };
  }
}
