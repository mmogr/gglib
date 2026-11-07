/**
 * The Devices place's pure formatting: how a code, its time left and an
 * away time are written. No fact is made here; each is the daemon's
 * (`RemoteStatus`, the offer the registry holds) set out for reading.
 *
 * @module places/devices/devices
 */

/** Whole seconds from `now` until `expiresAt`, never below zero. */
export function secondsLeft(expiresAt: number, now: number = Date.now()): number {
  return Math.max(0, Math.ceil((expiresAt - now) / 1000));
}

/** Seconds as a countdown reads them: `1:42`, `0:09`. */
export function clock(seconds: number): string {
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

/** The six digits as two groups of three, for reading aloud: `418 207`. */
export function codeGroups(code: string): string {
  return code.length === 6 ? `${code.slice(0, 3)} ${code.slice(3)}` : code;
}

/** The local time `awayForS` seconds before `now`, as `14:02`. */
export function awaySince(awayForS: number, now: number = Date.now()): string {
  const at = new Date(now - awayForS * 1000);
  return `${String(at.getHours()).padStart(2, '0')}:${String(at.getMinutes()).padStart(2, '0')}`;
}

/**
 * What the QR carries: the pairing string, uppercased, as the CLI's
 * `pairing_tui::qr` draws it.
 *
 * Not cosmetic: QR alphanumeric mode encodes only uppercase, and a payload in
 * it is a smaller code than the same text in byte mode, so easier for a phone
 * to read. The ticket parses case-insensitively for exactly this reason, in
 * gglib and in ggchat, and the six-digit code has no case.
 */
export function qrPayload(pairing: string): string {
  return pairing.toUpperCase();
}
