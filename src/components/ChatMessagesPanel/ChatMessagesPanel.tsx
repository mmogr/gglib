import React, { useState, useRef, useEffect, useMemo, useCallback } from 'react';
import 'highlight.js/styles/github-dark.css';
import { appLogger } from '../../services/platform';
import {
  ThreadPrimitive,
  ComposerPrimitive,
  useThreadRuntime,
  useThread,
  useComposerRuntime,
} from '@assistant-ui/react';
import type { ThreadMessageLike } from '@assistant-ui/react';
import { AlertTriangle, Download, Mic, MicOff, Pencil, RotateCcw, Sparkles } from 'lucide-react';
import { Button } from '../ui/Button';
import { getMessages, deleteMessage, saveMessage, updateMessage } from '../../services/clients/chat';
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
import { DeepResearchProvider } from './context/DeepResearchContext';
import { VoiceProvider, useVoiceContextValue } from './context/VoiceContext';
import type { ReasoningTimingTracker } from '../../hooks/useGglibRuntime/reasoningTiming';
import type { UseVoiceModeReturn } from '../../hooks/useVoiceMode';
import { DeepResearchToggle } from '../DeepResearch';
import { useDeepResearch } from '../../hooks/useDeepResearch';
import type { ResearchState } from '../../hooks/useDeepResearch/types';
import { cn } from '../../utils/cn';
import { DEFAULT_SYSTEM_PROMPT } from '../../hooks/useGglibRuntime';

// Use the same prompts as the runtime for consistency
const FALLBACK_SYSTEM_PROMPT = DEFAULT_SYSTEM_PROMPT;

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
}) => {
  const threadRuntime = useThreadRuntime({ optional: true });
  const composerRuntime = useComposerRuntime({ optional: true });
  const threadState = useThread({ optional: true });
  const isThreadRunning = threadState?.isRunning ?? false;

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

  // ─────────────────────────────────────────────────────────────────────────────
  // Deep Research mode state and hook
  // ─────────────────────────────────────────────────────────────────────────────
  const [isDeepResearchEnabled, setIsDeepResearchEnabled] = useState(false);
  
  // Track the assistant message ID that contains the research state
  const researchMessageIdRef = useRef<string | null>(null);

  // Deep research hook - handles the research loop and persistence
  const deepResearch = useDeepResearch({
    serverPort,
    conversationId: activeConversationId ?? undefined,
    systemPrompt: activeConversation?.system_prompt ?? DEFAULT_SYSTEM_PROMPT,
    onStateChange: (newState: ResearchState) => {
      // Update the assistant message with new research state
      if (researchMessageIdRef.current && threadRuntime) {
        const state = threadRuntime.getState();
        const updatedMessages = state.messages.map((msg) => {
          if (msg.id === researchMessageIdRef.current) {
            // Update the message's metadata with the new research state
            return {
              ...msg,
              metadata: {
                ...msg.metadata,
                custom: {
                  ...(msg.metadata?.custom || {}),
                  researchState: newState,
                  isDeepResearch: true,
                },
              },
            } as ThreadMessageLike;
          }
          return msg;
        });
        // Force re-render by resetting messages
        threadRuntime.reset(updatedMessages);
      }
    },
    onPersist: async (stateToSave: ResearchState) => {
      // Persist the research state to database
      // This will be saved as JSON in the message metadata
      if (!activeConversationId) return;
      
      try {
        // Get the dbId from the message's custom metadata
        const customMeta = researchMessageIdRef.current
          ? (threadRuntime?.getState().messages.find(m => m.id === researchMessageIdRef.current)?.metadata?.custom as { dbId?: number })
          : null;
        
        if (customMeta?.dbId) {
          // Update existing message with new content and metadata
          const content = stateToSave.finalReport || `[Deep Research: ${stateToSave.phase}]`;
          await updateMessage(
            customMeta.dbId,
            content,
            { isDeepResearch: true, researchState: stateToSave }
          );
        }
      } catch (error) {
        appLogger.error('component.chat', 'Failed to persist research state', { error });
      }
    },
    onError: (error: Error) => {
      setChatError(`Research error: ${error.message}`);
      showToast('Research failed', 'error');
    },
  });

  // Toggle deep research mode
  const toggleDeepResearch = useCallback(() => {
    appLogger.debug('component.chat', 'Deep research toggle clicked', { enabled: isDeepResearchEnabled });
    setIsDeepResearchEnabled((prev) => !prev);
  }, [isDeepResearchEnabled]);

  // Stop deep research
  const stopDeepResearch = useCallback(() => {
    deepResearch.stopResearch();
    showToast('Deep research stopped', 'info');
  }, [deepResearch, showToast]);

  // Handle deep research submission
  const handleDeepResearchSubmit = useCallback(async (query: string) => {
    appLogger.debug('component.chat', 'Starting deep research submission', { query: query.slice(0, 50), conversationId: activeConversationId });
    
    if (!activeConversationId || !threadRuntime) {
      appLogger.debug('component.chat', 'Missing conversation or runtime for deep research');
      showToast('No active conversation', 'error');
      return;
    }

    // 1. Create user message
    const userMessageId = `user-${crypto.randomUUID()}`;
    const userMessage: ThreadMessageLike = {
      id: userMessageId,
      role: 'user',
      content: [{ type: 'text', text: query }],
      createdAt: new Date(),
      metadata: {
        custom: {
          conversationId: activeConversationId,
        },
      },
    };

    // 2. Create placeholder assistant message for research artifact
    const assistantMessageId = `research-${crypto.randomUUID()}`;
    const initialResearchState: ResearchState = {
      originalQuery: query,
      messageId: assistantMessageId,
      conversationId: activeConversationId,
      startedAt: Date.now(),
      currentHypothesis: null,
      researchPlan: [],
      gatheredFacts: [],
      currentStep: 0,
      maxSteps: 30,
      phase: 'planning',
      knowledgeGaps: [],
      contradictions: [],
      lastReasoning: null,
      pendingObservations: [],
      finalReport: null,
      citations: [],
      // Verbose tracking fields
      activityLog: [],
      activeToolCalls: [],
      isLLMGenerating: false,
      // Multi-round research fields
      searchHistory: [],
      currentRound: 1,
      maxRounds: 3,
      roundSummaries: [],
      // Multi-perspective research fields
      complexity: 'simple',
      perspectives: [],
      currentPerspective: undefined,
      // Productive step tracking
      consecutiveUnproductiveSteps: 0,
      consecutiveTextOnlySteps: 0,
      stepsOnCurrentFocus: 0,
      currentFocusQuestionId: null,
      loopIterations: 0,
    };

    const assistantMessage: ThreadMessageLike = {
      id: assistantMessageId,
      role: 'assistant',
      content: [{ type: 'text', text: '' }],
      createdAt: new Date(),
      metadata: {
        custom: {
          conversationId: activeConversationId,
          isDeepResearch: true,
          researchState: initialResearchState,
        },
      },
    };

    // 3. Add messages to thread
    const currentMessages = threadRuntime.getState().messages;
    threadRuntime.reset([...currentMessages, userMessage, assistantMessage]);
    researchMessageIdRef.current = assistantMessageId;

    // 4. Persist user message to database
    try {
      await saveMessage(
        activeConversationId,
        'user',
        query
      );
    } catch (error) {
      appLogger.error('component.chat', 'Failed to save user message', { error, conversationId: activeConversationId });
    }

    // 5. Persist assistant message to database with research metadata
    let assistantDbId: number | undefined;
    try {
      assistantDbId = await saveMessage(
        activeConversationId,
        'assistant',
        '[Deep Research in progress...]',
        { isDeepResearch: true, researchState: initialResearchState }
      );
      
      // Update the runtime message with the dbId for future updates
      const messagesWithDbId = threadRuntime.getState().messages.map((msg) => {
        if (msg.id === assistantMessageId) {
          return {
            ...msg,
            metadata: {
              ...msg.metadata,
              custom: {
                ...(msg.metadata?.custom || {}),
                dbId: assistantDbId,
              },
            },
          } as ThreadMessageLike;
        }
        return msg;
      });
      threadRuntime.reset(messagesWithDbId);
    } catch (error) {
      appLogger.error('component.chat', 'Failed to save assistant message', { error, conversationId: activeConversationId });
    }

    // 6. Start the research loop
    try {
      await deepResearch.startResearch(query, assistantMessageId);
    } catch (error) {
      appLogger.error('component.chat', 'Research failed', { error, query: query.slice(0, 50) });
      setChatError(error instanceof Error ? error.message : 'Research failed');
    }
  }, [activeConversationId, threadRuntime, deepResearch, showToast, setChatError]);

  // Reset deep research state when conversation changes
  useEffect(() => {
    setIsDeepResearchEnabled(false);
    deepResearch.resetState();
    researchMessageIdRef.current = null;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeConversationId]); // Only reset when conversation changes, not on every deepResearch change

  const [savingSystemPrompt, setSavingSystemPrompt] = useState(false);
  const promptTextareaRef = useRef<HTMLTextAreaElement | null>(null);

  // Sync system prompt draft
  useEffect(() => {
    if (!isEditingPrompt) {
      setSystemPromptDraft(activeConversation?.system_prompt ?? FALLBACK_SYSTEM_PROMPT);
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
    () => activeConversation?.system_prompt?.trim() || FALLBACK_SYSTEM_PROMPT,
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
          role: message.role,
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
    <div className="flex flex-col h-full min-h-0 overflow-y-auto overflow-x-hidden relative flex-1 max-md:h-auto max-md:max-h-none bg-surface">
      {/* Header */}
      <div className="p-base border-b border-border bg-background shrink-0 flex justify-between items-center gap-md max-tablet:flex-wrap">
        <div className="flex items-center gap-sm min-w-0 flex-1 max-tablet:basis-full">
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
              <span className="inline-block w-[14px] h-[14px] border-2 border-text-muted border-t-primary rounded-full animate-icon-btn-spin" aria-label="Generating title…" />
            ) : (
              <Icon icon={Sparkles} size={14} />
            )}
          </Button>
          <span className={cn('text-xs py-xs px-sm rounded-full bg-background text-text-muted shrink-0', isThreadRunning && 'bg-primary/10 text-primary animate-research-pulse')}>
            {isThreadRunning ? 'Responding…' : 'Idle'}
          </span>
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
          <div className="flex items-center justify-between gap-md py-sm px-md bg-[var(--color-warning-alpha,rgba(255,193,7,0.1))] border border-[var(--color-warning,#ffc107)] rounded-sm text-[var(--color-warning-text,#856404)] text-sm shrink-0">
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
              <DeepResearchProvider
                isRunning={deepResearch.isRunning}
                skipQuestion={deepResearch.skipQuestion}
                skipAllPending={deepResearch.skipAllPending}
                addQuestion={deepResearch.addQuestion}
                generateMoreQuestions={deepResearch.generateMoreQuestions}
                expandQuestion={deepResearch.expandQuestion}
                goDeeper={deepResearch.goDeeper}
                forceAnswer={deepResearch.forceAnswer}
              >
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
                  <ThreadPrimitive.ScrollToBottom className="sticky bottom-sm self-center py-xs px-md bg-primary text-white border-none rounded-full text-sm cursor-pointer opacity-0 transition-opacity duration-200 data-[visible=true]:opacity-100">
                    Jump to latest
                  </ThreadPrimitive.ScrollToBottom>
                </ThreadPrimitive.Viewport>

                <div className="border-t border-border p-md shrink-0">
                  {isThreadRunning && !deepResearch.isRunning && (
                    <div className="text-sm text-primary mb-sm animate-research-pulse">Assistant is thinking…</div>
                  )}
                  {deepResearch.isRunning && (
                    <div className="text-sm text-primary mb-sm animate-research-pulse">Researching… This may take a few minutes.</div>
                  )}
                  <ComposerPrimitive.Root className="flex gap-sm items-end">
                    <ComposerPrimitive.Input
                      className="flex-1 py-sm px-md border border-border rounded-base bg-surface text-text text-sm font-[inherit] resize-none min-h-[40px] max-h-[150px] focus:outline-none focus:border-primary disabled:opacity-50 disabled:cursor-not-allowed"
                      placeholder={
                        isServerConnected
                          ? isDeepResearchEnabled
                            ? 'Ask a research question (Deep Research mode)'
                            : 'Type your message. Shift + Enter for newline'
                          : 'Server not connected'
                      }
                      disabled={!isServerConnected || deepResearch.isRunning}
                    />
                    <div className="flex gap-sm shrink-0">
                      <DeepResearchToggle
                        isEnabled={isDeepResearchEnabled}
                        onToggle={toggleDeepResearch}
                        isRunning={deepResearch.isRunning}
                        onStop={stopDeepResearch}
                        onWrapUp={deepResearch.requestWrapUp}
                        researchPhase={deepResearch.state?.phase}
                        disabled={!isServerConnected || isThreadRunning}
                        disabledReason={
                          !isServerConnected
                            ? 'Server not connected'
                            : isThreadRunning
                            ? 'Wait for current response'
                            : undefined
                        }
                      />
                      {isThreadRunning && !deepResearch.isRunning && (
                        <Button
                          variant="danger"
                          size="sm"
                          onClick={() => threadRuntime?.cancelRun()}
                          title="Stop generation"
                        >
                          Stop
                        </Button>
                      )}
                      {isDeepResearchEnabled ? (
                        <Button
                          variant="primary"
                          size="sm"
                          disabled={!isServerConnected || deepResearch.isRunning}
                          onClick={() => {
                            const composer = composerRuntime;
                            if (!composer) return;
                            const text = composer.getState().text.trim();
                            if (!text) return;
                            composer.setText('');
                            handleDeepResearchSubmit(text);
                          }}
                        >
                          Research ↵
                        </Button>
                      ) : (
                        <ComposerPrimitive.Send asChild>
                          <Button
                            variant="primary"
                            size="sm"
                            disabled={!isServerConnected}
                          >
                            Send ↵
                          </Button>
                        </ComposerPrimitive.Send>
                      )}
                    </div>
                  </ComposerPrimitive.Root>
                </div>
              </ThreadPrimitive.Root>
              </VoiceProvider>
              </ThinkingTimingProvider>
              </DeepResearchProvider>
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
