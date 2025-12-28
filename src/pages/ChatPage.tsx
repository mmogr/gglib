import { useState, useEffect, useCallback, useRef } from 'react';
import { type ChatPageTabId } from './chatTabs';
import { AssistantRuntimeProvider } from '@assistant-ui/react';
import { ConversationListPanel } from '../components/ConversationListPanel';
import { ChatMessagesPanel } from '../components/ChatMessagesPanel';
import { ConsoleInfoPanel } from '../components/ConsoleInfoPanel';
import { ConsoleLogPanel } from '../components/ConsoleLogPanel';
import { GenericToolUI, TimeToolUI } from '../components/ToolUI';
import { Button } from '../components/ui/Button';
import { Input } from '../components/ui/Input';
import { Textarea } from '../components/ui/Textarea';
import { useGglibRuntime, DEFAULT_SYSTEM_PROMPT as RUNTIME_DEFAULT_SYSTEM_PROMPT } from '../hooks/useGglibRuntime';
import { useChatPersistence } from '../hooks/useChatPersistence';
import { useSettings } from '../hooks/useSettings';
import { useToastContext } from '../contexts/ToastContext';
import { useServerState } from '../services/serverEvents';
import {
  listConversations,
  createConversation,
  deleteConversation,
  updateConversationTitle,
  updateConversationSystemPrompt,
  DEFAULT_TITLE_GENERATION_PROMPT,
} from '../services/clients/chat';
import type { ConversationSummary } from '../services/clients/chat';
import './ChatPage.css';

const DEFAULT_CONVERSATION_TITLE = 'New Chat';
// Base prompt stored on the conversation by default.
// Tool availability is handled dynamically at request-time.
const DEFAULT_SYSTEM_PROMPT = RUNTIME_DEFAULT_SYSTEM_PROMPT;

interface ChatPageProps {
  serverPort: number;
  modelId: number;
  modelName: string;
  contextLength?: number;
  serverStartTime?: number; // Unix timestamp in seconds
  initialView?: 'chat' | 'console'; // Which view to show initially
  onClose: () => Promise<void>; // Stops server and exits
}

export default function ChatPage({
  serverPort,
  modelId,
  modelName,
  contextLength,
  serverStartTime,
  initialView = 'chat',
  onClose,
}: ChatPageProps) {
  // Tab state
  const [activeTab, setActiveTab] = useState<ChatPageTabId>(initialView);
  
  // Conversation state
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [conversationLoading, setConversationLoading] = useState(true);
  const [activeConversationId, setActiveConversationId] = useState<number | null>(null);
  const [conversationSearch, setConversationSearch] = useState('');
  const [chatError, setChatError] = useState<string | null>(null);
  
  // New conversation modal state
  const [isNewConversationModalOpen, setIsNewConversationModalOpen] = useState(false);
  const [newConversationTitle, setNewConversationTitle] = useState(DEFAULT_CONVERSATION_TITLE);
  const [newConversationPrompt, setNewConversationPrompt] = useState(DEFAULT_SYSTEM_PROMPT);
  const [creatingConversation, setCreatingConversation] = useState(false);
  
  // Message persistence tracking
  const persistedMessageIds = useRef<Set<string>>(new Set());
  
  // Panel width for resize
  const [leftPanelWidth, setLeftPanelWidth] = useState(35);
  const layoutRef = useRef<HTMLDivElement>(null);
  const isDraggingRef = useRef(false);

  // Toast notifications
  const { showToast } = useToastContext();

  // Settings for title generation prompt and agent loop
  const { settings } = useSettings();
  const titleGenerationPrompt = settings?.title_generation_prompt || DEFAULT_TITLE_GENERATION_PROMPT;
  const maxToolIterations = settings?.max_tool_iterations ?? undefined;
  const maxStagnationSteps = settings?.max_stagnation_steps ?? undefined;

  // Runtime - now with external message state
  const { runtime, messages, setMessages, timingTracker, currentStreamingAssistantMessageId } = useGglibRuntime({
    conversationId: activeConversationId ?? undefined,
    selectedServerPort: serverPort,
    onError: (error) => setChatError(error.message),
    maxToolIterations,
    maxStagnationSteps,
  });

  // Server state from registry - derives isServerRunning reactively
  // Note: If serverState is null (no event received yet), we assume running
  // because ChatPage is only opened when a server is already running
  const serverState = useServerState(modelId);
  const isServerRunning = serverState?.status !== 'stopped' && serverState?.status !== 'crashed';

  // Track previous status for transition-only toast
  const prevStatusRef = useRef(serverState?.status);

  // Show toast only on status transition to stopped/crashed (not on remount)
  useEffect(() => {
    const prev = prevStatusRef.current;
    const next = serverState?.status;

    if (prev !== next && (next === 'stopped' || next === 'crashed')) {
      showToast(
        next === 'crashed'
          ? 'Server crashed. Chat is now read-only.'
          : 'Server stopped. Chat is now read-only.',
        'warning'
      );
    }

    prevStatusRef.current = next;
  }, [serverState?.status, showToast]);

  // Sync conversations
  const syncConversations = useCallback(
    async (options: { preferredId?: number | null; silent?: boolean } = {}) => {
      if (!options.silent) {
        setConversationLoading(true);
      }
      try {
        let list = await listConversations();
        let preferredId = options.preferredId ?? null;

        // Create default conversation if none exist
        if (!list.length) {
          preferredId = await createConversation(
            DEFAULT_CONVERSATION_TITLE,
            null,
            DEFAULT_SYSTEM_PROMPT,
          );
          list = await listConversations();
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

  // Load conversations on mount
  useEffect(() => {
    syncConversations();
  }, [syncConversations]);

  // Get active conversation
  const activeConversation = conversations.find((c) => c.id === activeConversationId) ?? null;

  // Hydrate messages when conversation changes
  // Note: Message persistence is handled by useChatPersistence below
  // This effect just clears the message state when switching to a new conversation
  useEffect(() => {
    if (!activeConversationId) {
      // New conversation - clear messages
      setMessages([]);
      persistedMessageIds.current.clear();
    }
  }, [activeConversationId, setMessages]);

  // Persistence hook - handles hydration and saving
  useChatPersistence({
    activeConversationId,
    systemPrompt: activeConversation?.system_prompt,
    conversationCreatedAt: activeConversation?.created_at,
    messages,
    setMessages,
    syncConversations,
    setChatError,
    timingTracker,
  });

  // Handle resize
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    isDraggingRef.current = true;
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  }, []);

  useEffect(() => {
    let rafId: number | null = null;

    const handleMouseMove = (e: MouseEvent) => {
      if (!isDraggingRef.current || !layoutRef.current) return;

      if (rafId !== null) {
        cancelAnimationFrame(rafId);
      }

      rafId = requestAnimationFrame(() => {
        if (!layoutRef.current) return;
        const rect = layoutRef.current.getBoundingClientRect();
        const x = e.clientX - rect.left;
        const percentage = (x / rect.width) * 100;
        const newLeftWidth = Math.max(20, Math.min(50, percentage));
        setLeftPanelWidth(newLeftWidth);
      });
    };

    const handleMouseUp = () => {
      if (rafId !== null) {
        cancelAnimationFrame(rafId);
      }
      isDraggingRef.current = false;
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);

    return () => {
      if (rafId !== null) {
        cancelAnimationFrame(rafId);
      }
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, []);

  // Conversation handlers
  const handleDeleteConversation = async (conversationId: number) => {
    const shouldDelete = window.confirm('Delete this conversation? This cannot be undone.');
    if (!shouldDelete) return;

    try {
      await deleteConversation(conversationId);
      persistedMessageIds.current = new Set();
      await syncConversations();
    } catch (error) {
      setChatError(error instanceof Error ? error.message : String(error));
    }
  };

  const handleNewConversation = () => {
    setNewConversationTitle(DEFAULT_CONVERSATION_TITLE);
    setNewConversationPrompt(activeConversation?.system_prompt ?? DEFAULT_SYSTEM_PROMPT);
    setIsNewConversationModalOpen(true);
  };

  const handleCreateConversation = async () => {
    setCreatingConversation(true);
    try {
      const title = newConversationTitle.trim() || DEFAULT_CONVERSATION_TITLE;
      const systemPrompt = newConversationPrompt.trim() || DEFAULT_SYSTEM_PROMPT;
      const newId = await createConversation(title, null, systemPrompt);
      persistedMessageIds.current = new Set();
      
      // Insert new conversation locally before selecting it
      const newConversation: ConversationSummary = {
        id: newId,
        title,
        model_id: null,
        system_prompt: systemPrompt,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      };
      setConversations(prev => [newConversation, ...prev]);
      
      // Select the new conversation
      setActiveConversationId(newId);
      setIsNewConversationModalOpen(false);
      setActiveTab('chat');
      
      // Reconcile with server ordering in background
      void syncConversations({ preferredId: newId, silent: true });
    } catch (error) {
      setChatError(error instanceof Error ? error.message : String(error));
    } finally {
      setCreatingConversation(false);
    }
  };

  const handleRenameConversation = async (title: string) => {
    if (!activeConversation) return;
    try {
      console.debug('[ChatPage] handleRenameConversation called:', {
        conversationId: activeConversation.id,
        title,
        titleLength: title.length,
      });
      await updateConversationTitle(activeConversation.id, title);
      console.debug('[ChatPage] updateConversationTitle succeeded, syncing...');
      await syncConversations({ preferredId: activeConversation.id, silent: true });
      console.debug('[ChatPage] handleRenameConversation completed successfully');
    } catch (error: any) {
      console.error('[ChatPage] handleRenameConversation failed:', {
        error,
        errorType: typeof error,
        errorName: error?.name,
        errorMessage: error?.message,
        errorStack: error?.stack,
        errorConstructor: error?.constructor?.name,
        isDOMException: error instanceof DOMException,
        isError: error instanceof Error,
        fullError: JSON.stringify(error, Object.getOwnPropertyNames(error)),
      });
      setChatError(error instanceof Error ? error.message : String(error));
    }
  };

  const handleClearConversation = async () => {
    if (!activeConversation) return;
    const confirmed = window.confirm('Start a fresh copy of this conversation?');
    if (!confirmed) return;

    try {
      await deleteConversation(activeConversation.id);
      const newId = await createConversation(
        activeConversation.title,
        null,
        activeConversation.system_prompt ?? DEFAULT_SYSTEM_PROMPT,
      );
      persistedMessageIds.current = new Set();
      await syncConversations({ preferredId: newId });
    } catch (error) {
      setChatError(error instanceof Error ? error.message : String(error));
    }
  };

  const handleExportConversation = () => {
    // Export would require access to runtime - simplified version
    if (!activeConversation) return;
    // For now, just export conversation metadata
    const data = { conversation: activeConversation };
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `conversation-${activeConversation.id}.json`;
    anchor.click();
    URL.revokeObjectURL(url);
  };

  const handleUpdateSystemPrompt = async (prompt: string | null) => {
    if (!activeConversation) return;
    try {
      await updateConversationSystemPrompt(activeConversation.id, prompt);
      await syncConversations({ preferredId: activeConversation.id, silent: true });
    } catch (error) {
      setChatError(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <div className="chat-page">
      {/* Chat Tab Content - always mounted, hidden when not active */}
      <AssistantRuntimeProvider runtime={runtime}>
        {/* Tool UI Components - render tool calls in chat messages */}
        <TimeToolUI />
        <GenericToolUI />
        
        <div
          ref={activeTab === 'chat' ? layoutRef : undefined}
          className={`chat-page-layout ${activeTab !== 'chat' ? 'chat-page-layout--hidden' : ''}`}
          style={{ gridTemplateColumns: `${leftPanelWidth}% ${100 - leftPanelWidth}%` }}
        >
          {/* Left Panel: Conversation List */}
          <div className="grid-panel-container">
            <ConversationListPanel
              conversations={conversations}
              activeConversationId={activeConversationId}
              onSelectConversation={setActiveConversationId}
              onDeleteConversation={handleDeleteConversation}
              onNewConversation={handleNewConversation}
              searchQuery={conversationSearch}
              onSearchChange={setConversationSearch}
              loading={conversationLoading}
              modelName={modelName}
              onClose={onClose}
              activeTab={activeTab}
              onTabChange={setActiveTab}
            />
            <div className="resize-handle" onMouseDown={handleMouseDown} />
          </div>

          {/* Right Panel: Chat Messages */}
          <div className="grid-panel-container">
            <ChatMessagesPanel
              key={activeConversationId ?? "none"}
              activeConversation={activeConversation}
              activeConversationId={activeConversationId}
              isServerConnected={isServerRunning}
              serverPort={serverPort}
              titleGenerationPrompt={titleGenerationPrompt}
              onRenameConversation={handleRenameConversation}
              onClearConversation={handleClearConversation}
              onExportConversation={handleExportConversation}
              onUpdateSystemPrompt={handleUpdateSystemPrompt}
              onClose={onClose}
              persistedMessageIds={persistedMessageIds}
              syncConversations={syncConversations}
              chatError={chatError}
              setChatError={setChatError}
              showToast={showToast}
              timingTracker={timingTracker}
              currentStreamingAssistantMessageId={currentStreamingAssistantMessageId}
            />
          </div>
        </div>
      </AssistantRuntimeProvider>

      {/* Console Tab Content - always mounted, hidden when not active */}
      <div
        ref={activeTab === 'console' ? layoutRef : undefined}
        className={`chat-page-layout ${activeTab !== 'console' ? 'chat-page-layout--hidden' : ''}`}
        style={{ gridTemplateColumns: `${leftPanelWidth}% ${100 - leftPanelWidth}%` }}
      >
        {/* Left Panel: Server Info */}
        <div className="grid-panel-container">
          <ConsoleInfoPanel
            modelId={modelId}
            modelName={modelName}
            serverPort={serverPort}
            contextLength={contextLength}
            startTime={serverStartTime ?? Math.floor(Date.now() / 1000)}
            onStopServer={onClose}
            activeTab={activeTab}
            onTabChange={setActiveTab}
          />
          <div className="resize-handle" onMouseDown={handleMouseDown} />
        </div>

        {/* Right Panel: Server Logs */}
        <div className="grid-panel-container">
          <ConsoleLogPanel serverPort={serverPort} />
        </div>
      </div>

      {/* New Conversation Modal */}
      {isNewConversationModalOpen && (
        <div
          className="chat-modal-overlay"
          onMouseDown={(e) => e.target === e.currentTarget && !creatingConversation && setIsNewConversationModalOpen(false)}
        >
          <div className="chat-modal">
            <h3 className="chat-modal-title">Start a new chat</h3>
            <label className="chat-modal-label">
              Title
              <Input
                className="chat-modal-input"
                value={newConversationTitle}
                onChange={(e) => setNewConversationTitle(e.target.value)}
                placeholder="New Chat"
              />
            </label>
            <label className="chat-modal-label">
              System Prompt
              <Textarea
                className="chat-modal-textarea"
                value={newConversationPrompt}
                onChange={(e) => setNewConversationPrompt(e.target.value)}
                placeholder={DEFAULT_SYSTEM_PROMPT}
                rows={4}
              />
            </label>
            <p className="chat-modal-hint">
              The system prompt steers the assistant's behavior for the entire conversation.
            </p>
            <div className="chat-modal-actions">
              <Button
                type="button"
                variant="secondary"
                onClick={() => setIsNewConversationModalOpen(false)}
                disabled={creatingConversation}
              >
                Cancel
              </Button>
              <Button
                type="button"
                variant="primary"
                onClick={handleCreateConversation}
                disabled={creatingConversation}
              >
                {creatingConversation ? 'Creating…' : 'Create chat'}
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
