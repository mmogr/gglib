import React, { useState, useRef, useEffect, useMemo, useCallback } from 'react';
import 'highlight.js/styles/github-dark.css';
import { appLogger } from '../../services/platform';
import {
  ThreadPrimitive,
  ComposerPrimitive,
  useThreadRuntime,
  useThread,
} from '@assistant-ui/react';
import type { ThreadMessageLike } from '@assistant-ui/react';
import { AlertTriangle, Download, Mic, MicOff, Pencil, RotateCcw, Sparkles } from 'lucide-react';
import { Button } from '../ui/Button';
import { getMessages, deleteMessage } from '../../services/clients/chat';
import type { ConversationSummary } from '../../services/clients/chat';
import type { ToastType } from '../Toast';
import { ConfirmDeleteModal } from './ConfirmDeleteModal';
import { ToolsPopover } from '../ToolsPopover';
import { Icon } from '../ui/Icon';
import { Input } from '../ui/Input';
import { Textarea } from '../ui/Textarea';
import {
  MessageActionsContext,
  AssistantMessageBubble,
  UserMessageBubble,
  SystemMessageBubble,
  EditComposer,
  extractDbId,
} from './components';
import type { MessageActionsContextValue } from './components';
import { useChatPersistence, useTitleGeneration } from './hooks';
import { useSharedTicker } from './hooks/useSharedTicker';
import { ThinkingTimingProvider } from './context/ThinkingTimingContext';
import { VoiceProvider, useVoiceContextValue } from './context/VoiceContext';
import type { ReasoningTimingTracker } from '../../hooks/useGglibRuntime/reasoningTiming';
import type { UseVoiceModeReturn } from '../../hooks/useVoiceMode';
import { cn } from '../../utils/cn';
import { DEFAULT_SYSTEM_PROMPT } from '../../hooks/useGglibRuntime';
import { ToolSupportIndicator } from '../ToolSupportIndicator';
import { getToolRegistry } from '../../services/tools';
import { CouncilThread } from '../Council/Messages/CouncilThread';
import { CouncilToggle } from '../Council/Composer/CouncilToggle';
import { useCouncil } from '../../hooks/useCouncil';
import type { GglibMessageCustom } from '../../types/messages';
import type { SerializableCouncilSession } from '../../types/council';
import { toSerializableSession } from '../../types/council';


interface ChatMessagesPanelProps {
  activeConversation: ConversationSummary | null;
  activeConversationId: number | null;
  isServerConnected: boolean;
  serverPort: number;
  titleGenerationPrompt: string;
  onRenameConversation: (title: string) => Promise<void>;
  onClearConversation: () => Promise<void>;
  onExportConversation: () => void;
  onUpdateSystemPrompt: (prompt: string | null) => Promise<void>;
  onClose?: () => void;
  persistedMessageIds: React.MutableRefObject<Set<string>>;
  syncConversations: (options?: { preferredId?: number | null; silent?: boolean }) => Promise<void>;
  chatError: string | null;
  setChatError: (error: string | null) => void;
  showToast: (message: string, type?: ToastType, duration?: number) => void;
  timingTracker: ReasoningTimingTracker | null;
  currentStreamingAssistantMessageId: string | null;
  /** Voice mode hook return (optional — only in Tauri) */
  voice?: UseVoiceModeReturn;
  /**
   * Whether the active model supports tool/function calling.
   * null = unknown (capability status not yet resolved).
   */
  supportsToolCalls?: boolean | null;
  /** Detected tool-calling format, e.g. "hermes" or "llama3". */
  toolFormat?: string | null;
  /** Ref for council submit callback (filled by this component). */
  councilSubmitRef?: React.MutableRefObject<((text: string) => void) | null>;
  /** Set one-shot metadata on the next user message. */
  setNextMessageMeta?: (meta: Partial<GglibMessageCustom>) => void;
  /** Called when a council session completes (for persistence). */
  onCouncilComplete?: (topic: string, synthesisText: string, session: SerializableCouncilSession) => void;
}

const ChatMessagesPanel: React.FC<ChatMessagesPanelProps> = ({
  activeConversation,
  activeConversationId,
  isServerConnected,
  serverPort,
  titleGenerationPrompt,
  onRenameConversation,
  onClearConversation,
  onExportConversation,
  onUpdateSystemPrompt,
  onClose,
  persistedMessageIds,
  syncConversations,
  chatError,
  setChatError,
  showToast,
  timingTracker,
  currentStreamingAssistantMessageId,
  voice,
  supportsToolCalls,
  toolFormat,
  councilSubmitRef,
  setNextMessageMeta,
  onCouncilComplete,
}) => {
  const threadRuntime = useThreadRuntime({ optional: true });
  const threadState = useThread({ optional: true });
  const isThreadRunning = threadState?.isRunning ?? false;

  // Council mode toggle state
  const [isCouncilMode, setIsCouncilMode] = useState(false);
  const isCouncilModeRef = useRef(isCouncilMode);
  isCouncilModeRef.current = isCouncilMode;

  // Council hook — wired to context
  const council = useCouncil({ serverPort });

  // Register the council suggest/refine callback so ChatPage can call it on submit.
  // During `setup` phase, follow-up messages refine the existing suggestion.
  useEffect(() => {
    if (councilSubmitRef) {
      councilSubmitRef.current = (text: string) => {
        if (council.session.phase === 'setup') {
          council.refine(text);
        } else {
          council.suggest(text);
        }
        setIsCouncilMode(false); // Reset toggle after submit
      };
      return () => { councilSubmitRef.current = null; };
    }
  }, [councilSubmitRef, council]);

  // Keep council-mode active while the session is in a non-idle phase
  // so that follow-up messages in the composer are routed through the
  // council intercept path (onCouncilSubmit) rather than normal chat.
  const councilActive = council.session.phase === 'setup' || council.session.phase === 'suggesting';
  useEffect(() => {
    if (councilActive && !isCouncilMode) {
      setIsCouncilMode(true);
    }
  }, [councilActive, isCouncilMode]);

  // Sync council mode flag to message metadata before each submission.
  // Uses a ref so the onNew callback always sees the latest toggle state.
  useEffect(() => {
    if (setNextMessageMeta) {
      setNextMessageMeta(isCouncilMode ? { isCouncilMode: true } : {});
    }
  }, [isCouncilMode, setNextMessageMeta]);

  // When council completes, persist the session as a message pair and reset
  const councilCompleteHandled = useRef(false);
  useEffect(() => {
    if (council.session.phase === 'complete' && !councilCompleteHandled.current) {
      councilCompleteHandled.current = true;
      const serialized = toSerializableSession(council.session);
      onCouncilComplete?.(council.session.topic, council.session.synthesisText, serialized);
      // Defer reset so the completion callback runs first
      queueMicrotask(() => council.reset());
    }
    if (council.session.phase !== 'complete') {
      councilCompleteHandled.current = false;
    }
  }, [council.session.phase, council.session, council, onCouncilComplete]);

  // Shared ticker for live timer updates (only runs while streaming)
  // Note: Updating tick triggers provider re-render, but messageComponents is stable
  // and ThinkingBlock re-renders are isolated. If performance issues arise on long
  // threads, migrate to useSyncExternalStore for ticker subscription.
  const tick = useSharedTicker(!!currentStreamingAssistantMessageId, 100);

  // Build a stable voice context value for message bubble components
  const voiceContextValue = useVoiceContextValue(voice);

  // ─────────────────────────────────────────────────────────────────────────────
  // Persistence hook — handles message hydration and persistence
  // ─────────────────────────────────────────────────────────────────────────────
  const { isLoading: messageLoading, dbIdByPosition } = useChatPersistence({
    threadRuntime,
    activeConversationId,
    activeConversation,
    persistedMessageIds,
    syncConversations,
    setChatError,
  });

  // ─────────────────────────────────────────────────────────────────────────────
  // Title generation hook — handles rename and AI title generation
  // ─────────────────────────────────────────────────────────────────────────────
  const {
    titleDraft,
    setTitleDraft,
    isRenaming,
    startRenaming,
    cancelRenaming,
    commitRename,
    isGeneratingTitle,
    generateTitle,
  } = useTitleGeneration({
    threadRuntime,
    activeConversation,
    activeConversationId,
    serverPort,
    titleGenerationPrompt,
    onRenameConversation,
    showToast,
  });

  // ─────────────────────────────────────────────────────────────────────────────
  // System prompt editing state (kept local — simple UI state)
  // ─────────────────────────────────────────────────────────────────────────────
  const [isEditingPrompt, setIsEditingPrompt] = useState(false);
  const [systemPromptDraft, setSystemPromptDraft] = useState(DEFAULT_SYSTEM_PROMPT);

  const [savingSystemPrompt, setSavingSystemPrompt] = useState(false);
  const promptTextareaRef = useRef<HTMLTextAreaElement | null>(null);

  // Sync system prompt draft
  useEffect(() => {
    if (!isEditingPrompt) {
      setSystemPromptDraft(activeConversation?.system_prompt ?? DEFAULT_SYSTEM_PROMPT);
    }
  }, [activeConversation?.system_prompt, isEditingPrompt]);

  // Focus prompt textarea when editing
  useEffect(() => {
    if (isEditingPrompt) {
      promptTextareaRef.current?.focus();
    }
  }, [isEditingPrompt]);

  // Reset editing state when conversation changes
  useEffect(() => {
    setIsEditingPrompt(false);
    setSavingSystemPrompt(false);
  }, [activeConversationId]);

  const promptPreview = useMemo(
    () => activeConversation?.system_prompt?.trim() || DEFAULT_SYSTEM_PROMPT,
    [activeConversation?.system_prompt],
  );

  const promptHasChanges = useMemo(
    () => systemPromptDraft.trim() !== promptPreview,
    [systemPromptDraft, promptPreview],
  );

  const handleSaveSystemPrompt = async () => {
    if (!promptHasChanges) {
      setIsEditingPrompt(false);
      return;
    }
    setSavingSystemPrompt(true);
    try {
      const trimmedPrompt = systemPromptDraft.trim();
      await onUpdateSystemPrompt(trimmedPrompt.length ? trimmedPrompt : null);
      setIsEditingPrompt(false);
    } finally {
      setSavingSystemPrompt(false);
    }
  };

  const handlePromptKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      handleSaveSystemPrompt();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      setIsEditingPrompt(false);
      setSystemPromptDraft(activeConversation?.system_prompt ?? DEFAULT_SYSTEM_PROMPT);
    }
  };

  // ─────────────────────────────────────────────────────────────────────────────
  // Delete message flow (kept local — tightly coupled to modal UI)
  // ─────────────────────────────────────────────────────────────────────────────
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [deleteTargetId, setDeleteTargetId] = useState<string | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);

  const getSubsequentMessageCount = useCallback((runtimeMessageId: string): number => {
    if (!threadRuntime) return 1;
    
    const state = threadRuntime.getState();
    const messageIndex = state.messages.findIndex((m) => m.id === runtimeMessageId);
    if (messageIndex === -1) return 1;
    
    let count = 0;
    for (let i = messageIndex; i < state.messages.length; i++) {
      if (state.messages[i].role !== 'system') {
        count++;
      }
    }
    return count;
  }, [threadRuntime]);

  const handleDeleteMessage = useCallback((runtimeMessageId: string) => {
    setDeleteTargetId(runtimeMessageId);
    setDeleteModalOpen(true);
  }, []);

  const handleConfirmDelete = useCallback(async () => {
    if (!deleteTargetId || !threadRuntime || !activeConversationId) return;
    
    setIsDeleting(true);
    try {
      let dbId = extractDbId(deleteTargetId);
      
      if (!dbId) {
        const state = threadRuntime.getState();
        const position = state.messages.findIndex(m => m.id === deleteTargetId);
        if (position >= 0) {
          dbId = dbIdByPosition.current.get(position) ?? null;
        }
      }
      
      if (dbId) {
        await deleteMessage(dbId);
      } else {
        appLogger.debug('component.chat', 'Could not find DB ID for message', { messageId: deleteTargetId });
      }
      
      // Reload messages from DB and reset runtime
      const messages = await getMessages(activeConversationId);
      
      const prompt = activeConversation?.system_prompt?.trim();
      const systemPromptMessage: ThreadMessageLike[] = prompt && activeConversation
        ? [{
            id: `system-${activeConversation.id}`,
            role: 'system',
            content: [{ type: 'text' as const, text: prompt }],
            createdAt: new Date(activeConversation.created_at),
          }]
        : [];

      const reloadedMessages: ThreadMessageLike[] = [
        ...systemPromptMessage,
        ...messages.map<ThreadMessageLike>((message) => ({
          id: `db-${message.id}`,
          role: message.role as 'user' | 'assistant' | 'system',
          content: message.content,
          createdAt: new Date(message.created_at),
        })),
      ];

      // Rebuild position mapping
      dbIdByPosition.current.clear();
      const systemOffset = systemPromptMessage.length;
      messages.forEach((msg, idx) => {
        dbIdByPosition.current.set(systemOffset + idx, msg.id);
      });

      const seededIds = reloadedMessages
        .map((msg) => msg.id)
        .filter((value): value is string => Boolean(value));
      persistedMessageIds.current = new Set(seededIds);
      threadRuntime.reset(reloadedMessages);
      
      await syncConversations({ silent: true });
      showToast('Message deleted', 'success');
    } catch (error) {
      appLogger.error('component.chat', 'Failed to delete message', { error, messageId: deleteTargetId });
      showToast('Failed to delete message', 'error');
    } finally {
      setIsDeleting(false);
      setDeleteModalOpen(false);
      setDeleteTargetId(null);
    }
  }, [deleteTargetId, threadRuntime, activeConversationId, activeConversation, dbIdByPosition, persistedMessageIds, syncConversations, showToast]);

  const handleCancelDelete = useCallback(() => {
    setDeleteModalOpen(false);
    setDeleteTargetId(null);
  }, []);

  // Context value for message actions
  const messageActionsValue = useMemo<MessageActionsContextValue>(
    () => ({ onDeleteMessage: handleDeleteMessage }),
    [handleDeleteMessage]
  );

  // Stable components map (component references don't change)
  const messageComponents = useMemo(
    () => ({
      AssistantMessage: AssistantMessageBubble,
      UserMessage: UserMessageBubble,
      SystemMessage: SystemMessageBubble,
      EditComposer: EditComposer,
    }),
    []
  );

  // ─────────────────────────────────────────────────────────────────────────────
  // Render
  // ─────────────────────────────────────────────────────────────────────────────
  return (
    <div className="flex flex-col overflow-hidden relative flex-1 bg-surface md:h-full md:min-h-0">
      {/* Header */}
      <div className="p-base border-b border-border bg-background shrink-0 flex flex-wrap justify-between items-center gap-md phone:flex-nowrap">
        <div className="flex items-center gap-sm min-w-0 basis-full phone:basis-auto phone:flex-1">
          {isRenaming ? (
            <Input
              className="text-lg font-semibold bg-background border border-primary rounded-sm py-xs px-sm text-text min-w-[150px]"
              value={titleDraft}
              autoFocus
              onChange={(e) => setTitleDraft(e.target.value)}
              onBlur={commitRename}
              onKeyDown={(e) => {
                if (e.key === 'Enter') commitRename();
                else if (e.key === 'Escape') cancelRenaming();
              }}
            />
          ) : (
            <h2 className="text-lg font-semibold m-0 overflow-hidden text-ellipsis whitespace-nowrap">{activeConversation?.title || 'New Chat'}</h2>
          )}
          <Button
            variant="ghost"
            size="sm"
            title="Rename conversation"
            onClick={startRenaming}
            iconOnly
          >
            <Icon icon={Pencil} size={14} />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className={cn(isGeneratingTitle && 'pointer-events-none')}
            title={
              !activeConversationId
                ? 'No active conversation'
                : !serverPort
                  ? 'Start a server to generate titles'
                  : 'Generate title with AI'
            }
            onClick={() => generateTitle()}
            disabled={!activeConversationId || !serverPort || isGeneratingTitle || isThreadRunning}
            iconOnly
          >
            {isGeneratingTitle ? (
              <span className="inline-block w-[14px] h-[14px] border-2 border-text-muted border-t-primary rounded-full animate-spin-360" aria-label="Generating title…" />
            ) : (
              <Icon icon={Sparkles} size={14} />
            )}
          </Button>
          <span className={cn('text-xs py-xs px-sm rounded-full bg-background text-text-muted shrink-0', isThreadRunning && 'bg-primary/10 text-primary animate-research-pulse')}>
            {isThreadRunning ? 'Responding…' : 'Idle'}
          </span>
          <ToolSupportIndicator
            supports={supportsToolCalls ?? null}
            hasToolsConfigured={getToolRegistry().getEnabledDefinitions().length > 0}
            toolFormat={toolFormat}
          />
        </div>
        <div className="flex gap-sm shrink-0">
          <ToolsPopover />
          {voice?.isSupported && (
            <Button
              variant="ghost"
              size="sm"
              className={cn(voice.isActive && 'text-error')}
              onClick={() => voice.isActive ? voice.stop() : voice.start()}
              title={voice.isActive ? 'Stop voice mode' : 'Start voice mode'}
              iconOnly
            >
              <Icon icon={voice.isActive ? MicOff : Mic} size={14} />
            </Button>
          )}
          <Button variant="ghost" size="sm" onClick={onClearConversation} title="Restart conversation" iconOnly>
            <Icon icon={RotateCcw} size={14} />
          </Button>
          <Button variant="ghost" size="sm" onClick={onExportConversation} title="Export conversation" iconOnly>
            <Icon icon={Download} size={14} />
          </Button>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden flex flex-col">
        {/* System prompt card */}
        <section className="border border-border rounded-base p-md bg-background flex flex-col gap-sm shrink-0">
          <div className="flex justify-between gap-md items-start">
            <div>
              <p className="text-xs uppercase tracking-[1px] text-text-muted m-0 mb-xs">System prompt</p>
              {!isEditingPrompt && (
                <p className="m-0 text-text text-sm leading-[1.5] line-clamp-2">{promptPreview}</p>
              )}
            </div>
            <div className="flex gap-sm items-center shrink-0">
              {isEditingPrompt ? (
                <span className="text-xs text-primary">Editing…</span>
              ) : (
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => {
                    setSystemPromptDraft(activeConversation?.system_prompt ?? DEFAULT_SYSTEM_PROMPT);
                    setIsEditingPrompt(true);
                  }}
                  disabled={!activeConversation}
                >
                  Edit
                </Button>
              )}
            </div>
          </div>
          {isEditingPrompt && (
            <>
              <Textarea
                ref={promptTextareaRef}
                className="w-full p-sm border border-border rounded-sm bg-surface text-text text-sm font-[inherit] resize-y min-h-[80px] focus:outline-none focus:border-primary"
                value={systemPromptDraft}
                onChange={(e) => setSystemPromptDraft(e.target.value)}
                placeholder={DEFAULT_SYSTEM_PROMPT}
                rows={4}
                onKeyDown={handlePromptKeyDown}
              />
              <div className="flex justify-between items-center gap-sm">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setSystemPromptDraft(DEFAULT_SYSTEM_PROMPT)}
                >
                  Reset
                </Button>
                <div className="flex gap-sm">
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => {
                      setIsEditingPrompt(false);
                      setSystemPromptDraft(activeConversation?.system_prompt ?? DEFAULT_SYSTEM_PROMPT);
                    }}
                    disabled={savingSystemPrompt}
                  >
                    Cancel
                  </Button>
                  <Button
                    variant="primary"
                    size="sm"
                    onClick={handleSaveSystemPrompt}
                    disabled={savingSystemPrompt || !promptHasChanges}
                  >
                    {savingSystemPrompt ? 'Saving…' : 'Save'}
                  </Button>
                </div>
              </div>
            </>
          )}
        </section>

        {/* Error banner */}
        {chatError && <div className="py-sm px-md bg-danger/10 border border-danger rounded-sm text-danger text-sm shrink-0">{chatError}</div>}

        {/* Server stopped banner */}
        {!isServerConnected && (
          <div className="flex items-center justify-between gap-md py-sm px-md bg-warning-subtle border border-warning-border rounded-sm text-warning text-sm shrink-0">
            <span className="inline-flex items-center gap-2">
              <Icon icon={AlertTriangle} size={16} />
              Server not running — Chat is read-only
            </span>
            {onClose && (
              <Button variant="secondary" size="sm" onClick={onClose}>
                Close
              </Button>
            )}
          </div>
        )}

        {/* Messages area */}
        <div className="flex-1 min-h-0 flex flex-col border border-border rounded-base bg-background overflow-hidden">
          {messageLoading ? (
            <div className="flex items-center justify-center h-full text-text-muted">Loading messages…</div>
          ) : (
            <MessageActionsContext.Provider value={messageActionsValue}>
              <ThinkingTimingProvider value={{ timingTracker, currentStreamingAssistantMessageId, tick }}>
              <VoiceProvider value={voiceContextValue}>
                <ThreadPrimitive.Root
                  key={activeConversationId ?? 'thread-root'}
                  className="flex flex-col h-full min-h-0"
                >
                  <ThreadPrimitive.Viewport className="flex-1 overflow-y-auto p-md flex flex-col gap-md scroll-smooth" autoScroll>
                    <ThreadPrimitive.Messages
                      components={messageComponents}
                    />
                    <CouncilThread
                      onRun={(config) => council.run(config)}
                      onCancel={() => council.reset()}
                      onUpdateAgent={council.updateAgent}
                      onRemoveAgent={council.removeAgent}
                      onAddAgent={council.addAgent}
                    />
                  <ThreadPrimitive.ScrollToBottom className="sticky bottom-sm self-center py-xs px-md bg-primary text-white border-none rounded-full text-sm cursor-pointer opacity-0 transition-opacity duration-200 data-[visible=true]:opacity-100">
                    Jump to latest
                  </ThreadPrimitive.ScrollToBottom>
                </ThreadPrimitive.Viewport>

                <div className="border-t border-border p-md shrink-0">
                  {isThreadRunning && (
                    <div className="text-sm text-primary mb-sm animate-research-pulse">Assistant is thinking…</div>
                  )}
                  {council.session.phase === 'suggesting' && (
                    <div className="text-sm text-primary mb-sm animate-pulse">Designing council…</div>
                  )}
                  <ComposerPrimitive.Root className="flex gap-sm items-end">
                    <ComposerPrimitive.Input
                      className="flex-1 py-sm px-md border border-border rounded-base bg-surface text-text text-sm font-[inherit] resize-none min-h-[40px] max-h-[150px] focus:outline-none focus:border-primary disabled:opacity-50 disabled:cursor-not-allowed"
                      placeholder={
                        isServerConnected
                          ? isCouncilMode
                            ? 'Describe the topic for the Council of Agents…'
                            : 'Type your message. Shift + Enter for newline'
                          : 'Server not connected'
                      }
                      disabled={!isServerConnected}
                    />
                    <CouncilToggle
                      active={isCouncilMode}
                      onToggle={() => setIsCouncilMode((prev) => !prev)}
                      disabled={!isServerConnected || council.isStreaming}
                    />
                    <div className="flex gap-sm shrink-0">
                      {isThreadRunning && (
                        <Button
                          variant="danger"
                          size="sm"
                          onClick={() => threadRuntime?.cancelRun()}
                          title="Stop generation"
                        >
                          Stop
                        </Button>
                      )}
                      {council.isStreaming && (
                        <Button
                          variant="danger"
                          size="sm"
                          onClick={() => council.cancel()}
                          title="Stop council"
                        >
                          Stop
                        </Button>
                      )}
                      <ComposerPrimitive.Send asChild>
                        <Button
                          variant="primary"
                          size="sm"
                          disabled={!isServerConnected}
                        >
                          Send ↵
                        </Button>
                      </ComposerPrimitive.Send>
                    </div>
                  </ComposerPrimitive.Root>
                </div>
              </ThreadPrimitive.Root>
              </VoiceProvider>
              </ThinkingTimingProvider>
            </MessageActionsContext.Provider>
          )}
        </div>
      </div>

      {/* Delete confirmation modal */}
      <ConfirmDeleteModal
        isOpen={deleteModalOpen}
        messageCount={deleteTargetId ? getSubsequentMessageCount(deleteTargetId) : 1}
        isDeleting={isDeleting}
        onConfirm={handleConfirmDelete}
        onCancel={handleCancelDelete}
      />
    </div>
  );
};

export default ChatMessagesPanel;
