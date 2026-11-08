/**
 * The pairing sheet: the code on offer, grown from Invite under This Mac.
 *
 * The daemon hands a code out once, in the answer to `enable` with `invite`
 * or to `invite`, so the registry holds it (`remoteOffer.ts`, DV-1) and this
 * only reads it: the sheet is open whenever an offer is, so a page left and
 * come back to mid-code shows it again, still counting down to the code's
 * own deadline. When the offer ends here the sheet says how: "<label>
 * joined." and then closes, or "Expired." with New code. Endings from before
 * the sheet was drawn are not its news.
 *
 * @module places/devices/PairingSheet
 */

import { FC, useEffect, useRef, useState } from 'react';
import { QRCodeSVG } from 'qrcode.react';
import { CopyValue } from '../../components/CopyValue';
import { StatusMark } from '../../components/status/StatusMark';
import { Button } from '../../components/ui/Button';
import { useRemoteState } from '../../services/remoteRegistry';
import type { PairingOffer } from '../../services/remoteOffer';
import { clock, codeGroups, qrPayload } from './devices';
import { useSecondsLeft } from './useSecondsLeft';

/** How long "<label> joined." stays before the sheet closes. */
const JOINED_MS = 2_500;

const SHEET = 'mx-md mt-sm flex animate-modal-slide-in flex-col gap-sm rounded-base bg-surface-elevated p-md';

interface PairingSheetProps {
  /** Offer a fresh code, after one expired. */
  onNewCode: () => void;
  /** A fresh code is being offered. */
  offering: boolean;
}

export const PairingSheet: FC<PairingSheetProps> = ({ onNewCode, offering }) => {
  const { status, offer, ended } = useRemoteState();
  // Endings numbered at or below these are not this sheet's to say: those
  // from before it was drawn, and the one it has closed.
  const drawn = useRef(ended?.seq ?? 0);
  const [closed, setClosed] = useState(0);
  const ending = ended !== null && ended.seq > drawn.current && ended.seq > closed ? ended : null;

  useEffect(() => {
    if (ending?.reason !== 'paired') return;
    const close = window.setTimeout(() => setClosed(ending.seq), JOINED_MS);
    return () => window.clearTimeout(close);
  }, [ending]);

  if (offer !== null) return <LiveCode offer={offer} />;
  if (ending === null) return null;
  if (ending.reason === 'paired') {
    const row = (status?.devices ?? []).find((d) => d.id === ending.device);
    const name = row?.label ?? ending.device;
    return (
      <div className={SHEET} style={{ transformOrigin: 'top right' }}>
        <p className="m-0 text-sm text-text">{name ? `${name} joined.` : 'A device joined.'}</p>
      </div>
    );
  }
  // A fresh code needs the tunnel up; with it down the sheet has nothing to offer.
  if ((ending.reason === 'expired' || ending.reason === 'lapsed') && status?.enabled) {
    return (
      <div className={SHEET} style={{ transformOrigin: 'top right' }}>
        <div className="flex items-center gap-sm">
          <p className="m-0 flex-1 text-sm text-text">Expired.</p>
          <Button variant="secondary" size="sm" onClick={onNewCode} disabled={offering}>
            {offering ? <StatusMark kind="changing" word="Offering…" /> : 'New code'}
          </Button>
        </div>
      </div>
    );
  }
  return null;
};

/** The code while it is live: the QR, the code, the join string, the time left. */
const LiveCode: FC<{ offer: PairingOffer }> = ({ offer }) => {
  const left = useSecondsLeft(offer.expiresAt) ?? 0;

  return (
    <div className={SHEET} style={{ transformOrigin: 'top right' }} aria-label="Pairing code" role="region">
      <div className="flex flex-wrap items-start gap-lg">
        {/* The QR first: a phone has no other way in. Black on white whatever
            the theme, since many scanners refuse an inverted code; level M, the
            CLI's, and the four-module quiet zone its screen draws. */}
        <figure className="m-0 flex flex-col items-center gap-xs">
          <QRCodeSVG
            value={qrPayload(offer.pairing)}
            level="M"
            marginSize={4}
            size={168}
            bgColor="#ffffff"
            fgColor="#000000"
            title="Pairing QR code"
            className="rounded-base"
          />
          <figcaption className="text-xs text-text-secondary">Scan with ggchat</figcaption>
        </figure>
        <div className="flex min-w-0 flex-1 flex-col gap-sm">
          <div className="flex items-center gap-sm">
            <span className="text-xs text-text-secondary">Code</span>
            <span className="text-[22px] tabular-nums text-text">{codeGroups(offer.code)}</span>
            <CopyValue value={offer.code} label="Copy the code" />
          </div>
          <div className="flex items-start gap-sm">
            <span className="shrink-0 text-xs text-text-secondary">On a computer:</span>
            <code className="min-w-0 flex-1 break-all font-mono text-xs text-text">gglib remote join {offer.pairing}</code>
            <CopyValue value={offer.pairing} label="Copy the pairing string" />
          </div>
          {offer.device && <span className="text-xs text-text-secondary">This code pairs {offer.device}.</span>}
          <span className="text-xs text-text-secondary">
            Expires in <span className="tabular-nums">{clock(left)}</span>
          </span>
        </div>
      </div>
    </div>
  );
};
