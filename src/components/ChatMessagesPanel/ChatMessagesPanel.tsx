import React, { useState, useRef, useEffect, useMemo, useCallback } from 'react';
import 'highlight.js/styles/github-dark.css';
import {
  ThreadPrimitive,
  ComposerPrimitive,
  useThreadRuntime,
  useThread,
} from '@assistant-ui/react';
import type { ThreadMessageLike } from '@assistant-ui/react';
import { AlertTriangle, Download, Pencil, RotateCcw, Sparkles } from 'lucide-react';
import { getMessages, deleteMessage } from '../../services/clients/chat';
import type { ConversationSummary } from '../../services/clients/chat';
import type { ToastType } from '../Toast';
import { ConfirmDeleteModal } from './ConfirmDeleteModal';
import { ToolsPopover } from '../ToolsPopover';
import { Icon } from '../ui/Icon';
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
import type { ReasoningTimingTracker } from '../../hooks/useGglibRuntime/reasoningTiming';
import './ChatMessagesPanel.css';
import { DEFAULT_SYSTEM_PROMPT } from '../../hooks/useGglibRuntime';

// Use the same prompts as the runtime for consistency
const FALLBACK_SYSTEM_PROMPT = DEFAULT_SYSTEM_PROMPT;

const cx = (...classes: Array<string | false | undefined>) =>
  classes.filter(Boolean).join(' ');

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
}) => {
  const threadRuntime = useThreadRuntime({ optional: true });
  const threadState = useThread({ optional: true });
  const isThreadRunning = threadState?.isRunning ?? false;

  // Shared ticker for live timer updates (only runs while streaming)
  // Note: Updating tick triggers provider re-render, but messageComponents is stable
  // and ThinkingBlock re-renders are isolated. If performance issues arise on long
  // threads, migrate to useSyncExternalStore for ticker subscription.
  const tick = useSharedTicker(!!currentStreamingAssistantMessageId, 100);

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
        console.warn('Could not find DB ID for message:', deleteTargetId);
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
      console.error('Failed to delete message:', error);
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
    <div className="mcc-panel chat-messages-panel">
      {/* Header */}
      <div className="mcc-panel-header chat-header">
        <div className="chat-title-group">
          {isRenaming ? (
            <input
              className="chat-title-input"
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
            <h2 className="chat-title">{activeConversation?.title || 'New Chat'}</h2>
          )}
          <button
            className="icon-btn icon-btn-sm"
            title="Rename conversation"
            onClick={startRenaming}
          >
            <Icon icon={Pencil} size={14} />
          </button>
          <button
            className={cx('icon-btn icon-btn-sm', isGeneratingTitle && 'generating')}
            title={
              !activeConversationId
                ? 'No active conversation'
                : !serverPort
                  ? 'Start a server to generate titles'
                  : 'Generate title with AI'
            }
            onClick={() => generateTitle()}
            disabled={!activeConversationId || !serverPort || isGeneratingTitle || isThreadRunning}
          >
            {isGeneratingTitle ? (
              <span className="icon-btn-spinner" aria-label="Generating title…" />
            ) : (
              <Icon icon={Sparkles} size={14} />
            )}
          </button>
          <span className={cx('chat-status-badge', isThreadRunning && 'active')}>
            {isThreadRunning ? 'Responding…' : 'Idle'}
          </span>
        </div>
        <div className="chat-header-actions">
          <ToolsPopover />
          <button className="icon-btn icon-btn-sm" onClick={onClearConversation} title="Restart conversation">
            <Icon icon={RotateCcw} size={14} />
          </button>
          <button className="icon-btn icon-btn-sm" onClick={onExportConversation} title="Export conversation">
            <Icon icon={Download} size={14} />
          </button>
        </div>
      </div>

      {/* Content */}
      <div className="mcc-panel-content chat-content">
        {/* System prompt card */}
        <section className="chat-prompt-card">
          <div className="chat-prompt-header">
            <div>
              <p className="chat-prompt-label">System prompt</p>
              {!isEditingPrompt && (
                <p className="chat-prompt-preview">{promptPreview}</p>
              )}
            </div>
            <div className="chat-prompt-actions">
              {isEditingPrompt ? (
                <span className="chat-prompt-editing-badge">Editing…</span>
              ) : (
                <button
                  type="button"
                  className="btn btn-sm"
                  onClick={() => {
                    setSystemPromptDraft(activeConversation?.system_prompt ?? DEFAULT_SYSTEM_PROMPT);
                    setIsEditingPrompt(true);
                  }}
                  disabled={!activeConversation}
                >
                  Edit
                </button>
              )}
            </div>
          </div>
          {isEditingPrompt && (
            <>
              <textarea
                ref={promptTextareaRef}
                className="chat-prompt-textarea"
                value={systemPromptDraft}
                onChange={(e) => setSystemPromptDraft(e.target.value)}
                placeholder={DEFAULT_SYSTEM_PROMPT}
                rows={4}
                onKeyDown={handlePromptKeyDown}
              />
              <div className="chat-prompt-editor-actions">
                <button
                  type="button"
                  className="btn btn-sm btn-ghost"
                  onClick={() => setSystemPromptDraft(DEFAULT_SYSTEM_PROMPT)}
                >
                  Reset
                </button>
                <div className="chat-prompt-editor-btns">
                  <button
                    type="button"
                    className="btn btn-sm"
                    onClick={() => {
                      setIsEditingPrompt(false);
                      setSystemPromptDraft(activeConversation?.system_prompt ?? DEFAULT_SYSTEM_PROMPT);
                    }}
                    disabled={savingSystemPrompt}
                  >
                    Cancel
                  </button>
                  <button
                    type="button"
                    className="btn btn-sm btn-primary"
                    onClick={handleSaveSystemPrompt}
                    disabled={savingSystemPrompt || !promptHasChanges}
                  >
                    {savingSystemPrompt ? 'Saving…' : 'Save'}
                  </button>
                </div>
              </div>
            </>
          )}
        </section>

        {/* Error banner */}
        {chatError && <div className="chat-error-banner">{chatError}</div>}

        {/* Server stopped banner */}
        {!isServerConnected && (
          <div className="chat-server-stopped-banner">
            <span className="inline-flex items-center gap-2">
              <Icon icon={AlertTriangle} size={16} />
              Server not running — Chat is read-only
            </span>
            {onClose && (
              <button type="button" className="btn btn-sm" onClick={onClose}>
                Close
              </button>
            )}
          </div>
        )}

        {/* Messages area */}
        <div className="chat-messages-surface">
          {messageLoading ? (
            <div className="chat-empty-state">Loading messages…</div>
          ) : (
            <MessageActionsContext.Provider value={messageActionsValue}>
              <ThinkingTimingProvider value={{ timingTracker, currentStreamingAssistantMessageId, tick }}>
                <ThreadPrimitive.Root
                  key={activeConversationId ?? 'thread-root'}
                  className="chat-thread-root"
                >
                  <ThreadPrimitive.Viewport className="chat-viewport" autoScroll>
                    <ThreadPrimitive.Messages
                      components={messageComponents}
                    />
                  <ThreadPrimitive.ScrollToBottom className="chat-scroll-button">
                    Jump to latest
                  </ThreadPrimitive.ScrollToBottom>
                </ThreadPrimitive.Viewport>

                <div className="chat-composer-shell">
                  {isThreadRunning && (
                    <div className="chat-typing-indicator">Assistant is thinking…</div>
                  )}
                  <ComposerPrimitive.Root className="chat-composer-root">
                    <ComposerPrimitive.Input
                      className="chat-composer-input"
                      placeholder={
                        isServerConnected
                          ? 'Type your message. Shift + Enter for newline'
                          : 'Server not connected'
                      }
                      disabled={!isServerConnected}
                    />
                    <div className="chat-composer-actions">
                      {isThreadRunning && (
                        <button
                          type="button"
                          className="btn btn-sm btn-danger"
                          onClick={() => threadRuntime?.cancelRun()}
                          title="Stop generation"
                        >
                          Stop
                        </button>
                      )}
                      <ComposerPrimitive.Send
                        className="btn btn-sm btn-primary"
                        disabled={!isServerConnected}
                      >
                        Send ↵
                      </ComposerPrimitive.Send>
                    </div>
                  </ComposerPrimitive.Root>
                </div>
              </ThreadPrimitive.Root>
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
