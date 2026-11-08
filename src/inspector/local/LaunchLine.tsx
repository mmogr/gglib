/**
 * The launch line over the rows: the command gglib would run, in mono,
 * wrapping between its tokens (a token wider than the line breaks inside
 * it). Each argument is a button that opens a small note under the line
 * with whose choice it was and why; for a token whose decision the Load
 * with options sheet holds (`SHEET_FIELDS`), the note offers Change for
 * this model…, which opens the sheet on that field. A token the person
 * chose (source `you`) is underlined. The text is `commandLine`'s, which
 * the CLI's recorded lines pin. A per-token override field and `+ flag`
 * wait for a Rust surface that sets any flag.
 *
 * @module inspector/local/LaunchLine
 */

import { Fragment, useRef, useState, type FC } from 'react';
import { CHOICE_WORDS } from '../../components/status/words';
import { Button } from '../../components/ui/Button';
import { useClickOutside } from '../../hooks/useClickOutside';
import type { LaunchPlan } from '../../types/generated/LaunchPlan';
import { SHEET_FIELDS, type SheetField } from '../loadOptions';
import { cn } from '../../utils/cn';
import { argText } from './commandLine';

interface LaunchLineProps {
  plan: Pick<LaunchPlan, 'program' | 'args'>;
  /** Change for this model…: open the sheet on a token's field. */
  onChange?: (field: SheetField) => void;
}

export const LaunchLine: FC<LaunchLineProps> = ({ plan, onChange }) => {
  const [open, setOpen] = useState<number | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  useClickOutside(ref, () => setOpen(null), open !== null);
  const picked = open === null ? null : plan.args[open];

  return (
    <div ref={ref} className="flex flex-col gap-xs" onKeyDown={(e) => e.key === 'Escape' && setOpen(null)}>
      <p aria-label="Launch line" className="m-0 font-mono text-xs leading-relaxed text-text-secondary">
        <span>{plan.program}</span>
        {plan.args.map((arg, i) => (
          <Fragment key={i}>
            {' '}
            <Button
              variant="ghost"
              size="sm"
              aria-expanded={open === i}
              onClick={() => setOpen(open === i ? null : i)}
              className={cn(
                'inline h-auto max-w-full break-all rounded-sm px-0 text-left font-mono text-xs font-normal text-text-secondary',
                arg.source === 'you' && 'underline underline-offset-2',
              )}
            >
              {argText(arg)}
            </Button>
          </Fragment>
        ))}
      </p>
      {picked && (
        <div role="note" aria-label={picked.flag} className="max-w-[320px] rounded-base bg-surface-elevated px-sm py-xs text-xs">
          <span className="text-text">{picked.reason}</span>
          <span className="block text-text-secondary">{`chosen by ${CHOICE_WORDS[picked.source]}`}</span>
          {onChange && SHEET_FIELDS.has(picked.decision) && (
            <Button
              variant="link"
              size="sm"
              className="text-xs font-normal"
              onClick={() => onChange(picked.decision as SheetField)}
            >
              Change for this model…
            </Button>
          )}
        </div>
      )}
    </div>
  );
};
