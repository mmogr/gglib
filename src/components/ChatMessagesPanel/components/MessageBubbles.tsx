import React, { useContext, useCallback } from 'react';
import {
  ComposerPrimitive,
  MessagePrimitive,
  ActionBarPrimitive,
  useMessage,
} from '@assistant-ui/react';
import { Bot, Copy, Loader2, Mic, Pencil, Trash2, User as UserIcon, Volume2 } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { Button } from '../../ui/Button';
import { parseThinkingContent } from '../../../utils/thinkingParser';
import ThinkingBlock from '../ThinkingBlock';
import MarkdownMessageContent from './MarkdownMessageContent';
import { threadMessageToTranscriptMarkdown } from '../../../utils/messages';
import { MessageActionsContext } from './MessageActionsContext';
import { useThinkingTiming } from '../context/ThinkingTimingContext';
import { ToolUsageBadge } from '../../ToolUsageBadge';
import { useDeepResearchContext } from '../context/DeepResearchContext';
import { ResearchArtifact } from '../../DeepResearch';
import type { GglibMessageCustom } from '../../../types/messages';
import type { ResearchState } from '../../../hooks/useDeepResearch/types';
import { useVoiceContextOptional } from '../context/VoiceContext';
import { stripThinkingBlocks } from '../../../utils/stripThinkingBlocks';

import { cn } from '../../../utils/cn';

/** Shared styling for small action buttons in message bubble footers. */
const ACTION_BTN =
  'bg-transparent border-none cursor-pointer py-[4px] px-[8px] rounded-sm text-[14px] opacity-70 transition-all duration-150 hover:opacity-100 hover:bg-[var(--color-surface-hover,rgba(255,255,255,0.1))]';

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
 * Check if a message originated from voice input/output.
 */
function isVoiceMessage(message: ReturnType<typeof useMessage>): boolean {
  const custom = (message as any)?.metadata?.custom as GglibMessageCustom | undefined;
  return custom?.isVoice === true;
}

/**
 * Extract speakable text from a message's content parts.
 */
function extractSpeakableText(message: ReturnType<typeof useMessage>): string {
  const content = (message as any)?.content;
  let text = '';
  if (typeof content === 'string') {
    text = content;
  } else if (Array.isArray(content)) {
    text = content
      .filter((p: any): p is { type: 'text'; text: string } => p?.type === 'text')
      .map((p: any) => p.text)
      .join(' ');
  }
  return stripThinkingBlocks(text);
}

/**
 * Speak button for assistant messages.
 * Only renders when voice mode is active and TTS is loaded.
 */
const SpeakButton: React.FC<{ message: ReturnType<typeof useMessage> }> = ({ message }) => {
  const voiceCtx = useVoiceContextOptional();

  const handleSpeak = useCallback(() => {
    if (!voiceCtx) return;
    const text = extractSpeakableText(message);
    if (text) {
      voiceCtx.speak(text);
    }
  }, [voiceCtx, message]);

  // Don't render unless voice mode is active with TTS ready
  if (!voiceCtx?.isActive || !voiceCtx?.ttsLoaded) return null;

  const busy = voiceCtx.isSpeaking || voiceCtx.isTtsGenerating;

  return (
    <button
      className={cn(ACTION_BTN, 'hover:text-[var(--color-accent,#89b4fa)] disabled:opacity-35 disabled:cursor-not-allowed')}
      onClick={handleSpeak}
      disabled={busy}
      title={busy ? 'TTS is busy' : 'Read aloud'}
      aria-label="Read aloud"
    >
      <Icon
        icon={voiceCtx.isTtsGenerating ? Loader2 : Volume2}
        size={14}
        className={voiceCtx.isTtsGenerating ? 'animate-chat-action-spin' : undefined}
      />
    </button>
  );
};

/**
 * Message bubble for assistant responses.
 * Handles thinking blocks, markdown rendering, and deep research artifacts.
 */
export const AssistantMessageBubble: React.FC = () => {
  const message = useMessage();
  const timing = useThinkingTiming();
  const deepResearchCtx = useDeepResearchContext();
  const isVoice = isVoiceMessage(message);
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
      <MessagePrimitive.Root className="group flex flex-col gap-sm p-md rounded-base bg-surface border border-border mr-xl max-tablet:mr-0">
        <div className="flex items-center gap-sm">
          <div className="text-lg" aria-hidden>
            <Icon icon={Bot} size={18} />
          </div>
          <div>
            <div className="font-medium text-sm">Assistant</div>
            <div className="text-xs text-text-muted">{timestamp}</div>
          </div>
        </div>
        <div className="leading-[1.6]">
          <ResearchArtifact
            state={researchState}
            isRunning={isResearchRunning}
            onSkipQuestion={deepResearchCtx?.skipQuestion}
            onSkipAllPending={deepResearchCtx?.skipAllPending}
            onAddQuestion={deepResearchCtx?.addQuestion}
            onGenerateMoreQuestions={deepResearchCtx?.generateMoreQuestions}
            onExpandQuestion={deepResearchCtx?.expandQuestion}
            onGoDeeper={deepResearchCtx?.goDeeper}
            onForceAnswer={deepResearchCtx?.forceAnswer}
            defaultExpanded={true}
          />
        </div>
        <ActionBarPrimitive.Root className="flex gap-sm opacity-0 transition-opacity duration-200 group-hover:opacity-100">
          <ActionBarPrimitive.Copy />
        </ActionBarPrimitive.Root>
      </MessagePrimitive.Root>
    );
  }

  // Standard assistant message rendering
  return (
    <MessagePrimitive.Root className="group flex flex-col gap-sm p-md rounded-base bg-surface border border-border mr-xl max-tablet:mr-0">
      <div className="flex items-center gap-sm">
        <div className="text-lg" aria-hidden>
          <Icon icon={isVoice ? Volume2 : Bot} size={18} />
        </div>
        <div>
          <div className="font-medium text-sm">Assistant</div>
          <div className="text-xs text-text-muted">
            {timestamp}
            <ToolUsageBadge />
          </div>
        </div>
      </div>
      <div className="leading-[1.6]">
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
          <span className="text-text-muted animate-blink">…</span>
        )}
      </div>
      <ActionBarPrimitive.Root className="flex gap-sm opacity-0 transition-opacity duration-200 group-hover:opacity-100">
        <SpeakButton message={message} />
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
  const isVoice = isVoiceMessage(message);
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
    <MessagePrimitive.Root className="group flex flex-col gap-sm p-md rounded-base ml-xl bg-primary-alpha border border-primary max-tablet:ml-0">
      <div className="flex items-center gap-sm">
        <div className="text-lg" aria-hidden>
          <Icon icon={isVoice ? Mic : UserIcon} size={18} />
        </div>
        <div>
          <div className="font-medium text-sm">{isVoice ? 'You (voice)' : 'You'}</div>
          <div className="text-xs text-text-muted">{timestamp}</div>
        </div>
      </div>
      <div className="leading-[1.6]">
        <MarkdownMessageContent />
      </div>
      <ActionBarPrimitive.Root className="flex gap-sm opacity-0 transition-opacity duration-200 group-hover:opacity-100">
        <ActionBarPrimitive.Copy className={ACTION_BTN} title="Copy message" aria-label="Copy message">
          <Icon icon={Copy} size={14} />
        </ActionBarPrimitive.Copy>
        <ActionBarPrimitive.Edit className={ACTION_BTN} title="Edit message" aria-label="Edit message">
          <Icon icon={Pencil} size={14} />
        </ActionBarPrimitive.Edit>
        <Button
          variant="ghost"
          size="sm"
          className={cn(ACTION_BTN, 'hover:!bg-[rgba(243,139,168,0.2)]')}
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
    <MessagePrimitive.Root className="group flex flex-col gap-sm p-md rounded-md ml-xl bg-primary-alpha border-2 border-primary max-tablet:ml-0">
      <div className="flex items-center gap-sm">
        <div className="text-lg" aria-hidden>
          <Icon icon={UserIcon} size={18} />
        </div>
        <div>
          <div className="font-medium text-sm">You</div>
          <div className="text-xs text-text-muted">{timestamp}</div>
        </div>
      </div>
      <ComposerPrimitive.Root className="flex flex-col gap-sm w-full">
        <ComposerPrimitive.Input className="w-full min-h-[60px] p-sm bg-background border border-border rounded-sm text-text font-[inherit] text-sm resize-y focus:outline-none focus:border-primary" />
        <div className="flex justify-end gap-sm">
          <ComposerPrimitive.Cancel className="py-xs px-md rounded-sm text-sm cursor-pointer transition-all duration-150 bg-transparent border border-border text-text-muted hover:bg-surface-hover hover:text-text">
            Cancel
          </ComposerPrimitive.Cancel>
          <ComposerPrimitive.Send className="py-xs px-md rounded-sm text-sm cursor-pointer transition-all duration-150 bg-primary border-none text-text font-medium hover:bg-primary-hover disabled:opacity-50 disabled:cursor-not-allowed">
            Save & Regenerate
          </ComposerPrimitive.Send>
        </div>
      </ComposerPrimitive.Root>
    </MessagePrimitive.Root>
  );
};
