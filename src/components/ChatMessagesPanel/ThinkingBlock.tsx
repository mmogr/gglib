import React, { useState } from 'react';
import ReactMarkdown from 'react-markdown';
import type { Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import { formatThinkingDuration } from '../../utils/thinkingParser';
import './ThinkingBlock.css';

const cx = (...classes: Array<string | false | undefined>) =>
  classes.filter(Boolean).join(' ');

interface ThinkingBlockProps {
  /** The thinking/reasoning content to display */
  thinking: string;
  /** Duration in seconds, or null if still streaming */
  durationSeconds: number | null;
  /** Whether thinking is still in progress (streaming) */
  isStreaming?: boolean;
  /** Whether to start expanded (default: false) */
  defaultExpanded?: boolean;
}

/**
 * Collapsible block for displaying reasoning model "thinking" content.
 * Similar to OpenWebUI's thinking display.
 */
const ThinkingBlock: React.FC<ThinkingBlockProps> = ({
  thinking,
  durationSeconds,
  isStreaming = false,
  defaultExpanded = false,
}) => {
  const [isExpanded, setIsExpanded] = useState(defaultExpanded);

  const handleToggle = () => {
    setIsExpanded((prev) => !prev);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      handleToggle();
    }
  };

  // Generate label based on state
  const getLabel = () => {
    if (isStreaming) {
      // Show live duration during streaming for better UX feedback
      if (durationSeconds != null) {
        return `Thinking… ${formatThinkingDuration(durationSeconds)}`;
      }
      return 'Thinking…';
    }
    if (durationSeconds != null) {
      return `Thought for ${formatThinkingDuration(durationSeconds)}`;
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
          <ReactMarkdown
            className="thinking-markdown"
            remarkPlugins={[remarkGfm]}
            rehypePlugins={[rehypeHighlight]}
            components={components}
          >
            {thinking || ''}
          </ReactMarkdown>
        </div>
      </div>
    </div>
  );
};

export default ThinkingBlock;
