import React, { useState, useRef, useEffect, useMemo } from 'react';
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
import ThinkingBlock from './ThinkingBlock';
import './ChatMessagesPanel.css';

const DEFAULT_SYSTEM_PROMPT = 'You are a helpful coding assistant.';

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
  const timestamp = new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
  }).format(message.createdAt ?? new Date());

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
        <ActionBarPrimitive.Copy />
        <ActionBarPrimitive.Edit />
      </ActionBarPrimitive.Root>
    </MessagePrimitive.Root>
  );
};

const SystemMessageBubble: React.FC = () => null;

interface ChatMessagesPanelProps {
  activeConversation: ConversationSummary | null;
  activeConversationId: number | null;
  isServerConnected: boolean;
  onRenameConversation: (title: string) => Promise<void>;
  onClearConversation: () => Promise<void>;
  onExportConversation: () => void;
  onUpdateSystemPrompt: (prompt: string | null) => Promise<void>;
  persistedMessageIds: React.MutableRefObject<Set<string>>;
  syncConversations: (options?: { preferredId?: number | null; silent?: boolean }) => Promise<void>;
  chatError: string | null;
  setChatError: (error: string | null) => void;
}

const ChatMessagesPanel: React.FC<ChatMessagesPanelProps> = ({
  activeConversation,
  activeConversationId,
  isServerConnected,
  onRenameConversation,
  onClearConversation,
  onExportConversation,
  onUpdateSystemPrompt,
  persistedMessageIds,
  syncConversations,
  chatError,
  setChatError,
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
  const promptTextareaRef = useRef<HTMLTextAreaElement | null>(null);

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

  // Persist new messages
  useEffect(() => {
    if (!threadRuntime || !activeConversationId) return;

    const unsubscribe = threadRuntime.subscribe(() => {
      const state = threadRuntime.getState();
      state.messages.forEach((message) => {
        if (persistedMessageIds.current.has(message.id)) return;
        if (message.role === 'assistant' && message.status?.type !== 'complete') return;

        const text = extractMessageText(message);
        if (!text.trim()) return;

        persistedMessageIds.current.add(message.id);
        ChatService.saveMessage({
          conversation_id: activeConversationId,
          role: message.role as ChatMessageDto['role'],
          content: text,
        })
          .then(() => syncConversations({ silent: true }))
          .catch((error) => console.error('Failed to persist message', error));
      });
    });

    return unsubscribe;
  }, [threadRuntime, activeConversationId, persistedMessageIds, syncConversations]);

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
            <ThreadPrimitive.Root
              key={activeConversationId ?? 'thread-root'}
              className="chat-thread-root"
            >
              <ThreadPrimitive.Viewport className="chat-viewport">
                <ThreadPrimitive.Messages
                  components={{
                    AssistantMessage: AssistantMessageBubble,
                    UserMessage: UserMessageBubble,
                    SystemMessage: SystemMessageBubble,
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
          )}
        </div>
      </div>
    </div>
  );
};

export default ChatMessagesPanel;
