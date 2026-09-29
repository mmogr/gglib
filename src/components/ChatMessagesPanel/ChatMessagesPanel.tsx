import React, { useMemo } from 'react';
import {
  ThreadPrimitive,
  useThreadRuntime,
  useThread,
} from '@assistant-ui/react';
import type { ToastType } from '../Toast';
import {
  ChatPanelHeader,
  SystemPromptSection,
  ChatStatusBanners,
  ComposerFooter,
  ConfirmDeleteModal,
  MessageActionsContext,
  AssistantMessageBubble,
  UserMessageBubble,
  SystemMessageBubble,
  EditComposer,
} from './components';
import type { MessageActionsContextValue } from './components';
import {
  useTitleGeneration,
  useMessageDeletion,
} from './hooks';
import { useSharedTicker } from './hooks/useSharedTicker';
import { ThinkingTimingProvider } from './context/ThinkingTimingContext';
import type { ReasoningTimingTracker } from '../../hooks/useGglibRuntime/reasoningTiming';
import type { ConversationSummary } from '../../services/transport';

interface ChatMessagesPanelProps {
  activeConversation: ConversationSummary | null;
  activeConversationId: number | null;
  isServerConnected: boolean;
  /** Absent for a chat with another machine: no local server to title with. */
  serverPort?: number;
  titleGenerationPrompt: string;
  onRenameConversation: (title: string) => Promise<void>;
  onClearConversation: () => Promise<void>;
  onExportConversation: () => void;
  onUpdateSystemPrompt: (prompt: string | null) => Promise<void>;
  onClose?: () => void;
  /** Whether the conversation's saved messages are still loading. */
  messageLoading: boolean;
  syncConversations: (options?: { preferredId?: number | null; silent?: boolean }) => Promise<void>;
  chatError: string | null;
  showToast: (message: string, type?: ToastType, duration?: number) => void;
  timingTracker: ReasoningTimingTracker | null;
  currentStreamingAssistantMessageId: string | null;
  /**
   * Whether the active model supports tool/function calling.
   * null = unknown (capability status not yet resolved).
   */
  supportsToolCalls?: boolean | null;
  /** Detected tool-calling format, e.g. "hermes" or "llama3". */
  toolFormat?: string | null;
  /** The model the next send goes to, as the page names it. */
  modelName: string;
  /** Its quantisation, from its catalogue entry, when known. */
  quantization?: string | null;
  /** The head's margin: the page's own controls. */
  headMargin?: React.ReactNode;
}

/** The notebook's column: the width its rows are laid out in. */
const NotebookColumn: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div className="@container mx-auto w-full max-w-[1000px] px-lg pt-xl">{children}</div>
);

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
  messageLoading,
  syncConversations,
  chatError,
  showToast,
  timingTracker,
  currentStreamingAssistantMessageId,
  supportsToolCalls,
  toolFormat,
  modelName,
  quantization,
  headMargin,
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
  // Delete flow — lives here because the cascade reload resets the thread
  // runtime this component owns
  // ─────────────────────────────────────────────────────────────────────────────
  const {
    initiateDelete,
    confirmDelete,
    cancelDelete,
    isDeleteModalOpen,
    isDeleting,
    pendingDeleteCount,
  } = useMessageDeletion({
    threadRuntime,
    activeConversationId,
    activeConversation,
    syncConversations,
    showToast,
  });

  // Context value for message actions
  const messageActionsValue = useMemo<MessageActionsContextValue>(
    () => ({ onDeleteMessage: initiateDelete }),
    [initiateDelete]
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

  const generateTitleBlockedReason = !activeConversationId
    ? 'No active conversation'
    : !serverPort
      ? 'Start a server to generate titles'
      : null;

  const head = (
    <>
      <ChatPanelHeader
        title={activeConversation?.title || 'New Chat'}
        isThreadRunning={isThreadRunning}
        generateTitleBlockedReason={generateTitleBlockedReason}
        isRenaming={isRenaming}
        titleDraft={titleDraft}
        isGeneratingTitle={isGeneratingTitle}
        onStartRename={startRenaming}
        onChangeTitleDraft={setTitleDraft}
        onCommitRename={commitRename}
        onCancelRename={cancelRenaming}
        onGenerateTitle={() => generateTitle()}
        onClearConversation={onClearConversation}
        onExportConversation={onExportConversation}
        margin={headMargin}
      >
        <SystemPromptSection conversation={activeConversation} onSave={onUpdateSystemPrompt} />
      </ChatPanelHeader>
      <ChatStatusBanners chatError={chatError} isServerConnected={isServerConnected} onClose={onClose} />
    </>
  );

  // ─────────────────────────────────────────────────────────────────────────────
  // Render
  // ─────────────────────────────────────────────────────────────────────────────
  return (
    <div className="flex flex-col overflow-hidden relative flex-1 bg-background md:h-full md:min-h-0">
      {messageLoading ? (
        <div className="flex-1 min-h-0 overflow-y-auto">
          <NotebookColumn>
            {head}
            <div role="status" className="py-lg text-text-muted">Loading messages…</div>
          </NotebookColumn>
        </div>
      ) : (
        <MessageActionsContext.Provider value={messageActionsValue}>
          <ThinkingTimingProvider value={{ timingTracker, currentStreamingAssistantMessageId, tick }}>
            <ThreadPrimitive.Root
              key={activeConversationId ?? 'thread-root'}
              className="flex flex-col flex-1 min-h-0"
            >
              <ThreadPrimitive.Viewport className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden flex flex-col scroll-smooth" autoScroll>
                <NotebookColumn>
                  {head}
                  <ThreadPrimitive.Messages components={messageComponents} />
                </NotebookColumn>
                <ThreadPrimitive.ScrollToBottom className="sticky bottom-sm self-center mt-auto py-xs px-md bg-primary text-text-inverse border-none rounded-full text-sm cursor-pointer opacity-0 transition-opacity duration-200 data-[visible=true]:opacity-100">
                  Jump to latest
                </ThreadPrimitive.ScrollToBottom>
              </ThreadPrimitive.Viewport>

              <ComposerFooter
                isServerConnected={isServerConnected}
                isThreadRunning={isThreadRunning}
                onStopGeneration={() => threadRuntime?.cancelRun()}
                modelName={modelName}
                quantization={quantization}
                supportsToolCalls={supportsToolCalls}
                toolFormat={toolFormat}
              />
            </ThreadPrimitive.Root>
          </ThinkingTimingProvider>
        </MessageActionsContext.Provider>
      )}

      {/* Delete confirmation modal */}
      <ConfirmDeleteModal
        isOpen={isDeleteModalOpen}
        messageCount={pendingDeleteCount}
        isDeleting={isDeleting}
        onConfirm={confirmDelete}
        onCancel={cancelDelete}
      />
    </div>
  );
};

export default ChatMessagesPanel;
