import { FC, useCallback, useEffect, useState } from 'react';
import { ClipboardCopy } from 'lucide-react';
import { QRCodeSVG } from 'qrcode.react';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import { Label, Stack } from '../primitives';
import type { PairingOffer } from '../../services/remoteOffer';

/**
 * What the QR carries: the pairing string, uppercased, as the CLI's
 * `pairing_tui::qr` draws it.
 *
 * Not cosmetic: QR alphanumeric mode encodes only uppercase, and a payload in
 * it is a smaller code than the same text in byte mode, so easier
 * for a phone to read. The ticket parses case-insensitively for exactly this
 * reason, in gglib and in ggchat, and the six-digit code has no case.
 */
export function qrPayload(pairing: string): string {
  return pairing.toUpperCase();
}

interface PairingRevealProps {
  reveal: PairingOffer;
  /** Called after a value reaches the clipboard, for the caller's toast. */
  onCopied?: (what: 'pairing' | 'ticket' | 'code') => void;
}

/** Whole seconds from now until `expiresAt`, never below zero. */
function secondsLeft(expiresAt: number): number {
  return Math.max(0, Math.ceil((expiresAt - Date.now()) / 1000));
}

/**
 * The ticket and the pairing code, while the registry holds them.
 *
 * The daemon answers with a code exactly once — to `enable --invite` or to
 * `invite` — and never hands that code out again, so the registry keeps it
 * until a device pairs, the code expires or is withdrawn, and this only shows
 * it. The countdown is read from the code's deadline, not from when this
 * mounted, so a popover reopened a minute later shows the minute gone.
 */
export const PairingReveal: FC<PairingRevealProps> = ({ reveal, onCopied }) => {
  const { expiresAt } = reveal;
  const [left, setLeft] = useState(() => secondsLeft(expiresAt));

  useEffect(() => {
    setLeft(secondsLeft(expiresAt));
    const tick = window.setInterval(() => setLeft(secondsLeft(expiresAt)), 1000);
    return () => window.clearInterval(tick);
  }, [expiresAt]);

  const copy = useCallback(
    (what: 'pairing' | 'ticket' | 'code', value: string) => {
      void navigator.clipboard.writeText(value);
      onCopied?.(what);
    },
    [onCopied],
  );

  return (
    <Stack gap="sm" className="mb-md">
      {/* The QR first: a phone has no other way in, and a computer has the
          command below. Black on white whatever the theme, since an inverted
          code is one many scanners refuse; level M, the CLI's, or higher where
          that costs no size, and the four-module quiet zone its screen draws. */}
      <div className="flex flex-col items-center gap-xs">
        <QRCodeSVG
          value={qrPayload(reveal.pairing)}
          level="M"
          marginSize={4}
          size={192}
          bgColor="#ffffff"
          fgColor="#000000"
          title="Pairing QR code"
          className="rounded-base"
        />
        <span className="text-xs text-text-muted">
          Scan with ggchat: Add provider, then Scan ticket.
        </span>
      </div>
      <Label size="xs" muted>Or on another computer</Label>
      <div className="flex gap-sm items-center">
        <code className="flex-1 bg-surface-elevated p-sm rounded-base text-xs font-mono break-all">
          gglib remote join {reveal.pairing}
        </code>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => copy('pairing', reveal.pairing)}
          title="Copy pairing string"
          iconOnly
        >
          <Icon icon={ClipboardCopy} size={14} />
        </Button>
      </div>
      <div className="flex items-baseline justify-between gap-sm">
        <span className="text-xs text-text-muted">Code</span>
        <Button
          variant="ghost"
          className="font-mono tabular-nums text-2xl tracking-[0.3em] text-text"
          onClick={() => copy('code', reveal.code)}
          title="Copy code"
        >
          {reveal.code}
        </Button>
      </div>
      {reveal.device && (
        <p className="text-xs text-text-muted m-0">This code pairs {reveal.device}.</p>
      )}
      <p className="text-xs text-text-muted m-0">
        Waiting for a device… the code expires in{' '}
        <span className="font-mono tabular-nums">{left}s</span>. It works once.
      </p>
    </Stack>
  );
};
