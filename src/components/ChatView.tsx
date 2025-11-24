import React, {
  useState,
  useEffect,
  useCallback,
  useMemo,
  useRef,
} from 'react';
import ReactMarkdown from 'react-markdown';
import type { Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import 'highlight.js/styles/github-dark.css';
import {
  AssistantRuntimeProvider,
  ThreadPrimitive,
  ComposerPrimitive,
  MessagePrimitive,
  ActionBarPrimitive,
  useThreadRuntime,
  useThread,
  useMessage,
} from '@assistant-ui/react';
import type { ThreadMessage, ThreadMessageLike } from '@assistant-ui/react';
import { useGglibRuntime, fetchAvailableServers, type ServerInfo } from '../hooks/useGglibRuntime';
import { ChatService, ConversationSummary, ChatMessageDto } from '../services/chat';
import styles from './ChatView.module.css';

interface SyncOptions {
  preferredId?: number | null;
  silent?: boolean;
}

const DEFAULT_CONVERSATION_TITLE = 'New Chat';
const DEFAULT_SYSTEM_PROMPT = 'You are a helpful coding assistant.';

const cx = (...classes: Array<string | false | undefined>) =>
  classes.filter(Boolean).join(' ');

const formatRelativeTime = (iso: string) => {
  const formatter = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
  const date = new Date(iso);
  const diffMinutes = Math.round((date.getTime() - Date.now()) / (1000 * 60));

  if (Math.abs(diffMinutes) < 60) {
    return formatter.format(diffMinutes, 'minute');
  }

  const diffHours = Math.round(diffMinutes / 60);
  if (Math.abs(diffHours) < 24) {
    return formatter.format(diffHours, 'hour');
  }

  const diffDays = Math.round(diffHours / 24);
  return formatter.format(diffDays, 'day');
};

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

const MarkdownMessageContent: React.FC = () => {
  const message = useMessage();
  const text = extractMessageText(message);

  const components: Partial<Components> = {
    table: ({ children }) => (
      <div className={styles.tableWrapper}>
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
          <code className={cx(styles.inlineCode, className)} {...rest}>
            {children}
          </code>
        );
      }
      return (
        <pre className={styles.codeBlock}>
          <code className={className} {...rest}>
            {children}
          </code>
        </pre>
      );
    },
  };

  return (
    <ReactMarkdown
      className={styles.markdownBody}
      remarkPlugins={[remarkGfm]}
      rehypePlugins={[rehypeHighlight]}
      components={components}
    >
      {text || ''}
    </ReactMarkdown>
  );
};

const AssistantMessageBubble: React.FC = () => {
  const message = useMessage();
  const timestamp = new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
  }).format(message.createdAt ?? new Date());

  return (
    <MessagePrimitive.Root className={cx(styles.messageBubble, styles.assistantMessage)}>
      <div className={styles.messageMeta}>
        <div className={styles.messageAvatar}>🤖</div>
        <div>
          <div className={styles.messageAuthor}>Assistant</div>
          <div className={styles.messageTimestamp}>{timestamp}</div>
        </div>
      </div>
      <div className={styles.messageContent}>
        <MarkdownMessageContent />
      </div>
      <ActionBarPrimitive.Root className={styles.messageActions}>
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
    <MessagePrimitive.Root className={cx(styles.messageBubble, styles.userMessage)}>
      <div className={styles.messageMeta}>
        <div className={styles.messageAvatar}>🧑‍💻</div>
        <div>
          <div className={styles.messageAuthor}>You</div>
          <div className={styles.messageTimestamp}>{timestamp}</div>
        </div>
      </div>
      <div className={styles.messageContent}>
        <MarkdownMessageContent />
      </div>
      <ActionBarPrimitive.Root className={styles.messageActions}>
        <ActionBarPrimitive.Copy />
        <ActionBarPrimitive.Edit />
      </ActionBarPrimitive.Root>
    </MessagePrimitive.Root>
  );
};

const SystemMessageBubble: React.FC = () => {
  // System prompts are already surfaced via the dedicated editor card, so skip rendering in-thread.
  return null;
};

interface ChatViewProps {
  onClose: () => void;
}

export const ChatView: React.FC<ChatViewProps> = ({ onClose }) => {
  const [availableServers, setAvailableServers] = useState<ServerInfo[]>([]);
  const [serversLoading, setServersLoading] = useState(false);
  const [selectedServerPort, setSelectedServerPort] = useState<number | undefined>();
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [conversationLoading, setConversationLoading] = useState(true);
  const [activeConversationId, setActiveConversationId] = useState<number | null>(null);
  const [conversationSearch, setConversationSearch] = useState('');
  const [messageLoading, setMessageLoading] = useState(false);
  const [chatError, setChatError] = useState<string | null>(null);
  const [isRenaming, setIsRenaming] = useState(false);
  const [titleDraft, setTitleDraft] = useState('');
  const persistedMessageIds = useRef<Set<string>>(new Set());
  const newChatTriggerRef = useRef<HTMLButtonElement | null>(null);

  const runtime = useGglibRuntime({
    selectedServerPort,
    onError: (error) => setChatError(error.message),
  });

  const syncConversations = useCallback(
    async (options: SyncOptions = {}) => {
      if (!options.silent) {
        setConversationLoading(true);
      }
      try {
        let list = await ChatService.listConversations();
        let preferredId = options.preferredId ?? null;

        if (!list.length) {
          preferredId = await ChatService.createConversation(
            DEFAULT_CONVERSATION_TITLE,
            null,
            DEFAULT_SYSTEM_PROMPT,
          );
          list = await ChatService.listConversations();
        }

        setConversations(list);
        setActiveConversationId((prev) => {
          if (preferredId && list.some((c) => c.id === preferredId)) {
            return preferredId;
          }
          if (prev && list.some((c) => c.id === prev)) {
            return prev;
          }
          return list[0]?.id ?? null;
        });
      } catch (error) {
        setChatError(error instanceof Error ? error.message : String(error));
      } finally {
        if (!options.silent) {
          setConversationLoading(false);
        }
      }
    },
    [],
  );

  useEffect(() => {
    syncConversations();
  }, [syncConversations]);

  const loadServers = useCallback(async () => {
    setServersLoading(true);
    try {
      const servers = await fetchAvailableServers();
      setAvailableServers(servers);
      // Auto-select first server if none selected
      if (!selectedServerPort && servers.length > 0) {
        setSelectedServerPort(servers[0].port);
      }
      // Clear selection if selected server is no longer available
      if (selectedServerPort && !servers.some(s => s.port === selectedServerPort)) {
        setSelectedServerPort(servers.length > 0 ? servers[0].port : undefined);
      }
      // Clear any load errors if servers loaded successfully
      if (chatError?.includes('Load failed') || chatError?.includes('No server selected')) {
        setChatError(null);
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error('Failed to load servers', errorMsg);
      setChatError(`Failed to load servers: ${errorMsg}`);
    } finally {
      setServersLoading(false);
    }
  }, [selectedServerPort, chatError]);

  useEffect(() => {
    loadServers();
    // Poll every 3 seconds to detect server changes
    const interval = setInterval(loadServers, 3000);
    return () => clearInterval(interval);
  }, [loadServers]);

  const activeConversation = useMemo(
    () => conversations.find((conversation) => conversation.id === activeConversationId) ?? null,
    [conversations, activeConversationId],
  );

  useEffect(() => {
    if (activeConversation && !isRenaming) {
      setTitleDraft(activeConversation.title);
    }
  }, [activeConversation, isRenaming]);

  const filteredConversations = useMemo(() => {
    const query = conversationSearch.trim().toLowerCase();
    if (!query) {
      return conversations;
    }
    return conversations.filter((conversation) =>
      conversation.title.toLowerCase().includes(query),
    );
  }, [conversationSearch, conversations]);


  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.modal} onClick={(event) => event.stopPropagation()}>
        <AssistantRuntimeProvider runtime={runtime}>
          <ChatPanel
            onClose={onClose}
            availableServers={availableServers}
            serversLoading={serversLoading}
            selectedServerPort={selectedServerPort}
            conversations={conversations}
            conversationLoading={conversationLoading}
            activeConversationId={activeConversationId}
            setActiveConversationId={setActiveConversationId}
            conversationSearch={conversationSearch}
            setConversationSearch={setConversationSearch}
            messageLoading={messageLoading}
            setMessageLoading={setMessageLoading}
            chatError={chatError}
            setChatError={setChatError}
            isRenaming={isRenaming}
            setIsRenaming={setIsRenaming}
            titleDraft={titleDraft}
            setTitleDraft={setTitleDraft}
            persistedMessageIds={persistedMessageIds}
            syncConversations={syncConversations}
            activeConversation={activeConversation}
            filteredConversations={filteredConversations}
            newChatTriggerRef={newChatTriggerRef}
          />
        </AssistantRuntimeProvider>
      </div>
    </div>
  );
};

interface ChatPanelProps {
  onClose: () => void;
  availableServers: ServerInfo[];
  serversLoading: boolean;
  selectedServerPort: number | undefined;
  conversations: ConversationSummary[];
  conversationLoading: boolean;
  activeConversationId: number | null;
  setActiveConversationId: React.Dispatch<React.SetStateAction<number | null>>;
  conversationSearch: string;
  setConversationSearch: React.Dispatch<React.SetStateAction<string>>;
  messageLoading: boolean;
  setMessageLoading: React.Dispatch<React.SetStateAction<boolean>>;
  chatError: string | null;
  setChatError: React.Dispatch<React.SetStateAction<string | null>>;
  isRenaming: boolean;
  setIsRenaming: React.Dispatch<React.SetStateAction<boolean>>;
  titleDraft: string;
  setTitleDraft: React.Dispatch<React.SetStateAction<string>>;
  persistedMessageIds: React.MutableRefObject<Set<string>>;
  syncConversations: (options?: SyncOptions) => Promise<void>;
  activeConversation: ConversationSummary | null;
  filteredConversations: ConversationSummary[];
  newChatTriggerRef: React.RefObject<HTMLButtonElement>;
}

const ChatPanel: React.FC<ChatPanelProps> = ({
  onClose,
  availableServers,
  serversLoading,
  selectedServerPort,
  conversationLoading,
  activeConversationId,
  setActiveConversationId,
  conversationSearch,
  setConversationSearch,
  messageLoading,
  setMessageLoading,
  chatError,
  setChatError,
  isRenaming,
  setIsRenaming,
  titleDraft,
  setTitleDraft,
  persistedMessageIds,
  syncConversations,
  activeConversation,
  filteredConversations,
  newChatTriggerRef,
}) => {
  const threadRuntime = useThreadRuntime({ optional: true });
  const threadState = useThread({ optional: true });

  const isThreadRunning = threadState?.isRunning ?? false;
  const [isNewConversationModalOpen, setIsNewConversationModalOpen] = useState(false);
  const [newConversationTitle, setNewConversationTitle] = useState(DEFAULT_CONVERSATION_TITLE);
  const [newConversationPrompt, setNewConversationPrompt] = useState(DEFAULT_SYSTEM_PROMPT);
  const [creatingConversation, setCreatingConversation] = useState(false);
  const [isEditingPrompt, setIsEditingPrompt] = useState(false);
  const [systemPromptDraft, setSystemPromptDraft] = useState(DEFAULT_SYSTEM_PROMPT);
  const [savingSystemPrompt, setSavingSystemPrompt] = useState(false);
  const promptTextareaRef = useRef<HTMLTextAreaElement | null>(null);
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const previouslyFocusedElementRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!isEditingPrompt) {
      setSystemPromptDraft(activeConversation?.system_prompt ?? DEFAULT_SYSTEM_PROMPT);
    }
  }, [activeConversation?.system_prompt, isEditingPrompt]);

  useEffect(() => {
    if (isEditingPrompt) {
      promptTextareaRef.current?.focus();
    }
  }, [isEditingPrompt]);

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

  const openNewConversationModal = () => {
    previouslyFocusedElementRef.current = document.activeElement as HTMLElement | null;
    setNewConversationTitle(DEFAULT_CONVERSATION_TITLE);
    setNewConversationPrompt(activeConversation?.system_prompt ?? DEFAULT_SYSTEM_PROMPT);
    setIsNewConversationModalOpen(true);
  };

  const closeNewConversationModal = useCallback(() => {
    if (!creatingConversation) {
      setIsNewConversationModalOpen(false);
      const focusTarget = previouslyFocusedElementRef.current ?? newChatTriggerRef.current;
      window.setTimeout(() => focusTarget?.focus(), 0);
    }
  }, [creatingConversation, newChatTriggerRef]);

  const handleCreateConversation = async () => {
    setCreatingConversation(true);
    try {
      const title = newConversationTitle.trim() || DEFAULT_CONVERSATION_TITLE;
      const systemPrompt = newConversationPrompt.trim() || DEFAULT_SYSTEM_PROMPT;
      const newId = await ChatService.createConversation(title, null, systemPrompt);
      persistedMessageIds.current = new Set();
      threadRuntime?.reset();
      await syncConversations({ preferredId: newId });
      setIsNewConversationModalOpen(false);
    } catch (error) {
      setChatError(error instanceof Error ? error.message : String(error));
    } finally {
      setCreatingConversation(false);
    }
  };

  useEffect(() => {
    if (!isNewConversationModalOpen) return;
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        closeNewConversationModal();
      }
    };
    document.addEventListener('keydown', handleEscape);
    return () => document.removeEventListener('keydown', handleEscape);
  }, [isNewConversationModalOpen, closeNewConversationModal]);

  useEffect(() => {
    if (!isNewConversationModalOpen || !dialogRef.current) {
      return;
    }

    const dialogElement = dialogRef.current;
    const selectors = 'button, [href], input, textarea, select, [tabindex]:not([tabindex="-1"])';
    const getFocusable = () =>
      Array.from(dialogElement.querySelectorAll<HTMLElement>(selectors)).filter(
        (el) =>
          !el.hasAttribute('disabled') &&
          el.getAttribute('aria-hidden') !== 'true' &&
          el.tabIndex !== -1,
      );

    const focusables = getFocusable();
    focusables[0]?.focus();

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Tab') {
        return;
      }

      const focusable = getFocusable();
      if (!focusable.length) {
        return;
      }

      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    dialogElement.addEventListener('keydown', handleKeyDown);
    return () => dialogElement.removeEventListener('keydown', handleKeyDown);
  }, [isNewConversationModalOpen]);

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
        if (cancelled) {
          return;
        }
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
    return () => {
      cancelled = true;
    };
  }, [
    threadRuntime,
    activeConversationId,
    activeConversation?.id,
    activeConversation?.system_prompt,
    activeConversation?.created_at,
    setMessageLoading,
    setChatError,
    persistedMessageIds,
  ]);

  useEffect(() => {
    if (!threadRuntime || !activeConversationId) {
      return;
    }

    const unsubscribe = threadRuntime.subscribe(() => {
      const state = threadRuntime.getState();
      state.messages.forEach((message) => {
        if (persistedMessageIds.current.has(message.id)) {
          return;
        }

        if (message.role === 'assistant' && message.status?.type !== 'complete') {
          return;
        }

        const text = extractMessageText(message);
        if (!text.trim()) {
          return;
        }

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

  const handleDeleteConversation = async (conversationId: number) => {
    const shouldDelete = window.confirm('Delete this conversation? This cannot be undone.');
    if (!shouldDelete) {
      return;
    }
    try {
      await ChatService.deleteConversation(conversationId);
      persistedMessageIds.current = new Set();
      if (activeConversationId === conversationId) {
        threadRuntime?.reset();
      }
      await syncConversations();
    } catch (error) {
      setChatError(error instanceof Error ? error.message : String(error));
    }
  };

  const handleRenameConversation = async () => {
    if (!activeConversation || !titleDraft.trim()) {
      setIsRenaming(false);
      setTitleDraft(activeConversation?.title ?? DEFAULT_CONVERSATION_TITLE);
      return;
    }

    try {
      await ChatService.updateConversationTitle(activeConversation.id, titleDraft.trim());
      setIsRenaming(false);
      await syncConversations({ preferredId: activeConversation.id, silent: true });
    } catch (error) {
      setChatError(error instanceof Error ? error.message : String(error));
    }
  };

  const handleClearConversation = async () => {
    if (!activeConversation) {
      return;
    }
    const confirmed = window.confirm('Start a fresh copy of this conversation?');
    if (!confirmed) {
      return;
    }

    try {
      await ChatService.deleteConversation(activeConversation.id);
      const newId = await ChatService.createConversation(
        activeConversation.title,
        null,
        activeConversation.system_prompt ?? DEFAULT_SYSTEM_PROMPT,
      );
      persistedMessageIds.current = new Set();
      threadRuntime?.reset();
      await syncConversations({ preferredId: newId });
    } catch (error) {
      setChatError(error instanceof Error ? error.message : String(error));
    }
  };

  const handleExportConversation = () => {
    if (!threadRuntime || !activeConversationId) {
      return;
    }
    const exported = threadRuntime.export();
    const blob = new Blob([JSON.stringify(exported, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `conversation-${activeConversationId}.json`;
    anchor.click();
    URL.revokeObjectURL(url);
  };

  const handleStartPromptEdit = () => {
    if (!activeConversation) {
      return;
    }
    setSystemPromptDraft(activeConversation.system_prompt ?? DEFAULT_SYSTEM_PROMPT);
    setIsEditingPrompt(true);
  };

  const handleCancelSystemPromptEdit = () => {
    setIsEditingPrompt(false);
    setSystemPromptDraft(activeConversation?.system_prompt ?? DEFAULT_SYSTEM_PROMPT);
  };

  const handleResetSystemPrompt = () => {
    setSystemPromptDraft(DEFAULT_SYSTEM_PROMPT);
  };

  const handleSaveSystemPrompt = async () => {
    if (!activeConversation) {
      return;
    }
    if (!promptHasChanges) {
      setIsEditingPrompt(false);
      return;
    }

    setSavingSystemPrompt(true);
    try {
      const trimmedPrompt = systemPromptDraft.trim();
      const nextPrompt = trimmedPrompt.length ? trimmedPrompt : null;
      await ChatService.updateConversationSystemPrompt(activeConversation.id, nextPrompt);
      setIsEditingPrompt(false);
      await syncConversations({ preferredId: activeConversation.id, silent: true });
    } catch (error) {
      setChatError(error instanceof Error ? error.message : String(error));
    } finally {
      setSavingSystemPrompt(false);
    }
  };

  const handlePromptEditorKeyDown = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
      event.preventDefault();
      handleSaveSystemPrompt();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      handleCancelSystemPromptEdit();
    }
  };

  return (
    <div className={styles.shell}>
      <aside className={styles.sidebar}>
        <div className={styles.sidebarHeader}>
          <div>
            <p className={styles.sidebarLabel}>Conversations</p>
            <h2 className={styles.sidebarTitle}>History</h2>
          </div>
          <button
            ref={newChatTriggerRef}
            className="btn btn-primary"
            onClick={openNewConversationModal}
          >
            ＋ New Chat
          </button>
        </div>
        <div className={styles.sidebarSearch}>
          <input
            type="search"
            placeholder="Search conversations"
            value={conversationSearch}
            onChange={(event) => setConversationSearch(event.target.value)}
          />
        </div>
        <div className={styles.sidebarList}>
          {conversationLoading ? (
            <div className={styles.emptyState}>Loading conversations…</div>
          ) : filteredConversations.length === 0 ? (
            <div className={styles.emptyState}>No conversations yet.</div>
          ) : (
            filteredConversations.map((conversation) => (
              <button
                key={conversation.id}
                type="button"
                className={cx(
                  styles.conversationItem,
                  conversation.id === activeConversationId && styles.conversationItemActive,
                )}
                aria-pressed={conversation.id === activeConversationId}
                onClick={() => setActiveConversationId(conversation.id)}
              >
                <div>
                  <p className={styles.conversationTitle}>{conversation.title}</p>
                  <p className={styles.conversationTimestamp}>
                    {formatRelativeTime(conversation.updated_at)}
                  </p>
                </div>
                <span
                  role="button"
                  tabIndex={0}
                  aria-label={`Delete conversation ${conversation.title}`}
                  className={styles.conversationDelete}
                  title="Delete conversation"
                  onClick={(event) => {
                    event.preventDefault();
                    event.stopPropagation();
                    handleDeleteConversation(conversation.id);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' || event.key === ' ') {
                      event.preventDefault();
                      event.stopPropagation();
                      handleDeleteConversation(conversation.id);
                    }
                  }}
                >
                  ✕
                </span>
              </button>
            ))
          )}
        </div>
      </aside>
      <section className={styles.chatPane}>
        <header className={styles.chatHeader}>
          <div className={styles.chatTitleGroup}>
                  {isRenaming ? (
                    <input
                      className={styles.titleInput}
                      value={titleDraft}
                      autoFocus
                      onChange={(event) => setTitleDraft(event.target.value)}
                      onBlur={handleRenameConversation}
                      onKeyDown={(event) => {
                        if (event.key === 'Enter') {
                          handleRenameConversation();
                        } else if (event.key === 'Escape') {
                          setIsRenaming(false);
                          setTitleDraft(activeConversation?.title ?? DEFAULT_CONVERSATION_TITLE);
                        }
                      }}
                    />
                  ) : (
                    <h2 className={styles.chatTitle}>
                      {activeConversation?.title || DEFAULT_CONVERSATION_TITLE}
                    </h2>
                  )}
                  <button
                    className={styles.iconButton}
                    title="Rename conversation"
                    onClick={() => setIsRenaming(true)}
                  >
                    ✏️
                  </button>
                  <span className={cx(styles.statusBadge, isThreadRunning && styles.statusBadgeActive)}>
                    {isThreadRunning ? 'Responding…' : 'Idle'}
                  </span>
          </div>
          <div className={styles.chatHeaderActions}>
            {availableServers.length > 0 && selectedServerPort && (
              <div className={styles.serverInfo}>
                <span className={styles.serverLabel}>Server:</span>
                <span className={styles.serverName}>
                  {availableServers.find(s => s.port === selectedServerPort)?.model_name || 'Unknown'}
                </span>
                <span className={styles.serverPort}>:{selectedServerPort}</span>
              </div>
            )}
            <button className={styles.iconButton} onClick={handleClearConversation} title="Restart conversation">
              ↺
            </button>
            <button className={styles.iconButton} onClick={handleExportConversation} title="Export conversation">
              ⤓
            </button>
            <button className={styles.iconButton} onClick={onClose} title="Close chat">
              ✕
            </button>
          </div>
        </header>

        <section className={styles.promptCard}>
          <div className={styles.promptCardHeader}>
            <div>
              <p className={styles.promptLabel}>System prompt</p>
              {!isEditingPrompt && (
                <p className={styles.promptPreview} aria-live="polite" aria-atomic="true">
                  {promptPreview}
                </p>
              )}
            </div>
            <div className={styles.promptActions}>
              {isEditingPrompt ? (
                <span className={styles.promptEditingBadge}>Editing…</span>
              ) : (
                <button
                  type="button"
                  className={cx(styles.promptButton, styles.promptPrimary)}
                  onClick={handleStartPromptEdit}
                  disabled={!activeConversation}
                >
                  Edit prompt
                </button>
              )}
            </div>
          </div>
          {isEditingPrompt && (
            <>
              <textarea
                ref={promptTextareaRef}
                className={styles.promptTextarea}
                value={systemPromptDraft}
                onChange={(event) => setSystemPromptDraft(event.target.value)}
                placeholder={DEFAULT_SYSTEM_PROMPT}
                rows={5}
                onKeyDown={handlePromptEditorKeyDown}
              />
              <div className={styles.promptEditorActions}>
                <button
                  type="button"
                  className={styles.promptResetButton}
                  onClick={handleResetSystemPrompt}
                >
                  Reset to default
                </button>
                <div className={styles.promptEditorActionGroup}>
                  <button
                    type="button"
                    className={styles.promptButton}
                    onClick={handleCancelSystemPromptEdit}
                    disabled={savingSystemPrompt}
                  >
                    Cancel
                  </button>
                  <button
                    type="button"
                    className={cx(styles.promptButton, styles.promptPrimary)}
                    onClick={handleSaveSystemPrompt}
                    disabled={savingSystemPrompt || !promptHasChanges}
                  >
                    {savingSystemPrompt ? 'Saving…' : 'Save prompt'}
                  </button>
                </div>
              </div>
            </>
          )}
        </section>

        {chatError && <div className={styles.errorBanner}>{chatError}</div>}

        {!serversLoading && availableServers.length === 0 && (
                <div className={styles.noServersBanner}>
                  <p><strong>No models are currently being served.</strong></p>
                  <p>Please serve a model from the <strong>Serve</strong> tab in the GUI or run:</p>
                  <code>gglib serve &lt;model-name-or-id&gt;</code>
                </div>
        )}

        <div className={styles.chatSurface}>
          {messageLoading ? (
            <div className={styles.emptyState}>Loading messages…</div>
          ) : (
            <ThreadPrimitive.Root
              key={activeConversationId ?? 'thread-root'}
              className={styles.threadRoot}
            >
                    <ThreadPrimitive.Viewport className={styles.viewport}>
                      <ThreadPrimitive.Messages
                        components={{
                          AssistantMessage: AssistantMessageBubble,
                          UserMessage: UserMessageBubble,
                          SystemMessage: SystemMessageBubble,
                        }}
                      />
                      <ThreadPrimitive.ScrollToBottom className={styles.scrollButton}>
                        Jump to latest
                      </ThreadPrimitive.ScrollToBottom>
                    </ThreadPrimitive.Viewport>



                    <div className={styles.composerShell}>
                      {isThreadRunning && <div className={styles.typingIndicator}>Assistant is thinking…</div>}
                      <ComposerPrimitive.Root className={styles.composerRoot}>
                        <ComposerPrimitive.Input
                          className={styles.composerInput}
                          placeholder={availableServers.length === 0 ? "Serve a model to start chatting" : "Type your message. Shift + Enter for newline"}
                          disabled={availableServers.length === 0}
                        />
                        <div className={styles.composerActions}>
                          {isThreadRunning && (
                            <button
                              type="button"
                              className={styles.stopButton}
                              onClick={() => threadRuntime?.cancelRun()}
                              title="Stop generation"
                            >
                              Stop
                            </button>
                          )}
                          <ComposerPrimitive.Send 
                            className={styles.sendButton}
                            disabled={availableServers.length === 0}
                          >
                            Send ↵
                          </ComposerPrimitive.Send>
                        </div>
                      </ComposerPrimitive.Root>
                    </div>
                  </ThreadPrimitive.Root>
                )}
              </div>
            </section>
            {isNewConversationModalOpen && (
              <div
                className={styles.dialogOverlay}
                role="dialog"
                aria-modal="true"
                aria-labelledby="new-chat-dialog-title"
                onClick={closeNewConversationModal}
              >
                <div
                  className={styles.dialog}
                  ref={dialogRef}
                  onClick={(event) => event.stopPropagation()}
                >
                  <h3 id="new-chat-dialog-title" className={styles.dialogTitle}>Start a new chat</h3>
                  <label className={styles.dialogLabel}>
                    Title
                    <input
                      className={styles.dialogInput}
                      value={newConversationTitle}
                      onChange={(event) => setNewConversationTitle(event.target.value)}
                      placeholder="New Chat"
                    />
                  </label>
                  <label className={styles.dialogLabel}>
                    System Prompt
                    <textarea
                      className={styles.dialogTextarea}
                      value={newConversationPrompt}
                      onChange={(event) => setNewConversationPrompt(event.target.value)}
                      placeholder={DEFAULT_SYSTEM_PROMPT}
                      rows={5}
                    />
                  </label>
                  <p className={styles.dialogHint}>
                    The system prompt steers the assistant’s behavior for the entire conversation.
                  </p>
                  <div className={styles.dialogActions}>
                    <button
                      type="button"
                      className={styles.dialogButton}
                      onClick={closeNewConversationModal}
                      disabled={creatingConversation}
                    >
                      Cancel
                    </button>
                    <button
                      type="button"
                      className={cx(styles.dialogButton, styles.dialogPrimary)}
                      onClick={handleCreateConversation}
                      disabled={creatingConversation}
                    >
                      {creatingConversation ? 'Creating…' : 'Create chat'}
                    </button>
                  </div>
                </div>
              </div>
            )}
          </div>
  );
};
