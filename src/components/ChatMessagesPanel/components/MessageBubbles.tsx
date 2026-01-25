import React, { useContext } from 'react';
import {
  ComposerPrimitive,
  MessagePrimitive,
  ActionBarPrimitive,
  useMessage,
} from '@assistant-ui/react';
import { Bot, Copy, Pencil, Trash2, User as UserIcon } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { Button } from '../../ui/Button';
import { parseThinkingContent } from '../../../utils/thinkingParser';
import ThinkingBlock from '../ThinkingBlock';
import MarkdownMessageContent from './MarkdownMessageContent';
import { threadMessageToTranscriptMarkdown } from '../../../utils/messages';
import { MessageActionsContext } from './MessageActionsContext';
import { useThinkingTiming } from '../context/ThinkingTimingContext';
import { ToolUsageBadge } from '../../ToolUsageBadge';
import { ResearchArtifact } from '../../DeepResearch';
import type { GglibMessageCustom } from '../../../types/messages';
import type { ResearchState } from '../../../hooks/useDeepResearch/types';

const cx = (...classes: Array<string | false | undefined>) =>
  classes.filter(Boolean).join(' ');

/**
 * Extract research state from message metadata (if present).
 */
function getResearchState(message: ReturnType<typeof useMessage>): ResearchState | null {
  // Access custom metadata via the message's metadata.custom field
  const custom = (message as any)?.metadata?.custom as GglibMessageCustom | undefined;
  return custom?.researchState ?? null;
}

/**
 * Check if a message is a deep research artifact.
 */
function isDeepResearchMessage(message: ReturnType<typeof useMessage>): boolean {
  const custom = (message as any)?.metadata?.custom as GglibMessageCustom | undefined;
  return custom?.isDeepResearch === true || custom?.researchState != null;
}

/**
 * Message bubble for assistant responses.
 * Handles thinking blocks, markdown rendering, and deep research artifacts.
 */
export const AssistantMessageBubble: React.FC = () => {
  const message = useMessage();
  const timing = useThinkingTiming();
  const timestamp = new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
  }).format(message.createdAt ?? new Date());

  // Check if this is a deep research message
  const researchState = getResearchState(message);
  const isResearch = isDeepResearchMessage(message);

  // Extract and parse thinking content from message (for non-research messages)
  const rawText = threadMessageToTranscriptMarkdown(message);
  const parsed = parseThinkingContent(rawText);
  
  // Determine if this message is currently streaming
  const isStreaming = timing?.currentStreamingAssistantMessageId === message.id;
  
  // Determine if we're currently in the thinking phase (streaming with only thinking, no main content yet)
  const isCurrentlyThinking = isStreaming && !!parsed.thinking && !parsed.content.trim();

  // For deep research messages, render ResearchArtifact
  if (isResearch && researchState) {
    // Research is "running" if it's not complete and not in error state
    // Note: We don't rely on isStreaming here because deep research manages its own state
    const isResearchRunning = researchState.phase !== 'complete' && researchState.phase !== 'error';
    
    return (
      <MessagePrimitive.Root className={cx('chat-message-bubble', 'chat-assistant-message', 'chat-research-message')}>
        <div className="chat-message-meta">
          <div className="chat-message-avatar" aria-hidden>
            <Icon icon={Bot} size={18} />
          </div>
          <div>
            <div className="chat-message-author">Assistant</div>
            <div className="chat-message-timestamp">{timestamp}</div>
          </div>
        </div>
        <div className="chat-message-content">
          <ResearchArtifact
            state={researchState}
            isRunning={isResearchRunning}
            defaultExpanded={true}
          />
        </div>
        <ActionBarPrimitive.Root className="chat-message-actions">
          <ActionBarPrimitive.Copy />
        </ActionBarPrimitive.Root>
      </MessagePrimitive.Root>
    );
  }

  // Standard assistant message rendering
  return (
    <MessagePrimitive.Root className={cx('chat-message-bubble', 'chat-assistant-message')}>
      <div className="chat-message-meta">
        <div className="chat-message-avatar" aria-hidden>
          <Icon icon={Bot} size={18} />
        </div>
        <div>
          <div className="chat-message-author">Assistant</div>
          <div className="chat-message-timestamp">
            {timestamp}
            <ToolUsageBadge />
          </div>
        </div>
      </div>
      <div className="chat-message-content">
        {parsed.thinking && (
          <ThinkingBlock
            messageId={message.id}
            segmentIndex={0}
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
        <div className="chat-message-avatar" aria-hidden>
          <Icon icon={UserIcon} size={18} />
        </div>
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
          <Icon icon={Copy} size={14} />
        </ActionBarPrimitive.Copy>
        <ActionBarPrimitive.Edit className="chat-action-btn chat-edit-btn" title="Edit message" aria-label="Edit message">
          <Icon icon={Pencil} size={14} />
        </ActionBarPrimitive.Edit>
        <Button
          variant="ghost"
          size="sm"
          className="chat-action-btn chat-delete-btn"
          onClick={handleDelete}
          title="Delete message"
          aria-label="Delete message"
          iconOnly
        >
          <Icon icon={Trash2} size={14} />
        </Button>
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
        <div className="chat-message-avatar" aria-hidden>
          <Icon icon={UserIcon} size={18} />
        </div>
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
