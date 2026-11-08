/**
 * The ⧉ that puts one value on the clipboard without opening anything, and
 * the one way a copy says it worked: "Copied" beside the control for one
 * second, never a toast (a copy is not a background outcome, and the tray
 * has no toast host).
 *
 * `CopyValue` is one button, one value, and the word beside it once the
 * value is there (a copy that failed says nothing there, and is logged).
 * `useCopied`
 * and `Copied` are the same word for a copy that is not a ⧉ (the tray's
 * Copy key, a secret's Copy). The accessible name says what is copied, so a
 * reader hears "Copy the endpoint address", not "button", and the word is a
 * status, so it is heard when it appears.
 *
 * @module components/CopyValue
 */

import { useEffect, useState, type FC, type MouseEvent } from 'react';
import { Copy } from 'lucide-react';
import { appLogger } from '../services/platform';
import { cn } from '../utils/cn';
import { Button } from './ui/Button';
import { Icon } from './ui/Icon';

/** How long "Copied" stays up, in ms. */
export const COPIED_MS = 1000;

/** `[shown, show]`: `shown` turns true on `show()` and back after `COPIED_MS`; another `show()` starts it again. */
export function useCopied(): [boolean, () => void] {
  const [shownAt, setShownAt] = useState<number | null>(null);
  useEffect(() => {
    if (shownAt === null) return undefined;
    const timer = setTimeout(() => setShownAt(null), COPIED_MS);
    return () => clearTimeout(timer);
  }, [shownAt]);
  return [shownAt !== null, () => setShownAt((at) => (at ?? 0) + 1)];
}

/** "Copied", beside the control that copied, while `shown`. */
export const Copied: FC<{ shown: boolean }> = ({ shown }) =>
  shown ? (
    <span role="status" className="shrink-0 text-xs text-text-secondary">
      Copied
    </span>
  ) : null;

interface CopyValueProps {
  /** What goes on the clipboard, whole. */
  value: string;
  /** The button's name: "Copy the endpoint address". */
  label: string;
  className?: string;
}

export const CopyValue: FC<CopyValueProps> = ({ value, label, className }) => {
  const [copied, showCopied] = useCopied();
  const copy = (event: MouseEvent) => {
    // A copy inside a row that navigates must not also navigate.
    event.stopPropagation();
    navigator.clipboard.writeText(value).then(
      showCopied,
      // Nothing is said beside it: "Copied" would be untrue.
      (error: unknown) => appLogger.error('component.app', 'Failed to copy to clipboard', { error }),
    );
  };
  return (
    <span className={cn('inline-flex shrink-0 items-center gap-xs', className)}>
      <Copied shown={copied} />
      <Button variant="ghost" size="sm" iconOnly onClick={copy} aria-label={label} title={label}>
        <Icon icon={Copy} size={14} />
      </Button>
    </span>
  );
};
