/**
 * The pairing code on offer, and how a status read ends it (ADR 0012).
 *
 * The daemon hands a code out once, in the answer to `enable` with `invite`
 * or to `invite`, and never again. So the offer is held by the registry
 * rather than by the panel that showed it: closing the popover, or opening a
 * chat in its place, must not throw away the one copy there is.
 *
 * Pure, so the rules for when an offer ends are tested without React.
 */

import type { RemoteEnableResponse, RemoteStatus } from './transport/types/remote';

/** A code on offer, with what is needed to show it and to know it ended. */
export interface PairingOffer {
  /** `<ticket>-<code>`, the one string the other machine pastes or scans. */
  pairing: string;
  /** The six digits, shown large and copied on their own. */
  code: string;
  /** The device the code will issue a key to: the id Devices lists it under. */
  device?: string;
  /** When the code stops working unused, in Unix milliseconds. */
  expiresAt: number;
  /** A status read has listed `device`'s row since the offer was made. */
  rowSeen: boolean;
}

/** How an offer ended. */
export type OfferEnd = 'paired' | 'expired' | 'withdrawn' | 'tunnel_down';

/**
 * The offer in `answer`, or `null` when it carries none.
 *
 * The daemon sends the pairing, the code and the lifetime together or not at
 * all, and an `enable` that was not asked to invite sends none of them.
 * Narrowing here is what stops that absence rendering as a pairing that has
 * already run out.
 */
export function offerFrom(answer: RemoteEnableResponse, now: number): PairingOffer | null {
  if (!answer.pairing || !answer.code || answer.expires_in_s === undefined) return null;
  return {
    pairing: answer.pairing,
    code: answer.code,
    device: answer.device,
    expiresAt: now + answer.expires_in_s * 1000,
    rowSeen: false,
  };
}

/**
 * The offer after the status moved from `prev` to `next`, and how it ended
 * if it did. The first rule that holds wins:
 *
 * 1. Paired: `paired` went from false to true, or the offered device's row
 *    reads joined. A transition, because `paired` left over from an earlier
 *    device must not take a fresh code off screen.
 * 2. Tunnel down: `enabled` went from true to false.
 * 3. Withdrawn: the offered device's row was listed and is gone, which is
 *    what forgetting it does.
 *
 * Nothing else ends it. A code the daemon no longer holds while its row is
 * still there, unjoined, is usually a gap: a redemption frees the code before
 * it sets `paired` and sends `remote_paired`, and the daemon lets an expired
 * code go a moment before this side's clock does. Ending the offer there
 * would call a pairing or an expiry a withdrawal; the event, or the
 * registry's own expiry timer, settles it instead. The cost: a code burned
 * by wrong guesses reads the same, as can one dropped by a restart whose
 * `remote_disabled` this side missed, and stays on screen until its
 * deadline, then is called expired.
 *
 * The row has to have been seen first: a read from before the answer cannot
 * list the device the answer names, and that absence is not a forget.
 */
export function settleOffer(
  offer: PairingOffer | null,
  prev: RemoteStatus | null,
  next: RemoteStatus,
): { offer: PairingOffer | null; ended?: OfferEnd } {
  if (offer === null) return { offer };
  // `?? []`: a daemon from before the roster answers status without one.
  const row = offer.device ? (next.devices ?? []).find((d) => d.id === offer.device) : undefined;
  if ((!(prev?.paired ?? false) && next.paired) || row?.joined) {
    return { offer: null, ended: 'paired' };
  }
  if ((prev?.enabled ?? false) && !next.enabled) {
    return { offer: null, ended: 'tunnel_down' };
  }
  if (offer.rowSeen && !row) {
    return { offer: null, ended: 'withdrawn' };
  }
  if (row && !offer.rowSeen) {
    return { offer: { ...offer, rowSeen: true } };
  }
  return { offer };
}
