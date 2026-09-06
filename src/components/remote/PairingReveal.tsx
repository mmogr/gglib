import { FC, useCallback, useEffect, useState } from 'react';
import { ClipboardCopy } from 'lucide-react';
import { Icon } from '../ui/Icon';
import { Button } from '../ui/Button';
import { Label, Stack } from '../primitives';
import type { RemoteEnableResponse } from '../../services/transport/types/remote';

interface PairingRevealProps {
  reveal: RemoteEnableResponse;
  /** Fires once when the code's lifetime runs out; the caller drops the reveal. */
  onExpired: () => void;
  /** Called after a value reaches the clipboard, for the caller's toast. */
  onCopied?: (what: 'pairing' | 'ticket' | 'code') => void;
}

/**
 * The ticket and the pairing code, shown once.
 *
 * `enable` answers with these exactly once and the daemon never hands them
 * out again, so this is the whole window in which a person can get them to
 * the other machine. The countdown is the code's: when it reaches zero the
 * reveal is gone, the way the CLI's pairing screen leaves the terminal, and
 * the caller also drops it the moment a device pairs.
 */
export const PairingReveal: FC<PairingRevealProps> = ({ reveal, onExpired, onCopied }) => {
  const [left, setLeft] = useState(reveal.expires_in_s);

  useEffect(() => {
    setLeft(reveal.expires_in_s);
    const started = Date.now();
    const tick = window.setInterval(() => {
      const elapsed = Math.floor((Date.now() - started) / 1000);
      const remaining = Math.max(0, reveal.expires_in_s - elapsed);
      setLeft(remaining);
      if (remaining === 0) {
        window.clearInterval(tick);
        onExpired();
      }
    }, 1000);
    return () => window.clearInterval(tick);
  }, [reveal, onExpired]);

  const copy = useCallback(
    (what: 'pairing' | 'ticket' | 'code', value: string) => {
      void navigator.clipboard.writeText(value);
      onCopied?.(what);
    },
    [onCopied],
  );

  return (
    <Stack gap="sm" className="mb-md">
      <Label size="xs" muted>On the other machine</Label>
      <div className="flex gap-sm items-center">
        <code className="flex-1 bg-surface-elevated p-sm rounded-base text-xs font-mono break-all">
          gglib remote connect {reveal.pairing}
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
      <p className="text-xs text-text-muted m-0">
        Waiting for a device… the code expires in{' '}
        <span className="font-mono tabular-nums">{left}s</span>. It works once.
      </p>
    </Stack>
  );
};
