import React, { useState } from 'react';
import ReactMarkdown from 'react-markdown';
import type { Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import { cn } from '../../utils/cn';
import { formatThinkingDuration } from '../../utils/thinkingParser';
import { useThinkingTiming } from './context/ThinkingTimingContext';

interface ThinkingBlockProps {
  /** Message ID for timing tracker lookup */
  messageId: string;
  /** Segment index within the message (for multiple thinking blocks) */
  segmentIndex: number;
  /** The thinking/reasoning content to display */
  thinking: string;
  /** Duration in seconds from parsed transcript, or null if not persisted yet */
  durationSeconds: number | null;
  /** Whether thinking is still in progress (streaming) */
  isStreaming?: boolean;
  /** Whether to start expanded (default: false) */
  defaultExpanded?: boolean;
}

/**
 * Collapsible block for displaying reasoning model "thinking" content.
 * Shows live timer during streaming, final duration after completion.
 * Supports fallback to timing tracker when transcript lacks duration attrs.
 */
const ThinkingBlock: React.FC<ThinkingBlockProps> = ({
  messageId,
  segmentIndex,
  thinking,
  durationSeconds,
  isStreaming = false,
  defaultExpanded = false,
}) => {
  const [isExpanded, setIsExpanded] = useState(defaultExpanded);
  const timing = useThinkingTiming();

  const handleToggle = () => {
    setIsExpanded((prev) => !prev);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      handleToggle();
    }
  };

  // Compute display duration: live elapsed time or final duration
  const tracker = timing?.timingTracker ?? null;
  const tick = timing?.tick ?? 0; // Reference tick to trigger re-renders
  
  // Live elapsed time (only during streaming)
  const liveElapsedMs =
    isStreaming && durationSeconds == null
      ? (tracker?.getElapsedMs(messageId, segmentIndex) ?? null)
      : null;
  
  // Final duration (from transcript or tracker fallback)
  const finalDurationSec =
    durationSeconds ??
    (!isStreaming ? (tracker?.getDurationSec(messageId, segmentIndex) ?? null) : null);
  
  // Reference tick to ensure re-renders during streaming
  void tick;
  
  // Display seconds (live or final)
  const displaySeconds =
    finalDurationSec ?? (liveElapsedMs != null ? liveElapsedMs / 1000 : null);

  // Generate label based on state
  const getLabel = () => {
    if (isStreaming) {
      if (displaySeconds != null) {
        return `Thinking… ${formatThinkingDuration(displaySeconds)}`;
      }
      return 'Thinking…';
    }
    if (displaySeconds != null) {
      return `Thought for ${formatThinkingDuration(displaySeconds)}`;
    }
    return 'Thinking';
  };

  // Markdown components for thinking content (simplified)
  const components: Partial<Components> = {
    code(props) {
      const { inline, className, children, ...rest } = props as {
        inline?: boolean;
        className?: string;
        children?: React.ReactNode;
      };
      if (inline) {
        return (
          <code className={cn('py-[0.125rem] px-[0.375rem] text-[0.75rem] font-mono bg-[#0d1117] rounded-[4px]', className)} {...rest}>
            {children}
          </code>
        );
      }
      return (
        <pre className="my-[0.5rem] p-[0.625rem] text-[0.75rem] font-mono bg-[#0d1117] rounded-[6px] overflow-x-auto [&_code]:bg-transparent [&_code]:p-0">
          <code className={className} {...rest}>
            {children}
          </code>
        </pre>
      );
    },
  };

  return (
    <div className={cn('mb-[0.75rem] border border-[#30363d] rounded-[8px] bg-[#161b22] overflow-hidden', isStreaming && 'border-[#388bfd33]')}>
      <div
        className="flex items-center gap-[0.5rem] py-[0.625rem] px-[0.875rem] cursor-pointer select-none transition-colors duration-150 hover:bg-[#21262d] focus-visible:outline-2 focus-visible:outline-[#58a6ff] focus-visible:outline-offset-[-2px]"
        role="button"
        tabIndex={0}
        onClick={handleToggle}
        onKeyDown={handleKeyDown}
        aria-expanded={isExpanded}
      >
        <span className={cn('text-[0.625rem] text-[#8b949e] transition-transform duration-200 shrink-0', isExpanded && 'rotate-90')}>
          ▶
        </span>
        <span className="text-[1rem] shrink-0">💭</span>
        <span className="text-[0.8125rem] font-medium text-[#8b949e]">{getLabel()}</span>
        {isStreaming && <span className="w-[12px] h-[12px] border-2 border-[#8b949e] border-t-transparent rounded-full animate-thinking-spin ml-auto shrink-0" />}
      </div>
      
      <div className={cn('max-h-0 overflow-hidden transition-[max-height] duration-[0.25s] ease-out', isExpanded && 'max-h-[500px] overflow-y-auto scrollbar-thin')}>
        <div className="px-[0.875rem] pb-[0.75rem] border-t border-[#30363d]">
          <div className="text-[0.8125rem] leading-[1.5] text-[#8b949e] [&_p]:my-[0.5rem] [&_p:first-child]:mt-[0.75rem] [&_p:last-child]:mb-0 [&_ul]:my-[0.5rem] [&_ul]:pl-[1.25rem] [&_ol]:my-[0.5rem] [&_ol]:pl-[1.25rem] [&_li]:my-[0.25rem]">
            <ReactMarkdown
              remarkPlugins={[remarkGfm]}
              rehypePlugins={[rehypeHighlight]}
              components={components}
            >
              {thinking || ''}
            </ReactMarkdown>
          </div>
        </div>
      </div>
    </div>
  );
};

export default ThinkingBlock;
