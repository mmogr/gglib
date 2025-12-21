import React from 'react';
import ReactMarkdown from 'react-markdown';
import type { Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import { useMessage } from '@assistant-ui/react';
import { threadMessageToTranscriptMarkdown } from '../../../utils/messages';

const cx = (...classes: Array<string | false | undefined>) =>
  classes.filter(Boolean).join(' ');

interface MarkdownMessageContentProps {
  /** Optional text override. If not provided, extracts from current message context. */
  text?: string;
}

/**
 * Renders message content as markdown with syntax highlighting and GFM support.
 * Uses the message context from @assistant-ui/react if text prop is not provided.
 */
const MarkdownMessageContent: React.FC<MarkdownMessageContentProps> = ({ text: propText }) => {
  const message = useMessage();
  const text = propText ?? threadMessageToTranscriptMarkdown(message);

  const components: Partial<Components> = {
    table: ({ children }) => (
      <div className="chat-table-wrapper">
        <table>{children}</table>
      </div>
    ),
    code(props) {
      const { inline, className, children, ...rest } = props as {
        inline?: boolean;
        className?: string;
        children?: React.ReactNode;
      };
      if (inline) {
        return (
          <code className={cx('chat-inline-code', className)} {...rest}>
            {children}
          </code>
        );
      }
      return (
        <pre className="chat-code-block">
          <code className={className} {...rest}>
            {children}
          </code>
        </pre>
      );
    },
  };

  return (
    <ReactMarkdown
      className="chat-markdown-body"
      remarkPlugins={[remarkGfm]}
      rehypePlugins={[rehypeHighlight]}
      components={components}
    >
      {text || ''}
    </ReactMarkdown>
  );
};

export default MarkdownMessageContent;
