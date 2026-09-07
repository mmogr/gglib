import { useState, useEffect, useCallback, useRef } from 'react';
import { usePanelResize } from '../hooks/usePanelResize';
import { type ChatPageTabId, CHAT_PAGE_TABS, REMOTE_CHAT_PAGE_TABS } from './chatTabs';
import { appLogger } from '../services/platform';
import { AssistantRuntimeProvider } from '@assistant-ui/react';
import { ConversationListPanel } from '../components/ConversationListPanel';
import { ChatMessagesPanel } from '../components/ChatMessagesPanel';
import { ConsoleInfoPanel } from '../components/ConsoleInfoPanel';
import { ConsoleLogPanel } from '../components/ConsoleLogPanel';
import { GenericToolUI } from '../components/ToolUI';
import { NewConversationModal } from '../components/NewConversationModal';
import TwoPanelLayout from '../components/TwoPanelLayout';
import { useGglibRuntime, DEFAULT_SYSTEM_PROMPT } from '../hooks/useGglibRuntime';
import { useChatPersistence } from '../hooks/useChatPersistence';
import { useSettings } from '../hooks/useSettings';
import { useToastContext } from '../contexts/ToastContext';
import { useConfirmContext } from '../contexts/ConfirmContext';

import { useServerState } from '../services/serverEvents';
import { getTransport, DEFAULT_TITLE_GENERATION_PROMPT } from '../services/transport';
import type { ConversationSummary } from '../services/transport';

const DEFAULT_CONVERSATION_TITLE = 'New Chat';

/**
 * Whose model is answering.
 *
 * The local arm is a server started here: a port to talk to, a model id the
 * registry knows, a console to read. The remote arm is the machine on the
 * other end of the tunnel — the daemon supplies its port and key per turn,
 * so neither exists on this side, and a union rather than optional numbers
 * is what stops the console and the health subscription being handed
 * placeholders they would report as a dead server.
 */
type ChatPageProps = {
  modelName: string;
  contextLength?: number;
  serverStartTime?: number; // Unix timestamp in seconds
  initialView?: 'chat' | 'console'; // Which view to show initially
  onClose: () => Promise<void>; // Stops server and exits
} & (
  | { remote?: false; serverPort: number; modelId: number }
  | { remote: true; serverPort?: undefined; modelId?: undefined }
);

export default function ChatPage(props: ChatPageProps) {
  // Destructured for the body, but `props` is kept: the checker can narrow
  // `props.remote` and correlate the port and id with it, and cannot do that
  // for locals it has already separated.
  const {
    serverPort,
    modelId,
    modelName,
    contextLength,
    serverStartTime,
    initialView = 'chat',
    remote = false,
    onClose,
  } = props;
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
  const { leftPanelWidth, layoutRef, handlePointerDown, handleKeyboardResize } = usePanelResize({ initial: 35, min: 20, max: 50, storageKey: 'gglib.chat.split' });

  // Toast notifications
  const { showToast } = useToastContext();
  const { confirm } = useConfirmContext();

  // Settings for title generation prompt and agent loop
  const { settings } = useSettings();
  const titleGenerationPrompt = settings?.titleGenerationPrompt || DEFAULT_TITLE_GENERATION_PROMPT;
  const maxToolIterations = settings?.maxToolIterations ?? undefined;

  // Tool support capability for the active model.
  // Fetched once on mount (model identity is fixed for the lifetime of ChatPage).
  // null = unknown (permissive fallback - never gates tools when status is uncertain).
  const [supportsToolCalls, setSupportsToolCalls] = useState<boolean | null>(null);
  const [toolFormat, setToolFormat] = useState<string | null>(null);
  useEffect(() => {
    // Nothing to ask about remotely: the capability is read from this
    // machine's server registry and the model is on the other machine.
    // `null` is already the permissive answer, which is the right one here.
    if (modelId === undefined) return;
    let cancelled = false;
    getTransport().getServerToolSupport(modelId)
      .then((data) => {
        if (!cancelled) {
          setSupportsToolCalls(data.supports_tool_calls);
          setToolFormat(data.detected_format ?? null);
        }
      })
      .catch(() => {
        // Permissive fallback: leave supportsToolCalls as null (unknown)
      });
    return () => { cancelled = true; };
  }, [modelId]);

  // Runtime - now with external message state
  const { runtime, messages, setMessages, timingTracker, currentStreamingAssistantMessageId } = useGglibRuntime({
    conversationId: activeConversationId ?? undefined,
    selectedServerPort: serverPort,
    onError: (error) => setChatError(error.message),
    // Non-fatal: the turn is still running, so this is a transient notice
    // rather than `chatError`, which renders as a failed turn.
    onSystemWarning: (message, suggestedAction) =>
      showToast(suggestedAction ? `${message} — ${suggestedAction}` : message, 'warning'),
    maxToolIterations,
    supportsToolCalls,
  });

  // Server state from registry - derives isServerRunning reactively
  // Note: If serverState is null (no event received yet), we assume running
  // because ChatPage is only opened when a server is already running.
  // A remote session subscribes to nothing — this registry only knows servers
  // started here, so its silence about the far machine must not be read as
  // that machine being down and the composer locked.
  const serverState = useServerState(modelId ?? -1);
  const isServerRunning =
    remote || (serverState?.status !== 'stopped' && serverState?.status !== 'crashed');

  // Track previous status for transition-only toast
  const prevStatusRef = useRef(serverState?.status);

  // Show toast only on status transition to stopped/crashed (not on remount).
  // Never for a remote chat: whatever this machine's registry is reporting,
  // it is not the model answering, and saying the chat is read-only when it
  // is not is worse than saying nothing.
  useEffect(() => {
    const prev = prevStatusRef.current;
    const next = serverState?.status;

    if (!remote && prev !== next && (next === 'stopped' || next === 'crashed')) {
      showToast(
        next === 'crashed'
          ? 'Server crashed. Chat is now read-only.'
          : 'Server stopped. Chat is now read-only.',
        'warning'
      );
    }

    prevStatusRef.current = next;
  }, [remote, serverState?.status, showToast]);

  // Sync conversations
  const syncConversations = useCallback(
    async (options: { preferredId?: number | null; silent?: boolean } = {}) => {
      if (!options.silent) {
        setConversationLoading(true);
      }
      try {
        let list = await getTransport().listConversations();
        let preferredId = options.preferredId ?? null;

        // Create default conversation if none exist
        if (!list.length) {
          preferredId = await getTransport().createConversation({
            title: DEFAULT_CONVERSATION_TITLE,
            modelId: null,
            systemPrompt: DEFAULT_SYSTEM_PROMPT,
          });
          list = await getTransport().listConversations();
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

  // Conversation handlers
  const handleDeleteConversation = async (conversationId: number) => {
    const shouldDelete = await confirm({
      title: 'Delete this conversation?',
      description: 'This cannot be undone.',
      confirmLabel: 'Delete',
      variant: 'danger',
    });
    if (!shouldDelete) return;

    try {
      await getTransport().deleteConversation(conversationId);
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
      const newId = await getTransport().createConversation({ title, modelId: null, systemPrompt });
      persistedMessageIds.current = new Set();
      
      // Insert new conversation locally before selecting it
      const newConversation: ConversationSummary = {
        id: newId,
        title,
        model_id: null,
        system_prompt: systemPrompt,
        settings: null,
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
      appLogger.debug('component.chat', 'Rename conversation called', {
        conversationId: activeConversation.id,
        title,
        titleLength: title.length,
      });
      await getTransport().updateConversationTitle(activeConversation.id, title);
      appLogger.debug('component.chat', 'Title update succeeded, syncing');
      await syncConversations({ preferredId: activeConversation.id, silent: true });
      appLogger.debug('component.chat', 'Rename conversation completed successfully');
    } catch (error: any) {
      appLogger.error('component.chat', 'Rename conversation failed', {
        error,
        conversationId: activeConversation.id,
        title
      });
      setChatError(error instanceof Error ? error.message : String(error));
    }
  };

  const handleClearConversation = async () => {
    if (!activeConversation) return;
    const confirmed = await confirm({
      title: 'Start a fresh copy?',
      description: 'The current conversation will be deleted and replaced with a new copy.',
      confirmLabel: 'Start fresh',
    });
    if (!confirmed) return;

    try {
      await getTransport().deleteConversation(activeConversation.id);
      const newId = await getTransport().createConversation({
        title: activeConversation.title,
        modelId: null,
        systemPrompt: activeConversation.system_prompt ?? DEFAULT_SYSTEM_PROMPT,
      });
      persistedMessageIds.current = new Set();
      await syncConversations({ preferredId: newId });
    } catch (error) {
      setChatError(error instanceof Error ? error.message : String(error));
    }
  };

  const handleExportConversation = async () => {
    if (!activeConversation) return;
    try {
      const messages = await getTransport().getMessages(activeConversation.id);
      const data = { conversation: activeConversation, messages };
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = `conversation-${activeConversation.id}.json`;
      anchor.click();
      URL.revokeObjectURL(url);
    } catch (error) {
      setChatError(error instanceof Error ? error.message : String(error));
    }
  };

  const handleUpdateSystemPrompt = async (prompt: string | null) => {
    if (!activeConversation) return;
    try {
      await getTransport().updateConversationSystemPrompt(activeConversation.id, prompt);
      await syncConversations({ preferredId: activeConversation.id, silent: true });
    } catch (error) {
      setChatError(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <div className="flex-1 flex flex-col min-h-0 bg-background">
      {/* Chat Tab Content - always mounted, hidden when not active */}
      <AssistantRuntimeProvider runtime={runtime}>
        {/* Tool UI Components - render tool calls in chat messages */}
        <GenericToolUI />
        
        <TwoPanelLayout
          ref={activeTab === 'chat' ? layoutRef : undefined}
          isHidden={activeTab !== 'chat'}
          className="flex-1 min-h-0"
          leftWidth={leftPanelWidth}
          onResizeStart={handlePointerDown}
          onKeyboardResize={handleKeyboardResize}
          leftClassName="max-h-[40vh] border-b border-border md:max-h-none md:border-b-0"
          left={
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
              tabs={remote ? REMOTE_CHAT_PAGE_TABS : CHAT_PAGE_TABS}
            />
          }
          right={
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
              supportsToolCalls={supportsToolCalls}
              toolFormat={toolFormat}
            />
          }
        />

      </AssistantRuntimeProvider>

      {/* Console Tab Content - always mounted, hidden when not active.
          Absent entirely for a remote chat: the process it reports on is on
          the other machine, so there is no id, port or log to hand it. */}
      {!props.remote && (
        <TwoPanelLayout
          ref={activeTab === 'console' ? layoutRef : undefined}
          isHidden={activeTab !== 'console'}
          className="flex-1 min-h-0"
          leftWidth={leftPanelWidth}
          onResizeStart={handlePointerDown}
          onKeyboardResize={handleKeyboardResize}
          leftClassName="max-h-[40vh] border-b border-border md:max-h-none md:border-b-0"
          left={
            <ConsoleInfoPanel
              modelId={props.modelId}
              modelName={modelName}
              serverPort={props.serverPort}
              contextLength={contextLength}
              startTime={serverStartTime ?? Math.floor(Date.now() / 1000)}
              onStopServer={onClose}
              activeTab={activeTab}
              onTabChange={setActiveTab}
            />
          }
          right={<ConsoleLogPanel serverPort={props.serverPort} />}
        />
      )}

      {isNewConversationModalOpen && (
        <NewConversationModal
          title={newConversationTitle}
          onTitleChange={setNewConversationTitle}
          systemPrompt={newConversationPrompt}
          onSystemPromptChange={setNewConversationPrompt}
          creating={creatingConversation}
          onCancel={() => setIsNewConversationModalOpen(false)}
          onCreate={handleCreateConversation}
        />
      )}
    </div>
  );
}
