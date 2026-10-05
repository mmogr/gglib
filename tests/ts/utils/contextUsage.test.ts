/**
 * The one percent and the one severity every usage meter is drawn from.
 */

import { describe, it, expect } from 'vitest';
import { usagePercent, usageSeverity } from '../../../src/utils/contextUsage';

describe('usagePercent', () => {
  it('rounds to the nearest whole percent', () => {
    expect(usagePercent(8200, 32768)).toBe(25);
    expect(usagePercent(8000, 32768)).toBe(24);
    expect(usagePercent(1388, 2000)).toBe(69);
    expect(usagePercent(0, 2000)).toBe(0);
  });

  it('rounds every exact half up, at the thresholds and where a float product falls short', () => {
    expect(usagePercent(139, 200)).toBe(70);
    expect(usagePercent(179, 200)).toBe(90);
    expect(usagePercent(1, 200)).toBe(1);
    // (113 / 200) * 100 is 56.49999999999999 as a float, which rounds down.
    expect(Math.round((113 / 200) * 100)).toBe(56);
    expect(usagePercent(113, 200)).toBe(57);
  });

  it('rounds just under a half down', () => {
    expect(usagePercent(1389, 2000)).toBe(69);
    expect(usagePercent(1390, 2000)).toBe(70);
  });

  it('is never over 100', () => {
    expect(usagePercent(32768, 32768)).toBe(100);
    expect(usagePercent(33000, 32768)).toBe(100);
    expect(usagePercent(1_000_000, 4096)).toBe(100);
  });
});

describe('usageSeverity', () => {
  it('is plain under 70, a warning from 70 and danger from 90', () => {
    expect([0, 69].map(usageSeverity)).toEqual(['normal', 'normal']);
    expect([70, 89].map(usageSeverity)).toEqual(['warning', 'warning']);
    expect([90, 100].map(usageSeverity)).toEqual(['danger', 'danger']);
  });
});
