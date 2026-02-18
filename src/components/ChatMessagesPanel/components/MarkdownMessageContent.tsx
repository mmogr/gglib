import React from 'react';
import ReactMarkdown from 'react-markdown';
import type { Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import { useMessage } from '@assistant-ui/react';
import { threadMessageToTranscriptMarkdown } from '../../../utils/messages';

import { cn } from '../../../utils/cn';

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
      <div className="overflow-x-auto my-sm [&_table]:border-collapse [&_table]:w-full [&_th]:border [&_th]:border-border [&_th]:py-xs [&_th]:px-sm [&_th]:text-left [&_td]:border [&_td]:border-border [&_td]:py-xs [&_td]:px-sm [&_td]:text-left">
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
          <code className={cn('bg-background py-[2px] px-[6px] rounded-sm font-mono text-[0.9em]', className)} {...rest}>
            {children}
          </code>
        );
      }
      return (
        <pre className="bg-background rounded-sm p-md overflow-x-auto my-sm [&_code]:font-mono [&_code]:text-sm">
          <code className={className} {...rest}>
            {children}
          </code>
        </pre>
      );
    },
  };

  return (
    <div className="text-sm [&_p]:m-0 [&_p]:mb-sm [&_p:last-child]:mb-0 [&_ul]:my-sm [&_ul]:pl-lg [&_ol]:my-sm [&_ol]:pl-lg">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeHighlight]}
        components={components}
      >
        {text || ''}
      </ReactMarkdown>
    </div>
  );
};

export default MarkdownMessageContent;
