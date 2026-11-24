/**
 * `utils/countdown`: the seconds left, rounded up and never below zero, and
 * a countdown as m:ss.
 */

import { describe, it, expect } from 'vitest';
import { clock, secondsLeft } from '../../../src/utils/countdown';

describe('a countdown', () => {
  it('reads as m:ss', () => {
    expect(clock(102)).toBe('1:42');
    expect(clock(9)).toBe('0:09');
    expect(clock(120)).toBe('2:00');
  });

  it('counts whole seconds left, rounded up and never below zero', () => {
    expect(secondsLeft(10_500, 0)).toBe(11);
    expect(secondsLeft(0, 5_000)).toBe(0);
  });
});
