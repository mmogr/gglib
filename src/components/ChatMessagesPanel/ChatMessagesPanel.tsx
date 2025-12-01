import React, { useState, useRef, useEffect, useMemo, useCallback, createContext, useContext } from 'react';
import ReactMarkdown from 'react-markdown';
import type { Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import 'highlight.js/styles/github-dark.css';
import {
  ThreadPrimitive,
  ComposerPrimitive,
  MessagePrimitive,
  ActionBarPrimitive,
  useThreadRuntime,
  useThread,
  useMessage,
} from '@assistant-ui/react';
import type { ThreadMessage, ThreadMessageLike } from '@assistant-ui/react';
import { ChatService, ConversationSummary, ChatMessageDto } from '../../services/chat';
import { parseThinkingContent } from '../../utils/thinkingParser';
import type { ToastType } from '../Toast';
import ThinkingBlock from './ThinkingBlock';
import { ConfirmDeleteModal } from './ConfirmDeleteModal';
import './ChatMessagesPanel.css';

const DEFAULT_SYSTEM_PROMPT = 'You are a helpful coding assistant.';

// Context for message actions (delete) - allows child message bubbles to trigger actions
interface MessageActionsContextValue {
  onDeleteMessage: (runtimeMessageId: string) => void;
}
const MessageActionsContext = createContext<MessageActionsContextValue | null>(null);

// Extract database ID from runtime message ID (e.g., "db-123" -> 123)
const extractDbId = (runtimeId: string): number | null => {
  const match = runtimeId.match(/^db-(\d+)$/);
  return match ? parseInt(match[1], 10) : null;
};

const cx = (...classes: Array<string | false | undefined>) =>
  classes.filter(Boolean).join(' ');

const extractMessageText = (message: ThreadMessage): string => {
  return message.content
    .map((part) => {
      if (typeof part === 'string') {
        return part;
      }
      if ('text' in part && part.text) {
        return part.text;
      }
      if (part.type === 'tool-call') {
        return `${part.toolName}(${part.argsText ?? ''})`;
      }
      return '';
    })
    .filter(Boolean)
    .join('\n\n');
};

// Markdown rendering component
const MarkdownMessageContent: React.FC<{ text?: string }> = ({ text: propText }) => {
  const message = useMessage();
  const text = propText ?? extractMessageText(message);

  const components: Partial<Components> = {
    table: ({ children }) => (
      <div className="chat-table-wrapper">
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
          <code className={cx('chat-inline-code', className)} {...rest}>
            {children}
          </code>
        );
      }
      return (
        <pre className="chat-code-block">
          <code className={className} {...rest}>
            {children}
          </code>
        </pre>
      );
    },
  };

  return (
    <ReactMarkdown
      className="chat-markdown-body"
      remarkPlugins={[remarkGfm]}
      rehypePlugins={[rehypeHighlight]}
      components={components}
    >
      {text || ''}
    </ReactMarkdown>
  );
};

// Message bubble components
const AssistantMessageBubble: React.FC = () => {
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

const UserMessageBubble: React.FC = () => {
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

const SystemMessageBubble: React.FC = () => null;

// EditComposer - shown when user clicks Edit on their message
const EditComposer: React.FC = () => {
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
  persistedMessageIds: React.MutableRefObject<Set<string>>;
  syncConversations: (options?: { preferredId?: number | null; silent?: boolean }) => Promise<void>;
  chatError: string | null;
  setChatError: (error: string | null) => void;
  showToast: (message: string, type?: ToastType, duration?: number) => void;
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
  persistedMessageIds,
  syncConversations,
  chatError,
  setChatError,
  showToast,
}) => {
  const threadRuntime = useThreadRuntime({ optional: true });
  const threadState = useThread({ optional: true });
  const isThreadRunning = threadState?.isRunning ?? false;

  const [isRenaming, setIsRenaming] = useState(false);
  const [titleDraft, setTitleDraft] = useState('');
  const [messageLoading, setMessageLoading] = useState(false);
  const [isEditingPrompt, setIsEditingPrompt] = useState(false);
  const [systemPromptDraft, setSystemPromptDraft] = useState(DEFAULT_SYSTEM_PROMPT);
  const [savingSystemPrompt, setSavingSystemPrompt] = useState(false);
  const [isGeneratingTitle, setIsGeneratingTitle] = useState(false);
  const hasAutoGeneratedTitleRef = useRef(false);
  const promptTextareaRef = useRef<HTMLTextAreaElement | null>(null);

  // Delete modal state
  const [deleteModalOpen, setDeleteModalOpen] = useState(false);
  const [deleteTargetId, setDeleteTargetId] = useState<string | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);

  // Position tracking: maps runtime message index -> DB message ID
  // Used to detect edits and calculate cascade delete counts
  const dbIdByPosition = useRef<Map<number, number>>(new Map());
  
  // Race condition protection for persist operations
  const isPersisting = useRef(false);

  // Sync title draft with active conversation
  useEffect(() => {
    if (activeConversation && !isRenaming) {
      setTitleDraft(activeConversation.title);
    }
  }, [activeConversation, isRenaming]);

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
    setIsGeneratingTitle(false);
    hasAutoGeneratedTitleRef.current = false;
  }, [activeConversationId]);

  const promptPreview = useMemo(
    () => activeConversation?.system_prompt?.trim() || DEFAULT_SYSTEM_PROMPT,
    [activeConversation?.system_prompt],
  );

  const promptHasChanges = useMemo(
    () => systemPromptDraft.trim() !== promptPreview,
    [systemPromptDraft, promptPreview],
  );

  // Load messages when conversation changes
  useEffect(() => {
    if (!threadRuntime || !activeConversationId) {
      return;
    }
    let cancelled = false;
    setMessageLoading(true);
    setChatError(null);

    const hydrate = async () => {
      try {
        const messages = await ChatService.getMessages(activeConversationId);
        if (cancelled) return;

        const prompt = activeConversation?.system_prompt?.trim();
        const systemPromptMessage: ThreadMessageLike[] = prompt && activeConversation
          ? [{
              id: `system-${activeConversation.id}`,
              role: 'system',
              content: [{ type: 'text' as const, text: prompt }],
              createdAt: new Date(activeConversation.created_at),
            }]
          : [];

        const initialMessages: ThreadMessageLike[] = [
          ...systemPromptMessage,
          ...messages.map<ThreadMessageLike>((message) => ({
            id: `db-${message.id}`,
            role: message.role,
            content: message.content,
            createdAt: new Date(message.created_at),
          })),
        ];

        // Build position -> DB ID mapping for edit detection and delete counting
        // Position 0 may be system message, so we track from the actual DB messages
        dbIdByPosition.current.clear();
        const systemOffset = systemPromptMessage.length;
        messages.forEach((msg, idx) => {
          dbIdByPosition.current.set(systemOffset + idx, msg.id);
        });

        const seededIds = initialMessages
          .map((msg) => msg.id)
          .filter((value): value is string => Boolean(value));
        persistedMessageIds.current = new Set(seededIds);
        threadRuntime.reset(initialMessages);
      } catch (error) {
        if (!cancelled) {
          setChatError(error instanceof Error ? error.message : String(error));
        }
      } finally {
        if (!cancelled) {
          setMessageLoading(false);
        }
      }
    };

    hydrate();
    return () => { cancelled = true; };
  }, [
    threadRuntime,
    activeConversationId,
    activeConversation?.id,
    activeConversation?.system_prompt,
    activeConversation?.created_at,
    setChatError,
    persistedMessageIds,
  ]);

  // Persist new messages and handle edit detection
  useEffect(() => {
    if (!threadRuntime || !activeConversationId) return;

    const unsubscribe = threadRuntime.subscribe(async () => {
      // Prevent concurrent persist operations
      if (isPersisting.current) return;
      
      const state = threadRuntime.getState();
      const messages = state.messages;
      
      for (let i = 0; i < messages.length; i++) {
        const message = messages[i];
        
        if (persistedMessageIds.current.has(message.id)) continue;
        if (message.role === 'assistant' && message.status?.type !== 'complete') continue;
        if (message.role === 'system') continue; // System messages handled separately

        const text = extractMessageText(message);
        if (!text.trim()) continue;

        isPersisting.current = true;
        
        try {
          // Check if this is an edit: a new message at a position that already has a DB entry
          // This happens when LocalRuntime creates a new branch from an edit
          if (message.role === 'user' && dbIdByPosition.current.has(i)) {
            const existingDbId = dbIdByPosition.current.get(i)!;
            
            // Cascade delete from this position onwards in DB
            await ChatService.deleteMessage(existingDbId);
            
            // Clear stale position mappings from this point forward
            for (const [pos] of dbIdByPosition.current) {
              if (pos >= i) {
                dbIdByPosition.current.delete(pos);
              }
            }
          }

          // Save the new message
          const newDbId = await ChatService.saveMessage({
            conversation_id: activeConversationId,
            role: message.role as ChatMessageDto['role'],
            content: text,
          });
          
          // Update position mapping for the new message
          dbIdByPosition.current.set(i, newDbId);
          persistedMessageIds.current.add(message.id);
          
          await syncConversations({ silent: true });
        } catch (error) {
          console.error('Failed to persist message', error);
        } finally {
          isPersisting.current = false;
        }
      }
    });

    return unsubscribe;
  }, [threadRuntime, activeConversationId, persistedMessageIds, syncConversations]);

  // Generate chat title using AI
  const handleGenerateTitle = useCallback(async (skipConfirmation = false) => {
    if (!activeConversation || !activeConversationId || !serverPort) return;

    // Show confirmation if overwriting an existing non-default title
    const isDefaultTitle = activeConversation.title === 'New Chat' || !activeConversation.title;
    if (!skipConfirmation && !isDefaultTitle) {
      const confirmed = window.confirm('Replace current title with AI-generated one?');
      if (!confirmed) return;
    }

    setIsGeneratingTitle(true);
    try {
      // Fetch fresh messages from the database
      const messages = await ChatService.getMessages(activeConversationId);
      
      if (messages.length === 0) {
        showToast('Cannot generate title for empty conversation', 'warning');
        return;
      }

      const generatedTitle = await ChatService.generateChatTitle(
        serverPort,
        messages,
        titleGenerationPrompt,
      );

      await onRenameConversation(generatedTitle);
      showToast('Title generated successfully', 'success');
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to generate title';
      showToast(message, 'error');
      console.error('Title generation failed:', error);
    } finally {
      setIsGeneratingTitle(false);
    }
  }, [activeConversation, activeConversationId, serverPort, titleGenerationPrompt, onRenameConversation, showToast]);

  // Auto-generate title on first assistant response
  useEffect(() => {
    if (!threadRuntime || !activeConversation || !activeConversationId || !serverPort) return;
    if (hasAutoGeneratedTitleRef.current) return;

    // Only auto-generate if title is still the default
    const isDefaultTitle = activeConversation.title === 'New Chat' || !activeConversation.title;
    if (!isDefaultTitle) {
      hasAutoGeneratedTitleRef.current = true; // Don't try again for this conversation
      return;
    }

    const unsubscribe = threadRuntime.subscribe(() => {
      const state = threadRuntime.getState();
      
      // Check if we have at least one completed assistant message
      const hasCompletedAssistantMessage = state.messages.some(
        (msg) => msg.role === 'assistant' && msg.status?.type === 'complete'
      );

      // Also need at least one user message for context
      const hasUserMessage = state.messages.some((msg) => msg.role === 'user');

      if (hasCompletedAssistantMessage && hasUserMessage && !hasAutoGeneratedTitleRef.current) {
        hasAutoGeneratedTitleRef.current = true;
        // Delay slightly to ensure message is persisted first
        setTimeout(() => {
          handleGenerateTitle(true); // Skip confirmation for auto-generate
        }, 500);
      }
    });

    return unsubscribe;
  }, [threadRuntime, activeConversation, activeConversationId, serverPort, handleGenerateTitle]);

  // Handlers
  const handleRename = async () => {
    if (!titleDraft.trim()) {
      setIsRenaming(false);
      setTitleDraft(activeConversation?.title ?? '');
      return;
    }
    await onRenameConversation(titleDraft.trim());
    setIsRenaming(false);
  };

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

  // Calculate subsequent message count for delete modal
  const getSubsequentMessageCount = useCallback((runtimeMessageId: string): number => {
    if (!threadRuntime) return 1;
    
    const state = threadRuntime.getState();
    const messageIndex = state.messages.findIndex((m) => m.id === runtimeMessageId);
    if (messageIndex === -1) return 1;
    
    // Count messages from this position to the end (excluding system messages)
    let count = 0;
    for (let i = messageIndex; i < state.messages.length; i++) {
      if (state.messages[i].role !== 'system') {
        count++;
      }
    }
    return count;
  }, [threadRuntime]);

  // Handle delete message request from UserMessageBubble
  const handleDeleteMessage = useCallback((runtimeMessageId: string) => {
    setDeleteTargetId(runtimeMessageId);
    setDeleteModalOpen(true);
  }, []);

  // Confirm and execute delete
  const handleConfirmDelete = useCallback(async () => {
    if (!deleteTargetId || !threadRuntime || !activeConversationId) return;
    
    setIsDeleting(true);
    try {
      // Find the DB ID from the runtime message ID
      // First try direct extraction (for hydrated messages with db-xxx format)
      let dbId = extractDbId(deleteTargetId);
      
      // If not found, look up by position (for newly created messages)
      if (!dbId) {
        const state = threadRuntime.getState();
        const messages = state.messages;
        const position = messages.findIndex(m => m.id === deleteTargetId);
        if (position >= 0) {
          dbId = dbIdByPosition.current.get(position) ?? null;
        }
      }
      
      if (dbId) {
        // Delete from database (cascade deletes subsequent)
        await ChatService.deleteMessage(dbId);
      } else {
        console.warn('Could not find DB ID for message:', deleteTargetId);
      }
      
      // Reload messages from DB and reset runtime
      const messages = await ChatService.getMessages(activeConversationId);
      
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

      // Update persisted IDs and reset runtime
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
  }, [deleteTargetId, threadRuntime, activeConversationId, activeConversation, persistedMessageIds, syncConversations, showToast]);

  // Cancel delete
  const handleCancelDelete = useCallback(() => {
    setDeleteModalOpen(false);
    setDeleteTargetId(null);
  }, []);

  // Context value for message actions
  const messageActionsValue = useMemo<MessageActionsContextValue>(
    () => ({ onDeleteMessage: handleDeleteMessage }),
    [handleDeleteMessage]
  );

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
              onBlur={handleRename}
              onKeyDown={(e) => {
                if (e.key === 'Enter') handleRename();
                else if (e.key === 'Escape') {
                  setIsRenaming(false);
                  setTitleDraft(activeConversation?.title ?? '');
                }
              }}
            />
          ) : (
            <h2 className="chat-title">{activeConversation?.title || 'New Chat'}</h2>
          )}
          <button
            className="icon-btn icon-btn-sm"
            title="Rename conversation"
            onClick={() => setIsRenaming(true)}
          >
            ✏️
          </button>
          <button
            className={cx('icon-btn icon-btn-sm', isGeneratingTitle && 'generating')}
            title={serverPort ? 'Generate title with AI' : 'Start a server to generate titles'}
            onClick={() => handleGenerateTitle()}
            disabled={!serverPort || isGeneratingTitle || isThreadRunning}
          >
            {isGeneratingTitle ? (
              <span className="icon-btn-spinner" aria-label="Generating title…" />
            ) : (
              '✨'
            )}
          </button>
          <span className={cx('chat-status-badge', isThreadRunning && 'active')}>
            {isThreadRunning ? 'Responding…' : 'Idle'}
          </span>
        </div>
        <div className="chat-header-actions">
          <button className="icon-btn icon-btn-sm" onClick={onClearConversation} title="Restart conversation">
            ↺
          </button>
          <button className="icon-btn icon-btn-sm" onClick={onExportConversation} title="Export conversation">
            ⤓
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

        {/* Messages area */}
        <div className="chat-messages-surface">
          {messageLoading ? (
            <div className="chat-empty-state">Loading messages…</div>
          ) : (
            <MessageActionsContext.Provider value={messageActionsValue}>
              <ThreadPrimitive.Root
                key={activeConversationId ?? 'thread-root'}
                className="chat-thread-root"
              >
                <ThreadPrimitive.Viewport className="chat-viewport" autoScroll>
                  <ThreadPrimitive.Messages
                    components={{
                      AssistantMessage: AssistantMessageBubble,
                      UserMessage: UserMessageBubble,
                      SystemMessage: SystemMessageBubble,
                      EditComposer: EditComposer,
                    }}
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
