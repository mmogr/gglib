import React, { useContext } from 'react';
import {
  ComposerPrimitive,
  MessagePrimitive,
  ActionBarPrimitive,
  useMessage,
} from '@assistant-ui/react';
import { parseThinkingContent } from '../../../utils/thinkingParser';
import ThinkingBlock from '../ThinkingBlock';
import MarkdownMessageContent, { extractMessageText } from './MarkdownMessageContent';
import { MessageActionsContext } from './MessageActionsContext';

const cx = (...classes: Array<string | false | undefined>) =>
  classes.filter(Boolean).join(' ');

/**
 * Message bubble for assistant responses.
 * Handles thinking blocks and markdown rendering.
 */
export const AssistantMessageBubble: React.FC = () => {
  const message = useMessage();
  const timestamp = new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
  }).format(message.createdAt ?? new Date());

  // Extract and parse thinking content from message
  const rawText = extractMessageText(message);
  const parsed = parseThinkingContent(rawText);
  const isStreaming = message.status?.type === 'running';
  
  // Determine if we're currently in the thinking phase (streaming with only thinking, no main content yet)
  const isCurrentlyThinking = isStreaming && !!parsed.thinking && !parsed.content.trim();

  return (
    <MessagePrimitive.Root className={cx('chat-message-bubble', 'chat-assistant-message')}>
      <div className="chat-message-meta">
        <div className="chat-message-avatar">🤖</div>
        <div>
          <div className="chat-message-author">Assistant</div>
          <div className="chat-message-timestamp">{timestamp}</div>
        </div>
      </div>
      <div className="chat-message-content">
        {parsed.thinking && (
          <ThinkingBlock
            thinking={parsed.thinking}
            durationSeconds={parsed.durationSeconds}
            isStreaming={isCurrentlyThinking}
          />
        )}
        {parsed.content && (
          <MarkdownMessageContent text={parsed.content} />
        )}
        {!parsed.thinking && !parsed.content && isStreaming && (
          <span className="chat-streaming-placeholder">…</span>
        )}
      </div>
      <ActionBarPrimitive.Root className="chat-message-actions">
        <ActionBarPrimitive.Copy />
      </ActionBarPrimitive.Root>
    </MessagePrimitive.Root>
  );
};

/**
 * Message bubble for user messages.
 * Includes copy, edit, and delete actions.
 */
export const UserMessageBubble: React.FC = () => {
  const message = useMessage();
  const messageActions = useContext(MessageActionsContext);
  const timestamp = new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
  }).format(message.createdAt ?? new Date());

  const handleDelete = () => {
    if (messageActions && message.id) {
      messageActions.onDeleteMessage(message.id);
    }
  };

  return (
    <MessagePrimitive.Root className={cx('chat-message-bubble', 'chat-user-message')}>
      <div className="chat-message-meta">
        <div className="chat-message-avatar">🧑‍💻</div>
        <div>
          <div className="chat-message-author">You</div>
          <div className="chat-message-timestamp">{timestamp}</div>
        </div>
      </div>
      <div className="chat-message-content">
        <MarkdownMessageContent />
      </div>
      <ActionBarPrimitive.Root className="chat-message-actions">
        <ActionBarPrimitive.Copy className="chat-action-btn" title="Copy message" aria-label="Copy message">
          📋
        </ActionBarPrimitive.Copy>
        <ActionBarPrimitive.Edit className="chat-action-btn chat-edit-btn" title="Edit message" aria-label="Edit message">
          ✏️
        </ActionBarPrimitive.Edit>
        <button
          className="chat-action-btn chat-delete-btn"
          onClick={handleDelete}
          title="Delete message"
          aria-label="Delete message"
        >
          🗑️
        </button>
      </ActionBarPrimitive.Root>
    </MessagePrimitive.Root>
  );
};

/**
 * Placeholder for system messages (not rendered).
 */
export const SystemMessageBubble: React.FC = () => null;

/**
 * Edit composer shown when user clicks Edit on their message.
 */
export const EditComposer: React.FC = () => {
  const message = useMessage();
  const timestamp = new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
  }).format(message.createdAt ?? new Date());

  return (
    <MessagePrimitive.Root className={cx('chat-message-bubble', 'chat-user-message', 'chat-edit-mode')}>
      <div className="chat-message-meta">
        <div className="chat-message-avatar">🧑‍💻</div>
        <div>
          <div className="chat-message-author">You</div>
          <div className="chat-message-timestamp">{timestamp}</div>
        </div>
      </div>
      <ComposerPrimitive.Root className="chat-edit-composer">
        <ComposerPrimitive.Input className="chat-edit-input" />
        <div className="chat-edit-actions">
          <ComposerPrimitive.Cancel className="chat-edit-cancel">
            Cancel
          </ComposerPrimitive.Cancel>
          <ComposerPrimitive.Send className="chat-edit-send">
            Save & Regenerate
          </ComposerPrimitive.Send>
        </div>
      </ComposerPrimitive.Root>
    </MessagePrimitive.Root>
  );
};
