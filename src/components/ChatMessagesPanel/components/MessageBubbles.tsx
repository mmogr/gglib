import React, { useContext, useMemo } from 'react';
import { ComposerPrimitive, MessagePrimitive, useMessage, useThread } from '@assistant-ui/react';
import ThinkingBlock from './ThinkingBlock';
import MarkdownMessageContent from './MarkdownMessageContent';
import { MessageActionsContext } from './MessageActionsContext';
import { TurnBranches } from './BranchSwitcher';
import { MessageImages, ToolImages } from './MessageImages';
import { ComposerImages, ImageInputContext } from './ComposerImages';
import { ReplyArrival } from './ReplyArrival';
import { ToolCallRow } from './ToolCallRow';
import { FigureLine, TurnFoot, useTurnReveal } from './TurnFoot';
import { QuestionActions, ReplyActions } from './TurnActions';
import { arrivingPhase, modelShown, replyFacts } from '../../../readings/turnFigures';
import { useThinkingTiming } from '../context/ThinkingTimingContext';
import { useRunPreviews } from '../context/RunPreviewsContext';
import { settledWords, toolCallRows } from '../../ToolExecutionProgress';
import { extractReasoningText } from '../../../utils/messages';
import { replyImages, type GglibMessageCustom } from '../../../types/messages';

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

const NO_PARTS: readonly unknown[] = [];

interface TurnLike {
  id: string;
  role: string;
  metadata?: unknown;
}

const customOf = (message: { metadata?: unknown }) =>
  (message.metadata as { custom?: GglibMessageCustom } | undefined)?.custom;

/** The newest reply's id: the one whose figures and actions are always shown. */
function latestReplyId(messages: readonly TurnLike[]): string | null {
  for (let i = messages.length - 1; i >= 0; i -= 1) {
    if (messages[i].role === 'assistant') return messages[i].id;
  }
  return null;
}

/** The model of the reply before message `id`: `null` when not known, `undefined` when there is none. */
function previousModel(messages: readonly TurnLike[], id: string): string | null | undefined {
  for (let i = messages.findIndex((m) => m.id === id) - 1; i >= 0; i -= 1) {
    if (messages[i].role === 'assistant') return customOf(messages[i])?.made?.modelName ?? null;
  }
  return undefined;
}

/**
 * A reply: plain 16 px text in the reading column. Above it, its model's
 * name where the reply before was another model's; while no token has come,
 * the arrival line; then the fold with its reasoning and one row per tool
 * call. Under it, the images its tools made, always shown and never in the
 * fold, then its foot: the figure line and the action line, always on the
 * newest reply and on hover or focus elsewhere, its time on hover, and the
 * branch switcher where the chat's branches part. A tool still running
 * shows, in its row, how far it has got and the picture it is making,
 * which is the run's and never the message's.
 */
export const AssistantMessageBubble: React.FC = () => {
  const message = useMessage();
  const timing = useThinkingTiming();
  const previews = useRunPreviews();
  const { shown, handlers } = useTurnReveal();
  const latest = useThread((thread) => latestReplyId(thread.messages)) === message.id;
  const previous = useThread((thread) => previousModel(thread.messages, message.id));

  const contentArray: readonly unknown[] = Array.isArray(message.content) ? message.content : NO_PARTS;
  const thinkingText = extractReasoningText(contentArray);
  const contentText = textOf(message.content);
  const facts = replyFacts(message);
  const rows = useMemo(() => toolCallRows(contentArray), [contentArray]);
  const model = modelShown(facts, previous);

  const isStreaming = timing?.currentStreamingAssistantMessageId === message.id;
  const isCurrentlyThinking = isStreaming && !!thinkingText && !contentText;
  const hasFold = !!thinkingText || rows.length > 0;
  const arriving = isStreaming && !hasFold && !contentText;

  return (
    <MessagePrimitive.Root data-turn="reply" className="flex flex-col gap-sm" {...handlers}>
      {model && <p className="m-0 text-xs text-text-muted">{model}</p>}
      {arriving && (
        <ReplyArrival
          phase={arrivingPhase({ prompt: facts.prompt, waiting: facts.waiting })}
          prompt={facts.prompt}
          waiting={facts.waiting}
        />
      )}
      {hasFold && (
        <ThinkingBlock
          messageId={message.id}
          segmentIndex={0}
          thinking={thinkingText ?? ''}
          durationSeconds={facts.thinkingSeconds ?? null}
          isStreaming={isCurrentlyThinking}
          defaultExpanded={isStreaming}
          toolCalls={rows.length}
        >
          {rows.length > 0 && (
            <ul aria-label="Tool calls" className="m-0 p-0 list-none flex flex-col gap-sm">
              {rows.map((row) => (
                <ToolCallRow key={row.id} row={row} preview={previews.get(row.id)} />
              ))}
            </ul>
          )}
        </ThinkingBlock>
      )}
      {/* While the reply arrives, each tool call that settles is said, once,
          politely: one span per call, so the region announces only additions. */}
      {isStreaming && rows.length > 0 && (
        <div className="sr-only" aria-live="polite" aria-relevant="additions">
          {settledWords(rows).map((call) => (
            <span key={call.id}>{call.words} </span>
          ))}
        </div>
      )}
      {contentText && (
        <div className="text-base leading-relaxed text-text">
          <MarkdownMessageContent text={contentText} />
        </div>
      )}
      {!hasFold && !contentText && !isStreaming && (
        <p className="m-0 text-xs text-text-muted">
          {facts.unfinished ? 'Stopped before it wrote anything.' : 'No text.'}
        </p>
      )}
      <ToolImages images={replyImages(message)} />
      {!arriving && (
        <TurnFoot
          at={facts.savedAt}
          shown={shown}
          figures={<FigureLine facts={facts} />}
          figuresAlways={latest}
          actions={isStreaming ? undefined : <ReplyActions hasText={!!contentText} />}
          actionsAlways={latest}
          branches={<TurnBranches />}
        />
      )}
    </MessagePrimitive.Root>
  );
};

/**
 * Who sent a question, where it was not you: the paired device its saved
 * row names, on either machine, or on a far chat "Other machine" for a row
 * typed at that machine. A question sent from here names no one.
 */
function senderOf(message: { id: string; metadata?: unknown }, far: boolean): string | null {
  const device = customOf(message)?.device;
  if (device) return device;
  return far && message.id.startsWith('db-') ? 'Other machine' : null;
}

/**
 * A question: a filled block at full contrast, its images then its text,
 * with its sender above it when that was not you, and under it its foot:
 * its time on hover, and the action line on hover or focus.
 */
export const UserMessageBubble: React.FC = () => {
  const message = useMessage();
  const far = useContext(MessageActionsContext)?.source === 'far';
  const { shown, handlers } = useTurnReveal();
  const sender = senderOf(message, far);

  return (
    <MessagePrimitive.Root data-turn="question" className="flex flex-col items-end gap-xs" {...handlers}>
      {sender && <p className="m-0 text-xs text-text-muted">{sender}</p>}
      <div className="max-w-[85%] min-w-0 rounded-lg bg-surface-elevated py-sm px-base text-base leading-relaxed text-text">
        <MessageImages />
        <MarkdownMessageContent />
      </div>
      <TurnFoot align="end" at={message.createdAt} shown={shown} actions={<QuestionActions />} branches={<TurnBranches />} />
    </MessagePrimitive.Root>
  );
};

/**
 * Placeholder for system messages (not rendered).
 */
export const SystemMessageBubble: React.FC = () => null;

/**
 * An edit of a turn, in its place: a question's text and images, each of
 * which can be removed (a paste adds one only where the model takes
 * images), asked again; or a reply's text, kept as written. An edit that
 * would rewrite a saved reply is made on a new branch of the chat, and the
 * chat it was made on is kept (ADR 0017). assistant-ui sends an edit only
 * when its text changed.
 */
export const EditComposer: React.FC = () => {
  const message = useMessage();
  const imageInput = useContext(ImageInputContext);
  const reply = message.role === 'assistant';

  return (
    <MessagePrimitive.Root data-turn={reply ? 'reply' : 'question'} className="flex flex-col">
      <ComposerPrimitive.Root className="flex flex-col gap-sm w-full">
        {!reply && <ComposerImages />}
        <ComposerPrimitive.Input
          aria-label={reply ? 'Edit reply' : 'Edit message'}
          addAttachmentOnPaste={!reply && imageInput.offered}
          className="w-full min-h-[60px] p-sm bg-background-input border border-border rounded-md text-text font-[inherit] text-base resize-y focus:outline-none focus:border-primary"
        />
        <div className="flex justify-end gap-sm">
          <ComposerPrimitive.Cancel className="py-xs px-md rounded-sm text-sm cursor-pointer transition-all duration-150 bg-transparent border border-border text-text-muted hover:bg-surface-hover hover:text-text">
            Cancel
          </ComposerPrimitive.Cancel>
          <ComposerPrimitive.Send className="py-xs px-md rounded-base text-sm cursor-pointer transition-all duration-150 bg-primary border-none text-text-inverse font-semibold hover:bg-primary-hover disabled:opacity-50 disabled:cursor-not-allowed">
            {reply ? 'Save' : 'Send'}
          </ComposerPrimitive.Send>
        </div>
      </ComposerPrimitive.Root>
    </MessagePrimitive.Root>
  );
};
