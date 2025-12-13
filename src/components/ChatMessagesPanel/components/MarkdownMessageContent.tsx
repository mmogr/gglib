import React from 'react';
import ReactMarkdown from 'react-markdown';
import type { Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import { useMessage } from '@assistant-ui/react';
import type { ThreadMessage } from '@assistant-ui/react';

const cx = (...classes: Array<string | false | undefined>) =>
  classes.filter(Boolean).join(' ');

/**
 * Extract plain text from a ThreadMessage's content parts.
 */
export const extractMessageText = (message: ThreadMessage): string => {
  return message.content
    .map((part) => {
      if (typeof part === 'string') {
        return part;
      }
      if ('text' in part && part.text) {
        return part.text;
      }
      if (part.type === 'tool-call') {
        return `${part.toolName}(${part.argsText ?? ''})`;
      }
      return '';
    })
    .filter(Boolean)
    .join('\n\n');
};

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
  const text = propText ?? extractMessageText(message);

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
