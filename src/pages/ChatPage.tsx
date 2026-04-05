import { useState, useEffect, useCallback, useRef } from 'react';
import { usePanelResize } from '../hooks/usePanelResize';
import { type ChatPageTabId } from './chatTabs';
import { appLogger } from '../services/platform';
import { AssistantRuntimeProvider } from '@assistant-ui/react';
import { ConversationListPanel } from '../components/ConversationListPanel';
import { ChatMessagesPanel } from '../components/ChatMessagesPanel';
import { ConsoleInfoPanel } from '../components/ConsoleInfoPanel';
import { ConsoleLogPanel } from '../components/ConsoleLogPanel';
import { GenericToolUI } from '../components/ToolUI';
import { VoiceOverlay } from '../components/VoiceOverlay';
import TwoPanelLayout from '../components/TwoPanelLayout';
import { Button } from '../components/ui/Button';
import { Input } from '../components/ui/Input';
import { Textarea } from '../components/ui/Textarea';
import { useGglibRuntime, DEFAULT_SYSTEM_PROMPT } from '../hooks/useGglibRuntime';
import { useChatPersistence } from '../hooks/useChatPersistence';
import { useSettings } from '../hooks/useSettings';
import { useVoiceModeContext } from '../contexts/VoiceModeContext';
import { useToastContext } from '../contexts/ToastContext';
import { useConfirmContext } from '../contexts/ConfirmContext';
import { useServerState } from '../services/serverEvents';
import { getServerToolSupport } from '../services/clients/servers';
import {
  listConversations,
  createConversation,
  deleteConversation,
  updateConversationTitle,
  updateConversationSystemPrompt,
  DEFAULT_TITLE_GENERATION_PROMPT,
} from '../services/clients/chat';
import type { ConversationSummary } from '../services/clients/chat';

const DEFAULT_CONVERSATION_TITLE = 'New Chat';

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
  const { leftPanelWidth, layoutRef, handlePointerDown, handleKeyboardResize } = usePanelResize({ initial: 35, min: 20, max: 50 });

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
    let cancelled = false;
    getServerToolSupport(modelId)
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
  const { runtime, messages, setMessages, isRunning, timingTracker, currentStreamingAssistantMessageId, setNextMessageMeta } = useGglibRuntime({
    conversationId: activeConversationId ?? undefined,
    selectedServerPort: serverPort,
    onError: (error) => setChatError(error.message),
    maxToolIterations,
    supportsToolCalls,
  });

  // Server state from registry - derives isServerRunning reactively
  // Note: If serverState is null (no event received yet), we assume running
  // because ChatPage is only opened when a server is already running
  const serverState = useServerState(modelId);
  const isServerRunning = serverState?.status !== 'stopped' && serverState?.status !== 'crashed';

  // Voice mode — shared singleton from App-level VoiceModeProvider.
  // The provider reads the same settings defaults so behaviour is identical
  // to the previous per-component useVoiceMode() call.
  const voice = useVoiceModeContext();

  // Send voice transcript as a chat message
  const handleVoiceTranscript = useCallback((text: string) => {
    if (!text.trim()) return;
    setNextMessageMeta({ isVoice: true });
    runtime.thread.append({
      role: 'user',
      content: [{ type: 'text', text }],
    });
  }, [runtime, setNextMessageMeta]);

  // Auto-speak: when the LLM finishes responding, speak the last assistant message
  const wasRunningRef = useRef(false);
  useEffect(() => {
    if (wasRunningRef.current && !isRunning && voice?.isActive && voice?.autoSpeak && voice?.ttsLoaded) {
      // Find the last assistant message and extract only visible text.
      // Thinking blocks are stripped by the Rust pipeline's strip_markdown();
      // no need to strip them here.
      const lastAssistant = [...messages].reverse().find(m => m.role === 'assistant');
      if (lastAssistant) {
        const content = lastAssistant.content;
        let text = '';
        if (typeof content === 'string') {
          text = content;
        } else if (Array.isArray(content)) {
          text = content
            .filter((p): p is { type: 'text'; text: string } =>
              (p as { type: string }).type === 'text'
            )
            .map(p => p.text)
            .join(' ');
        }
        // The Rust voice pipeline strips thinking blocks in strip_markdown();
        // calling stripThinkingBlocks() here is redundant and has been removed.
        if (text) {
          voice?.speak(text).catch(err => {
            appLogger.error('hook.runtime', 'Auto-speak failed', { error: String(err) });
          });
        }
      }
    }
    wasRunningRef.current = isRunning;
  }, [isRunning, voice, messages]);

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
      await updateConversationTitle(activeConversation.id, title);
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
              voice={voice ?? undefined}
              supportsToolCalls={supportsToolCalls}
              toolFormat={toolFormat}
            />
          }
        />

        {/* Voice overlay (floating controls when voice mode is active) */}
        <VoiceOverlay voice={voice} onTranscript={handleVoiceTranscript} />
      </AssistantRuntimeProvider>

      {/* Console Tab Content - always mounted, hidden when not active */}
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
            modelId={modelId}
            modelName={modelName}
            serverPort={serverPort}
            contextLength={contextLength}
            startTime={serverStartTime ?? Math.floor(Date.now() / 1000)}
            onStopServer={onClose}
            activeTab={activeTab}
            onTabChange={setActiveTab}
          />
        }
        right={<ConsoleLogPanel serverPort={serverPort} />}
      />

      {/* New Conversation Modal */}
      {isNewConversationModalOpen && (
        <div
          className="fixed inset-0 bg-black/60 backdrop-blur-[4px] flex items-center justify-center z-[1000]"
          onMouseDown={(e) => e.target === e.currentTarget && !creatingConversation && setIsNewConversationModalOpen(false)}
        >
          <div className="bg-surface border border-border rounded-lg p-xl w-[min(450px,90vw)] max-h-[90vh] overflow-y-auto flex flex-col gap-md">
            <h3 className="text-lg font-semibold m-0">Start a new chat</h3>
            <label className="flex flex-col gap-xs text-sm text-text-muted">
              Title
              <Input
                className="py-sm px-md border border-border rounded-sm bg-background text-text text-sm focus:outline-none focus:border-primary"
                value={newConversationTitle}
                onChange={(e) => setNewConversationTitle(e.target.value)}
                placeholder="New Chat"
              />
            </label>
            <label className="flex flex-col gap-xs text-sm text-text-muted">
              System Prompt
              <Textarea
                className="py-sm px-md border border-border rounded-sm bg-background text-text text-sm font-[inherit] resize-y min-h-[100px] focus:outline-none focus:border-primary"
                value={newConversationPrompt}
                onChange={(e) => setNewConversationPrompt(e.target.value)}
                placeholder={DEFAULT_SYSTEM_PROMPT}
                rows={4}
              />
            </label>
            <p className="text-xs text-text-muted m-0">
              The system prompt steers the assistant's behavior for the entire conversation.
            </p>
            <div className="flex justify-end gap-sm mt-sm">
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
