import { FC, useId, useRef, useState } from 'react';
import { AlertTriangle } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { ContextUsageDonut } from '../../ContextUsageDonut';
import { useClickOutside } from '../../../hooks/useClickOutside';
import { cn } from '../../../utils/cn';
import { useContextReading } from '../hooks/useContextReading';
import { contextLines, percentText, spokenReading, type ContextReading } from './contextReading';

/**
 * A reading's ring and its detail. Hovering says the sentence; a click
 * opens the detail, which Escape or a click elsewhere closes. From 70% the
 * ring is never colour alone: a warning icon and the figure stand beside it.
 *
 * The detail is a `group`, not a `dialog`: `useClickOutside` counts any
 * dialog as inside every popout, so a dialog here would keep the tools
 * popout open under it.
 */
const ReadingRing: FC<{ reading: ContextReading }> = ({ reading }) => {
  const rootRef = useRef<HTMLDivElement>(null);
  const detailId = useId();
  const [open, setOpen] = useState(false);
  useClickOutside(rootRef, () => setOpen(false), open);
  const lines = contextLines(reading);

  return (
    <div className="relative inline-flex" ref={rootRef}>
      <Button
        variant="ghost"
        size="sm"
        className="px-1.5"
        aria-label={`Context: ${spokenReading(reading)}`}
        title={lines[0]}
        aria-expanded={open}
        aria-controls={open ? detailId : undefined}
        onClick={() => setOpen(!open)}
      >
        <ContextUsageDonut compact used={reading.used} total={reading.size} size={18} strokeWidth={3} />
        {reading.severity !== 'normal' && (
          <span
            className={cn(
              'ml-1.5 inline-flex items-center gap-1 font-mono tabular-nums',
              reading.severity === 'danger' ? 'text-danger' : 'text-warning',
            )}
          >
            <Icon icon={AlertTriangle} size={12} />
            {percentText(reading)}
          </span>
        )}
      </Button>

      {open && (
        <div
          id={detailId}
          role="group"
          aria-label="Context"
          className="absolute left-0 bottom-full mb-1 z-popover w-max max-w-[240px] flex flex-col gap-xs px-[14px] py-[10px] bg-surface-elevated border border-border rounded-lg shadow-lg text-left text-xs text-text-secondary tabular-nums"
        >
          {lines.map((line, index) => (
            <p key={line} className={cn('m-0', index === 0 && 'text-text')}>
              {line}
            </p>
          ))}
        </div>
      )}
    </div>
  );
};

/**
 * The composer margin's context ring: how much of the model's context the
 * conversation has used, as its last finished reply's figures say.
 *
 * Nothing is drawn without a reading: no empty ring, no zero and no dash.
 * A detail left open goes with its reading.
 */
export const ContextRing: FC = () => {
  const reading = useContextReading();
  return reading ? <ReadingRing reading={reading} /> : null;
};
