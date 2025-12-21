/**
 * Clock abstraction for testable timing without performance.now() flakiness.
 * 
 * @module clock
 */

/**
 * Clock interface for getting current time in milliseconds.
 * Allows injection of fake clocks in tests for deterministic timing.
 */
export interface Clock {
  /** Returns current time in milliseconds */
  now(): number;
}

/**
 * Production clock using performance.now().
 * Provides high-resolution monotonic timestamps.
 */
export const performanceClock: Clock = {
  now: () => performance.now(),
};
