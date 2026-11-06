/**
 * The ⧉ that puts one value on the clipboard without opening anything.
 *
 * One button, one value, and a callback once the value is there: the
 * caller says what "copied" looks like where it lives (a toast in the main
 * window, a one-second word in the tray), since this control has no toast
 * host of its own. The accessible name says what is copied, so a reader
 * hears "Copy the endpoint address", not "button".
 *
 * @module components/CopyValue
 */

import type { FC, MouseEvent } from 'react';
import { Copy } from 'lucide-react';
import { Button } from './ui/Button';
import { Icon } from './ui/Icon';

interface CopyValueProps {
  /** What goes on the clipboard, whole. */
  value: string;
  /** The button's name: "Copy the endpoint address". */
  label: string;
  /** Called after `value` reaches the clipboard. */
  onCopied?: (value: string) => void;
  className?: string;
}

export const CopyValue: FC<CopyValueProps> = ({ value, label, onCopied, className }) => {
  const copy = (event: MouseEvent) => {
    // A copy inside a row that navigates must not also navigate.
    event.stopPropagation();
    void navigator.clipboard.writeText(value).then(() => onCopied?.(value));
  };
  return (
    <Button
      variant="ghost"
      size="sm"
      iconOnly
      className={className}
      onClick={copy}
      aria-label={label}
      title={label}
    >
      <Icon icon={Copy} size={14} />
    </Button>
  );
};
