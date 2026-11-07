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

interface ThinkingBlockProps {
  /** Message ID for timing tracker lookup */
  messageId: string;
  /** Segment index within the message (for multiple thinking blocks) */
  segmentIndex: number;
  /** The reasoning text; empty for a reply that called tools without it. */
  thinking: string;
  /** Duration in seconds from parsed transcript, or null if not persisted yet */
  durationSeconds: number | null;
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
 * A reply's fold: one 12 px line, `▸ Thought for 3.0 s · 2 tool calls`,
 * that opens in place onto the reasoning and one row per tool call. While
 * the reply thinks the line is live ("Thinking… 4.2 s", from the timing
 * tracker until the transcript has a duration). It is open while the reply
 * arrives and closed after, unless the person has chosen.
 */
const ThinkingBlock: React.FC<ThinkingBlockProps> = ({
  messageId,
  segmentIndex,
  thinking,
  durationSeconds,
  isStreaming = false,
  defaultExpanded = false,
  toolCalls = 0,
  children,
}) => {
  const [choice, setChoice] = useState<boolean | null>(null);
  const expanded = choice ?? defaultExpanded;
  const bodyId = useId();
  const timing = useThinkingTiming();

  // Live elapsed time while thinking; the transcript's, or the tracker's, after.
  // The tick is read so that every tick draws the seconds again.
  const tracker = timing?.timingTracker ?? null;
  void timing?.tick;
  const liveElapsedMs =
    isStreaming && durationSeconds == null ? (tracker?.getElapsedMs(messageId, segmentIndex) ?? null) : null;
  const finalSeconds =
    durationSeconds ?? (!isStreaming ? (tracker?.getDurationSec(messageId, segmentIndex) ?? null) : null);
  const seconds = finalSeconds ?? (liveElapsedMs != null ? liveElapsedMs / 1000 : null);

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
