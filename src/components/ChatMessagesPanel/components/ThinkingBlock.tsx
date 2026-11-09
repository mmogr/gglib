import React, { useId, useState, type ReactNode } from 'react';
import { ChevronRight } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import type { Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import { cn } from '../../../utils/cn';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { useThinkingTiming } from '../context/ThinkingTimingContext';
import { foldSummary } from '../../../readings/turnFigures';

/** One message of the reply the fold is over, and its thinking time once saved. */
export interface FoldSegment {
  /** The message's id, for the timing tracker. */
  messageId: string;
  /** Its thinking time from its saved row, or null if not saved yet. */
  durationSeconds: number | null;
}

interface ThinkingBlockProps {
  /** The messages of the reply, one per round of tool calls, in order. */
  segments: readonly FoldSegment[];
  /** The reasoning text; empty for a reply that called tools without it. */
  thinking: string;
  /** Whether thinking is still in progress (streaming) */
  isStreaming?: boolean;
  /** Open until the person chooses: while the reply arrives. */
  defaultExpanded?: boolean;
  /** How many tools the reply called, for the summary. */
  toolCalls?: number;
  /** The rows of those calls, under the reasoning. */
  children?: ReactNode;
}

// react-markdown draws a block as <pre><code>, and says nothing else of
// which code is inline: the <pre> is the block, and <code> alone is inline.
const COMPONENTS: Partial<Components> = {
  pre: ({ children }) => (
    <pre className="my-sm p-sm text-xs font-mono bg-surface rounded-base overflow-x-auto [&_code]:bg-transparent! [&_code]:p-0!">
      {children}
    </pre>
  ),
  code: ({ className, children, node: _node, ...rest }) => (
    <code className={cn('py-[1px] px-[5px] text-xs font-mono bg-surface rounded-sm', className)} {...rest}>
      {children}
    </code>
  ),
};

/**
 * A reply's fold: one 12 px line, `▸ Thought for 3 s · 2 tool calls`,
 * that opens in place onto the reasoning and one row per tool call. Its
 * time is the sum of each message's: the saved row's, else the timing
 * tracker's, and while the reply thinks the live elapsed time of the one
 * thinking ("Thinking… 4.2 s"). It is open while the reply arrives and
 * closed after, unless the person has chosen.
 */
const ThinkingBlock: React.FC<ThinkingBlockProps> = ({
  segments,
  thinking,
  isStreaming = false,
  defaultExpanded = false,
  toolCalls = 0,
  children,
}) => {
  const [choice, setChoice] = useState<boolean | null>(null);
  const expanded = choice ?? defaultExpanded;
  const bodyId = useId();
  const timing = useThinkingTiming();

  // Each message's time: the transcript's, else the tracker's once its
  // thinking ended, else, while the reply thinks, how long it has so far.
  // The tick is read so that every tick draws the seconds again.
  const tracker = timing?.timingTracker ?? null;
  void timing?.tick;
  let seconds: number | null = null;
  for (const { messageId, durationSeconds } of segments) {
    const live = isStreaming ? (tracker?.getElapsedMs(messageId, 0) ?? null) : null;
    const own = durationSeconds ?? tracker?.getDurationSec(messageId, 0) ?? (live != null ? live / 1000 : null);
    if (own != null) seconds = (seconds ?? 0) + own;
  }

  const summary = foldSummary({ thinking: !!thinking, live: isStreaming, seconds, toolCalls });

  return (
    <div className="flex flex-col">
      <Button
        variant="ghost"
        size="sm"
        className="self-start h-auto gap-xs px-0 py-[2px] text-xs font-normal text-text-muted tabular-nums hover:bg-transparent hover:text-text-secondary"
        aria-expanded={expanded}
        aria-controls={bodyId}
        leftIcon={<Icon icon={ChevronRight} size={12} className={cn('transition-transform duration-150', expanded && 'rotate-90')} />}
        onClick={() => setChoice(!expanded)}
      >
        {summary}
      </Button>
      <div id={bodyId} hidden={!expanded} className="mt-xs pl-base flex flex-col gap-sm">
        {thinking && (
          <div className="text-sm leading-[1.5] text-text-muted [&_p]:my-sm [&_p:first-child]:mt-0 [&_p:last-child]:mb-0 [&_ul]:my-sm [&_ul]:pl-lg [&_ol]:my-sm [&_ol]:pl-lg">
            <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeHighlight]} components={COMPONENTS}>
              {thinking}
            </ReactMarkdown>
          </div>
        )}
        {children}
      </div>
    </div>
  );
};

export default ThinkingBlock;
