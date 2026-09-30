import React, { useContext, useId, useState } from 'react';
import {
  ComposerPrimitive,
  MessagePrimitive,
  ActionBarPrimitive,
  useMessage,
} from '@assistant-ui/react';
import { Copy, Pencil, RefreshCw, Trash2 } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { Button } from '../../ui/Button';
import ThinkingBlock from './ThinkingBlock';
import MarkdownMessageContent from './MarkdownMessageContent';
import { MessageActionsContext } from './MessageActionsContext';
import { TurnRow } from './TurnRow';
import { ReplyArriving, ReplyMade, TurnWho } from './TurnMargin';
import { arrivingPhase, replyFacts, replyName } from './turnFigures';
import { useThinkingTiming } from '../context/ThinkingTimingContext';
import { ToolUsageBadge } from '../../ToolUsageBadge';
import { ToolExecutionProgress } from '../../ToolExecutionProgress';
import { extractReasoningText } from '../../../utils/messages';
import type { GglibMessageCustom } from '../../../types/messages';

import { cn } from '../../../utils/cn';

/** Shared styling for small action buttons under a turn's body. */
const ACTION_BTN =
  'bg-transparent border-none cursor-pointer py-xs px-sm rounded-base text-sm opacity-70 transition-all duration-150 hover:opacity-100 hover:bg-surface-elevated focus-visible:opacity-100 focus-visible:outline-2 focus-visible:outline-primary';

/** The action bar: hidden until the turn is hovered or holds focus. */
const ACTION_BAR =
  'flex gap-sm mt-sm opacity-0 transition-opacity duration-200 group-hover:opacity-100 focus-within:opacity-100';

/** The text of a message's text parts, joined. */
function textOf(content: unknown): string {
  if (typeof content === 'string') return content.trim();
  if (!Array.isArray(content)) return '';
  const chunks: string[] = [];
  for (const part of content) {
    const text =
      typeof part === 'string'
        ? part
        : (part as { type?: unknown; text?: unknown })?.type === 'text'
          ? (part as { text?: unknown }).text
          : null;
    if (typeof text === 'string' && text.trim()) chunks.push(text.trim());
  }
  return chunks.join('\n\n');
}

/**
 * A reply: the model's turn. The margin says who and, from what the page
 * has for this turn, how it was made; while it arrives, what it is doing.
 * Its reasoning and tool calls are the detail "How this was made" opens,
 * shown while it arrives. A turn with no text of its own always shows them,
 * so no row is a margin beside an empty body.
 */
export const AssistantMessageBubble: React.FC = () => {
  const message = useMessage();
  const timing = useThinkingTiming();
  const detailId = useId();
  const [detailChoice, setDetailChoice] = useState<boolean | null>(null);

  const contentArray: readonly unknown[] = Array.isArray(message.content) ? message.content : [];
  const thinkingText = extractReasoningText(contentArray);
  const contentText = textOf(message.content);
  const facts = replyFacts(message);
  const far = useContext(MessageActionsContext)?.source === 'far';

  const isStreaming = timing?.currentStreamingAssistantMessageId === message.id;
  const isCurrentlyThinking = isStreaming && !!thinkingText && !contentText;
  const toolCallsRunning = contentArray.some(
    (part) => (part as { type?: unknown }).type === 'tool-call' && !('result' in (part as object)),
  );
  const hasDetail = !!thinkingText || facts.toolCalls > 0;
  // Without text the detail is the body: never folded away.
  const detailOpen = !contentText || (detailChoice ?? isStreaming);

  const made = isStreaming ? (
    <ReplyArriving
      phase={arrivingPhase({
        prompt: facts.prompt,
        hasReasoning: !!thinkingText,
        hasText: !!contentText,
        toolCallsRunning,
      })}
      prompt={facts.prompt}
    />
  ) : (
    <ReplyMade
      facts={facts}
      detailId={hasDetail && contentText ? detailId : undefined}
      detailOpen={detailOpen}
      onToggleDetail={() => setDetailChoice(!detailOpen)}
    />
  );

  return (
    <MessagePrimitive.Root className="group">
      <TurnRow
        who={
          <TurnWho
            name={replyName(facts)}
            at={facts.savedAt}
            quantization={facts.made?.modelQuantization}
            device={facts.made?.device}
          />
        }
        made={made}
        body={
          <>
            {hasDetail && (
              <div id={detailId} hidden={!detailOpen} className="mb-md">
                {thinkingText && (
                  <ThinkingBlock
                    messageId={message.id}
                    segmentIndex={0}
                    thinking={thinkingText}
                    durationSeconds={facts.thinkingSeconds ?? null}
                    isStreaming={isCurrentlyThinking}
                    timeInMargin={!isStreaming && facts.thinkingSeconds != null}
                  />
                )}
                <ToolUsageBadge />
                <ToolExecutionProgress />
              </div>
            )}
            <div className="text-base leading-relaxed text-text">
              {contentText && <MarkdownMessageContent text={contentText} />}
              {!thinkingText && !contentText && isStreaming && (
                <span className="text-text-muted animate-blink" aria-hidden>…</span>
              )}
              {!hasDetail && !contentText && !isStreaming && (
                <p className="m-0 text-text-muted">
                  {facts.unfinished ? 'Stopped before it wrote anything.' : 'No text.'}
                </p>
              )}
            </div>
            <ActionBarPrimitive.Root className={ACTION_BAR}>
              <ActionBarPrimitive.Copy className={ACTION_BTN} title="Copy message" aria-label="Copy message">
                <Icon icon={Copy} size={14} />
              </ActionBarPrimitive.Copy>
              {!far && (
                <ActionBarPrimitive.Reload className={ACTION_BTN} title="Regenerate reply" aria-label="Regenerate reply">
                  <Icon icon={RefreshCw} size={14} />
                </ActionBarPrimitive.Reload>
              )}
            </ActionBarPrimitive.Root>
          </>
        }
      />
    </MessagePrimitive.Root>
  );
};

/**
 * Who sent a user's turn: the paired device its saved row names, on either
 * machine. Otherwise "You" on this machine, and on a far chat for a turn
 * just sent from here and not yet read back; a far row that names no
 * device was typed at the other machine.
 */
function userName(message: { id: string; metadata?: unknown }, far: boolean): string {
  const device = (message.metadata as { custom?: GglibMessageCustom } | undefined)?.custom?.device;
  if (device) return device;
  return far && message.id.startsWith('db-') ? 'Other machine' : 'You';
}

/**
 * A turn of the user's. Includes copy, edit, and delete actions, but on a
 * far chat only copy.
 */
export const UserMessageBubble: React.FC = () => {
  const message = useMessage();
  const messageActions = useContext(MessageActionsContext);
  const far = messageActions?.source === 'far';

  const handleDelete = () => {
    if (messageActions && message.id) {
      messageActions.onDeleteMessage(message.id);
    }
  };

  return (
    <MessagePrimitive.Root className="group">
      <TurnRow
        who={<TurnWho name={userName(message, far)} at={message.createdAt} />}
        body={
          <>
            <div className="text-base leading-relaxed text-text-secondary">
              <MarkdownMessageContent />
            </div>
            <ActionBarPrimitive.Root className={ACTION_BAR}>
              <ActionBarPrimitive.Copy className={ACTION_BTN} title="Copy message" aria-label="Copy message">
                <Icon icon={Copy} size={14} />
              </ActionBarPrimitive.Copy>
              {!far && (
                <ActionBarPrimitive.Edit className={ACTION_BTN} title="Edit message" aria-label="Edit message">
                  <Icon icon={Pencil} size={14} />
                </ActionBarPrimitive.Edit>
              )}
              {!far && <Button
                variant="dangerGhost"
                size="sm"
                className={cn(ACTION_BTN, 'hover:opacity-100')}
                onClick={handleDelete}
                title="Delete message"
                aria-label="Delete message"
                iconOnly
              >
                <Icon icon={Trash2} size={14} />
              </Button>}
            </ActionBarPrimitive.Root>
          </>
        }
      />
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

  return (
    <MessagePrimitive.Root className="group">
      <TurnRow
        who={<TurnWho name="You" at={message.createdAt} />}
        body={
          <ComposerPrimitive.Root className="flex flex-col gap-sm w-full">
            <ComposerPrimitive.Input
              aria-label="Edit message"
              className="w-full min-h-[60px] p-sm bg-background-input border border-border rounded-md text-text font-[inherit] text-base resize-y focus:outline-none focus:border-primary"
            />
            <div className="flex justify-end gap-sm">
              <ComposerPrimitive.Cancel className="py-xs px-md rounded-sm text-sm cursor-pointer transition-all duration-150 bg-transparent border border-border text-text-muted hover:bg-surface-hover hover:text-text">
                Cancel
              </ComposerPrimitive.Cancel>
              <ComposerPrimitive.Send className="py-xs px-md rounded-base text-sm cursor-pointer transition-all duration-150 bg-primary border-none text-text-inverse font-medium hover:bg-primary-hover disabled:opacity-50 disabled:cursor-not-allowed">
                Save & Regenerate
              </ComposerPrimitive.Send>
            </div>
          </ComposerPrimitive.Root>
        }
      />
    </MessagePrimitive.Root>
  );
};
