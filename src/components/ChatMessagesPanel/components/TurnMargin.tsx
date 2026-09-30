import { FC } from 'react';
import { Readout } from '../../primitives';
import { Button } from '../../ui/Button';
import { PromptProgressBar } from '../../PromptProgressBar';
import type { PromptReading } from '../../../types/messages';
import { formatCount } from '../../../utils/format';
import { madeLines, turnTime, type ArrivingPhase, type ReplyFacts } from './turnFigures';

/**
 * The margin's head: who wrote the turn, then when and, for a reply, the
 * model's quantisation, each when known. A reply to a paired device's turn
 * names the device.
 */
export const TurnWho: FC<{ name: string; at?: Date; quantization?: string; device?: string }> = ({
  name,
  at,
  quantization,
  device,
}) => (
  <>
    <span className="text-sm font-semibold text-text-secondary">{name}</span>
    {device && <span>for {device}</span>}
    {(at || quantization) && (
      <span className="font-mono tabular-nums">
        {at && <time dateTime={at.toISOString()}>{turnTime(at)}</time>}
        {at && quantization && ' · '}
        {quantization}
      </span>
    )}
  </>
);

interface ReplyMadeProps {
  facts: ReplyFacts;
  /** The id of the detail the button shows; no button when absent. */
  detailId?: string;
  detailOpen: boolean;
  onToggleDetail: () => void;
}

/** The margin's foot for a reply that arrived: its figures, then the detail. */
export const ReplyMade: FC<ReplyMadeProps> = ({ facts, detailId, detailOpen, onToggleDetail }) => {
  const lines = madeLines(facts);
  if (lines.length === 0 && !detailId) return null;
  return (
    <>
      {lines.length > 0 && (
        <ul className="m-0 p-0 list-none flex flex-row flex-wrap gap-x-md gap-y-[2px] font-mono tabular-nums @min-[40rem]:flex-col @min-[40rem]:items-end">
          {lines.map((line) => (
            <li key={line} className={line === 'unfinished' ? 'text-warning' : undefined}>
              {line}
            </li>
          ))}
        </ul>
      )}
      {detailId && (
        <Button
          variant="link"
          size="sm"
          className="h-auto p-0 text-xs text-text-secondary underline underline-offset-[3px] hover:text-text"
          aria-expanded={detailOpen}
          aria-controls={detailId}
          onClick={onToggleDetail}
        >
          How this was made
        </Button>
      )}
    </>
  );
};

/** The margin's foot for a reply still arriving: what it is doing, and how far. */
export const ReplyArriving: FC<{ phase: ArrivingPhase; prompt?: PromptReading }> = ({ phase, prompt }) => (
  <>
    <span role="status" className="flex items-center gap-sm text-sm text-text-secondary">
      <span aria-hidden className="w-[6px] h-[6px] rounded-full bg-success animate-research-pulse" />
      {phase}
    </span>
    {prompt && phase === 'Reading the prompt' && (
      <Readout
        label="Prompt read"
        value={`${formatCount(prompt.processed)} of ${formatCount(prompt.total)}`}
        unit="tok"
        size="sm"
        align="end"
        trend={<PromptProgressBar processed={prompt.processed} total={prompt.total} className="w-[120px]" />}
      />
    )}
    {prompt && phase !== 'Reading the prompt' && (
      <span className="font-mono tabular-nums">{formatCount(prompt.total)} tok read</span>
    )}
    {prompt && <span className="font-mono tabular-nums">{formatCount(prompt.cached)} from cache</span>}
  </>
);
