import React, { useState } from 'react';
import ReactMarkdown from 'react-markdown';
import type { Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import { formatThinkingDuration } from '../../utils/thinkingParser';
import { useThinkingTiming } from './context/ThinkingTimingContext';
import './ThinkingBlock.css';

const cx = (...classes: Array<string | false | undefined>) =>
  classes.filter(Boolean).join(' ');

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
          <code className={cx('thinking-inline-code', className)} {...rest}>
            {children}
          </code>
        );
      }
      return (
        <pre className="thinking-code-block">
          <code className={className} {...rest}>
            {children}
          </code>
        </pre>
      );
    },
  };

  return (
    <div className={cx('thinking-block', isStreaming && 'thinking-block-streaming')}>
      <div
        className="thinking-header"
        role="button"
        tabIndex={0}
        onClick={handleToggle}
        onKeyDown={handleKeyDown}
        aria-expanded={isExpanded}
      >
        <span className={cx('thinking-chevron', isExpanded && 'thinking-chevron-expanded')}>
          ▶
        </span>
        <span className="thinking-icon">💭</span>
        <span className="thinking-label">{getLabel()}</span>
        {isStreaming && <span className="thinking-spinner" />}
      </div>
      
      <div className={cx('thinking-content', isExpanded && 'thinking-content-expanded')}>
        <div className="thinking-content-inner">
          <div className="thinking-markdown">
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
