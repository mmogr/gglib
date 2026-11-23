/**
 * `places/devices/devices`: the code in two groups of three, the away time
 * as the local HH:MM, and the QR payload uppercased into QR alphanumeric.
 */

import { describe, it, expect } from 'vitest';
import { awaySince, codeGroups, qrPayload } from '../../../src/places/devices/devices';
import { TICKET } from './devicesFixtures';

describe('the Devices place formatting', () => {
  it('the code as 3+3, anything else as it came', () => {
    expect(codeGroups('418207')).toBe('418 207');
    expect(codeGroups('12345')).toBe('12345');
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
