/**
 * The launch line over the rows: the command gglib would run, in mono,
 * wrapping between its tokens (a token wider than the line breaks inside
 * it). Each argument is a button that opens a small note under the line
 * with whose choice it was and why. The text is `commandLine`'s, which the
 * CLI's recorded lines pin. Overriding a token is the Load with options
 * sheet's (I2), not this line's.
 *
 * @module inspector/local/LaunchLine
 */

import { Fragment, useRef, useState, type FC } from 'react';
import { CHOICE_WORDS } from '../../components/status/words';
import { Button } from '../../components/ui/Button';
import { useClickOutside } from '../../hooks/useClickOutside';
import type { LaunchPlan } from '../../types/generated/LaunchPlan';
import { argText } from './commandLine';

export const LaunchLine: FC<{ plan: Pick<LaunchPlan, 'program' | 'args'> }> = ({ plan }) => {
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
              className="inline h-auto max-w-full break-all rounded-sm px-0 text-left font-mono text-xs font-normal text-text-secondary"
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
        </div>
      )}
    </div>
  );
};
