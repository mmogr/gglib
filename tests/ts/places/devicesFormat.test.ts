/**
 * `places/devices/devices`: the countdown as m:ss, the code in two groups of
 * three, the seconds left never below zero, the away time as the local
 * HH:MM, and the QR payload uppercased into QR alphanumeric.
 */

import { describe, it, expect } from 'vitest';
import { awaySince, clock, codeGroups, qrPayload, secondsLeft } from '../../../src/places/devices/devices';
import { TICKET } from './devicesFixtures';

describe('the Devices place formatting', () => {
  it('a countdown as m:ss', () => {
    expect(clock(102)).toBe('1:42');
    expect(clock(9)).toBe('0:09');
    expect(clock(120)).toBe('2:00');
  });

  it('the code as 3+3, anything else as it came', () => {
    expect(codeGroups('418207')).toBe('418 207');
    expect(codeGroups('12345')).toBe('12345');
  });

  it('seconds left, rounded up and never below zero', () => {
    expect(secondsLeft(10_500, 0)).toBe(11);
    expect(secondsLeft(0, 5_000)).toBe(0);
  });

  it('the local time the machine went away', () => {
    const now = new Date(2026, 10, 7, 9, 5, 30).getTime();
    expect(awaySince(65, now)).toBe('09:04');
  });

  it('the QR payload is the pairing string uppercased, all of it QR alphanumeric', () => {
    const payload = qrPayload(`${TICKET}-483920`);
    expect(payload).toBe(`${TICKET.toUpperCase()}-483920`);
    expect(payload).toMatch(/^[A-Z0-9 $%*+./:-]+$/);
  });
});
