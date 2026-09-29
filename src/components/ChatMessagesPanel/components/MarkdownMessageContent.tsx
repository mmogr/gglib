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
 * Inner component that reads text from the assistant-ui message context.
 * Must only be rendered inside a MessageProvider.
 */
const ContextAwareMarkdownContent: React.FC = () => {
  const message = useMessage();
  const text = threadMessageToTranscriptMarkdown(message);
  return <MarkdownRenderer text={text} />;
};

/**
 * Renders message content as markdown with syntax highlighting and GFM support.
 *
 * When `text` is provided, renders standalone (no message context needed).
 * When `text` is omitted, reads from the @assistant-ui/react message context.
 */
const MarkdownMessageContent: React.FC<MarkdownMessageContentProps> = ({ text }) => {
  if (text != null) return <MarkdownRenderer text={text} />;
  return <ContextAwareMarkdownContent />;
};

/** Pure markdown renderer — no hooks, no context dependency. */
const MarkdownRenderer: React.FC<{ text: string }> = ({ text }) => {

  const components: Partial<Components> = {
    table: ({ children }) => (
      <div className="overflow-x-auto my-md [&_table]:border-collapse [&_table]:w-full [&_th]:border-b [&_th]:border-border [&_th]:py-sm [&_th]:pr-md [&_th]:text-left [&_th]:text-sm [&_th]:font-semibold [&_th]:text-text-secondary [&_td]:border-b [&_td]:border-border-light [&_td]:py-sm [&_td]:pr-md [&_td]:text-left">
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
          <code className={cn('bg-surface-elevated py-[1px] px-[5px] rounded-sm font-mono text-[0.9em]', className)} {...rest}>
            {children}
          </code>
        );
      }
      return (
        <pre className="bg-surface rounded-md py-md px-base overflow-x-auto my-md [&_code]:font-mono [&_code]:text-sm">
          <code className={className} {...rest}>
            {children}
          </code>
        </pre>
      );
    },
  };

  return (
    <div className="[&_p]:m-0 [&_p]:mb-md [&_p:last-child]:mb-0 [&_ul]:my-sm [&_ul]:pl-lg [&_ol]:my-sm [&_ol]:pl-lg">
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
