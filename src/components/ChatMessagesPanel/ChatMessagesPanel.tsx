import React, { useMemo, useState } from 'react';
import {
  ThreadPrimitive,
  useThreadRuntime,
  useThread,
} from '@assistant-ui/react';
import type { ToastType } from '../Toast';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import {
  ChatPanelHeader,
  SystemPromptLine,
  ChatStatusBanners,
  ComposerFooter,
  MessageActionsContext,
  AssistantMessageBubble,
  UserMessageBubble,
  SystemMessageBubble,
  EditComposer,
  ImageInputContext,
  Unanswered,
  BranchingContext,
  BranchEnd,
} from './components';
import type { MessageActionsContextValue, ModelChoice, ModelGone } from './components';
import {
  useTitleGeneration,
  useMessageDeletion,
  usePictureKeeping,
} from './hooks';
import { useSharedTicker } from './hooks/useSharedTicker';
import { ThinkingTimingProvider } from './context/ThinkingTimingContext';
import type { ReasoningTimingTracker } from '../../hooks/useGglibRuntime/reasoningTiming';
import type { ChatSource, ConversationSummary } from '../../services/transport';
import type { RunInfo } from '../../types/generated/RunInfo';
import type { ImageInput } from '../../hooks/useImageInput';
import type { ThinkingSwitch } from '../../hooks/useThinkingSwitch';
import type { UseGglibRuntimeReturn } from '../../hooks/useGglibRuntime';
import type { DrawSwitch } from '../../hooks/useDrawSwitch';
import type { ChatFolder } from '../../hooks/useChatFolder';
import { NO_PREVIEWS, type RunPreviews } from '../../hooks/useGglibRuntime/runPreviews';
import { RunPreviewsProvider } from './context/RunPreviewsContext';

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
  /** Delete the open conversation, from its title ▾; the place asks first. */
  onDeleteConversation: () => void;
  /** Whether the conversation's saved messages are still loading. */
  messageLoading: boolean;
  syncConversations: (options?: { preferredId?: number | null; silent?: boolean }) => Promise<void>;
  chatError: string | null;
  showToast: (message: string, type?: ToastType, duration?: number) => void;
  timingTracker: ReasoningTimingTracker | null;
  currentStreamingAssistantMessageId: string | null;
  /** The run the page last read to its end, as it ended: a reply that finishes in a chat not yet titled titles it. */
  endedRun: RunInfo | null;
  /**
   * Whether the active model supports tool/function calling.
   * null = unknown (capability status not yet resolved).
   */
  supportsToolCalls?: boolean | null;
  /** Detected tool-calling format, e.g. "hermes" or "llama3". */
  toolFormat?: string | null;
  /** The model the next send goes to, by its bare name; empty for a far chat, whose machine picks it. */
  modelName: string;
  /** Its registry id; absent for a chat with another machine. */
  modelId?: number;
  /** Move the chat to another model, from the composer's margin. */
  onPickModel?: (choice: ModelChoice) => Promise<void>;
  /** The model Load it again is starting, which the line says until it lands. */
  startingModel?: string | null;
  /** Load the model again, from the line that says it was unloaded. */
  onLoadModel?: () => Promise<void>;
  /** Unload the model, from beside the picker; the chat stays open. */
  onUnloadModel?: () => Promise<void>;
  /** Its quantisation, from its catalogue entry, when known. */
  quantization?: string | null;
  /**
   * Whose chat it is. A far chat is read and continued here, never changed:
   * no rename, restart, export or prompt edit, and no turn edited or deleted.
   */
  source?: ChatSource;
  /** Whether the model takes images, why not, and its context: for every composer in the thread. */
  imageInput: ImageInput;
  /** The chat's Thinking switch, for the composer's margin. */
  thinking?: ThinkingSwitch;
  /** The chat's folder, for the composer's margin. */
  folder?: ChatFolder;
  /** What the chat offers of its branches, and Retry; absent for a far chat. */
  branching?: UseGglibRuntimeReturn['branching'];
  /** The Draw button, for the composer's margin. */
  draw?: DrawSwitch;
  /** The frames the run's tools are making, by call, for the reply that shows them. */
  previews?: RunPreviews;
  /** The paired machine the chat's model is on (a paired or a far chat), for the composer's away line. */
  deskName?: string;
  /** This machine's model was unloaded or crashed: the composer is replaced by a line that says so. */
  modelGone?: ModelGone | null;
  /** Open the model's inspector (the picker's Show <model>). */
  onShowModel?: () => void;
  /** Open the model's log (a crash's Show log). */
  onShowLog?: () => void;
  /** Open a new chat on the paired machine's model, from an empty chat's picker. */
  onOpenFar?: (id: number) => void;
}

/**
 * The panel's column: the head and the turns, 680 px wide at most inside
 * its gutters, the composer's width (`ComposerFooter`), so the title, the
 * turns and the composer share one left edge.
 */
const NotebookColumn: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div className="@container mx-auto w-full max-w-[728px] px-lg pt-xl">{children}</div>
);

/** The turns' column: at most 680 px, the turns 32 px apart with no lines between them. */
const ReadingColumn: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div className="mx-auto w-full max-w-[680px] flex flex-col gap-xl pt-lg pb-xl">{children}</div>
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
  onDeleteConversation,
  messageLoading,
  syncConversations,
  chatError,
  showToast,
  timingTracker,
  currentStreamingAssistantMessageId,
  endedRun,
  supportsToolCalls,
  toolFormat,
  modelName,
  modelId,
  onPickModel,
  onLoadModel,
  startingModel,
  onUnloadModel,
  quantization,
  source = 'this',
  imageInput,
  thinking,
  folder,
  branching,
  draw,
  previews = NO_PREVIEWS,
  deskName,
  modelGone = null,
  onShowModel,
  onShowLog,
  onOpenFar,
}) => {
  const threadRuntime = useThreadRuntime({ optional: true });
  // The system prompt's editor, opened from its line or from the title ▾.
  const [editingPrompt, setEditingPrompt] = useState(false);
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
    endedRun,
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
  const { confirm } = useConfirmContext();
  const initiateDelete = useMessageDeletion({
    threadRuntime,
    activeConversationId,
    activeConversation,
    syncConversations,
    showToast,
    confirm,
    readOnly: source === 'far',
  });

  // Fixing and redoing a drawn picture: here for the reason deletion is.
  const pictures = usePictureKeeping({
    threadRuntime,
    activeConversationId,
    activeConversation,
    source,
    models: draw?.models,
    picked: draw?.picked,
  });

  // Context value for message actions
  const messageActionsValue = useMemo<MessageActionsContextValue>(
    () => ({ onDeleteMessage: (id: string) => void initiateDelete(id), source, ...(pictures && { pictures }) }),
    [initiateDelete, source, pictures]
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

  // The run read here failed: its line under the turns says why (`Unanswered`),
  // so the banner does not say the same failure again.
  const failure = endedRun?.status === 'failed' ? (endedRun.error?.message ?? '') : null;
  const failureShown = failure !== null && isServerConnected && !isThreadRunning;
  const bannerError =
    failureShown && chatError !== null && (failure ? chatError.includes(failure) : chatError === 'The reply failed.')
      ? null
      : chatError;

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
        onEditSystemPrompt={() => setEditingPrompt(true)}
        onClearConversation={onClearConversation}
        onExportConversation={onExportConversation}
        onDeleteConversation={onDeleteConversation}
        readOnly={source === 'far' || !activeConversation}
      >
        {source === 'this' && (
          <SystemPromptLine
            conversation={activeConversation}
            editing={editingPrompt && !!activeConversation}
            onEdit={() => setEditingPrompt(true)}
            onDone={() => setEditingPrompt(false)}
            onSave={onUpdateSystemPrompt}
          />
        )}
      </ChatPanelHeader>
      <ChatStatusBanners chatError={bannerError} />
    </>
  );

  // ─────────────────────────────────────────────────────────────────────────────
  // Render
  // ─────────────────────────────────────────────────────────────────────────────
  return (
    <div className="flex flex-col overflow-hidden relative min-w-0 bg-background flex-1 md:h-full md:min-h-0">
      <div className="flex flex-col flex-1 min-h-0">
        {messageLoading ? (
          <div className="flex-1 min-h-0 overflow-y-auto">
            <NotebookColumn>
              {head}
              <div role="status" className="py-lg text-text-muted">Loading messages…</div>
            </NotebookColumn>
          </div>
        ) : (
          <MessageActionsContext.Provider value={messageActionsValue}>
            <BranchingContext.Provider value={branching ?? null}>
              <ImageInputContext.Provider value={imageInput}>
                <ThinkingTimingProvider value={{ timingTracker, currentStreamingAssistantMessageId, tick }}>
                  <ThreadPrimitive.Root
                    key={activeConversationId ?? 'thread-root'}
                    className="flex flex-col flex-1 min-h-0"
                  >
                    <ThreadPrimitive.Viewport className="flex-1 min-h-0 overflow-y-auto overflow-x-hidden flex flex-col scroll-smooth" autoScroll>
                      <NotebookColumn>
                        {head}
                        <ReadingColumn>
                          <RunPreviewsProvider value={previews}>
                            <ThreadPrimitive.Messages components={messageComponents} />
                          </RunPreviewsProvider>
                          <BranchEnd />
                          {isServerConnected && <Unanswered failure={failure} />}
                        </ReadingColumn>
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
                      modelId={modelId}
                      onPickModel={onPickModel}
                      onLoadModel={onLoadModel}
                      startingModel={startingModel}
                      onUnloadModel={onUnloadModel}
                      quantization={quantization}
                      supportsToolCalls={supportsToolCalls}
                      toolFormat={toolFormat}
                      thinking={thinking}
                      folder={folder}
                      draw={draw}
                      far={source === 'far'}
                      saved={activeConversationId !== null}
                      deskName={deskName}
                      modelGone={modelGone}
                      onShowModel={onShowModel}
                      onShowLog={onShowLog}
                      onOpenFar={onOpenFar}
                    />
                  </ThreadPrimitive.Root>
                </ThinkingTimingProvider>
              </ImageInputContext.Provider>
            </BranchingContext.Provider>
          </MessageActionsContext.Provider>
        )}
      </div>
    </div>
  );
};

export default ChatMessagesPanel;
