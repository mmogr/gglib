import { useState, useEffect, useId, useRef } from 'react';
import { usePanelResize } from '../hooks/usePanelResize';
import { type ChatPageTabId, ChatPageControls } from './chatTabs';
import { AssistantRuntimeProvider } from '@assistant-ui/react';
import {
  ConversationListPanel,
  ConversationRail,
  useConversationActivity,
  useListFold,
} from '../components/ConversationListPanel';
import { ACTIVITY_POLL_MS } from '../components/ConversationListPanel/useConversationActivity';
import { useChatConversations } from './useChatConversations';
import { useConversationActions } from './useConversationActions';
import { ChatMessagesPanel, type ModelChoice } from '../components/ChatMessagesPanel';
import { ConsoleInfoPanel } from '../components/ConsoleInfoPanel';
import { ConsoleLogPanel } from '../components/ConsoleLogPanel';
import { GenericToolUI } from '../components/ToolUI';
import { NewConversationModal } from '../components/NewConversationModal';
import TwoPanelLayout from '../components/TwoPanelLayout';
import { useGglibRuntime, DEFAULT_SYSTEM_PROMPT } from '../hooks/useGglibRuntime';
import { useSettings } from '../hooks/useSettings';
import { useChatModelFacts } from '../hooks/useChatModelFacts';
import { useImageInput } from '../hooks/useImageInput';
import { useThinkingSwitch } from '../hooks/useThinkingSwitch';
import { useToastContext } from '../contexts/ToastContext';
import { useConfirmContext } from '../contexts/ConfirmContext';
import { cn } from '../utils/cn';

import { useServerState } from '../services/serverEvents';
import { getTransport, DEFAULT_TITLE_GENERATION_PROMPT } from '../services/transport';
import type { ConversationSummary } from '../services/transport';
import type { HubChatOpen } from '../types/generated/HubChatOpen';
import type { ModelRef } from '../types/generated/ModelRef';
import type { ChatDraft } from '../types/messages';
import { formatError } from '../utils/errors';

const DEFAULT_CONVERSATION_TITLE = 'New Chat';

/**
 * Whose model is answering.
 *
 * The local arm is a server started here: a port to talk to, a model id the
 * registry knows, a console to read. The paired arm is a model of the
 * machine on the other end of the tunnel, by that machine and its id there —
 * the daemon supplies its port and key per turn, so neither exists on this
 * side, and a union rather than optional numbers is what stops the console
 * and the health subscription being handed placeholders they would report
 * as a dead server. A session's machine is fixed: the picker, which moves a
 * chat between this machine's models, is not offered on the paired arm.
 */
type ChatPageProps = {
  modelName: string;
  contextLength?: number;
  serverStartTime?: number; // Unix timestamp in seconds
  initialView?: 'chat' | 'console'; // Which view to show initially
  conversationId?: number | null; // The conversation to open with, e.g. after a model switch
  draft?: ChatDraft; // Unsent text and images to put back in the composer, e.g. after a model switch
  startingModel?: string | null; // The model a switch is starting, which locks the picker
  // Move the chat to another model, keeping open the conversation (and the
  // draft) that `context` reads when the switch lands; local only.
  onSwitchModel?: (choice: ModelChoice, context: () => { conversationId: number | null; draft: ChatDraft }) => Promise<void>;
  // Unload the model, which every client of the proxy loses with it; the
  // chat stays open, read-only. Local only.
  onUnloadModel?: () => Promise<void>;
  onClose: () => void; // Leaves the chat; the model stays loaded
} & (
  | { paired?: undefined; serverPort: number; modelId: number }
  | {
      /** The paired machine's model, and the name that machine is shown by. */
      paired: { far: ModelRef; machineName: string };
      serverPort?: undefined;
      modelId?: undefined;
    }
);

export default function ChatPage(props: ChatPageProps) {
  // Destructured for the body, but `props` is kept: the checker can narrow
  // `props.paired` and correlate the port and id with it, and cannot do that
  // for locals it has already separated.
  const {
    serverPort,
    modelId,
    modelName,
    contextLength,
    serverStartTime,
    initialView = 'chat',
    conversationId = null,
    draft,
    startingModel = null,
    onSwitchModel,
    onUnloadModel,
    paired,
    onClose,
  } = props;
  const pairedChat = paired !== undefined;
  // Tab state
  const [activeTab, setActiveTab] = useState<ChatPageTabId>(initialView);
  
  const [conversationSearch, setConversationSearch] = useState('');
  const [chatError, setChatError] = useState<string | null>(null);
  // The list, from this machine or the one it is joined to; a far chat is
  // read and carried on here, and changed only there.
  const {
    source, switchSource, conversations, setConversations, conversationLoading, activeConversationId,
    setActiveConversationId, landingConversationId, fetched, syncConversations,
  } = useChatConversations(conversationId, setChatError, paired?.far.machine);
  const far = source === 'far';
  
  // New conversation modal state
  const [isNewConversationModalOpen, setIsNewConversationModalOpen] = useState(false);
  const [newConversationTitle, setNewConversationTitle] = useState(DEFAULT_CONVERSATION_TITLE);
  const [newConversationPrompt, setNewConversationPrompt] = useState(DEFAULT_SYSTEM_PROMPT);
  const [creatingConversation, setCreatingConversation] = useState(false);
  
  // The conversation list beside the notebook, and the rail that folds it
  const listFold = useListFold();
  // Running and New, for the list's rows and the rail's list button
  const activity = useConversationActivity(activeConversationId, fetched, ACTIVITY_POLL_MS, source);
  const countOf = (ids: ReadonlySet<number>) => conversations.filter((c) => ids.has(c.id)).length;
  const listId = useId();
  const searchInputRef = useRef<HTMLInputElement>(null);
  const handleSearch = () => {
    listFold.unfold();
    setTimeout(() => searchInputRef.current?.focus(), 0);
  };

  // Panel width for resize (the console view)
  const { leftPanelWidth, layoutRef, handlePointerDown, handleKeyboardResize } = usePanelResize({ initial: 35, min: 20, max: 50, storageKey: 'gglib.chat.split' });

  // Toast notifications
  const { showToast } = useToastContext();
  const { confirm } = useConfirmContext();

  // Settings for title generation prompt
  const { settings } = useSettings();
  const titleGenerationPrompt = settings?.titleGenerationPrompt || DEFAULT_TITLE_GENERATION_PROMPT;

  // Tool support and quantisation for the active model. The model is fixed
  // for the lifetime of ChatPage: a switch from the composer's picker
  // remounts the page on the new session, with this conversation still open.
  const { supportsToolCalls, toolFormat, quantization, sees, contextLength: servedContext, thinks } = useChatModelFacts(modelId);
  const imageInput = useImageInput({ far, paired: paired?.far, sees, contextLength: servedContext });

  // Get active conversation
  const activeConversation = conversations.find((c) => c.id === activeConversationId) ?? null;

  // The Thinking switch: shown where the chat's model thinks, saying what the
  // chat remembers. The far list tells none of a chat's settings, so a far
  // chat is kept as its machine last answered it, for this alone.
  const [farOpen, setFarOpen] = useState<HubChatOpen | null>(null);
  const thinking = useThinkingSwitch({ conversationId: activeConversationId, conversations, far, farOpen, paired: paired?.far, thinks });

  // Runtime: sends start runs the daemon owns and saves; opening a
  // conversation shows what is saved, then the run still going in it.
  const { runtime, isLoading: messageLoading, endedRun, timingTracker, currentStreamingAssistantMessageId } = useGglibRuntime({
    conversationId: activeConversationId ?? undefined,
    conversation: activeConversation,
    source,
    onConversationChanged: (id) => void syncConversations({ preferredId: id, silent: true }),
    selectedServerPort: serverPort,
    pairedModel: paired?.far,
    onError: (error) => setChatError(error.message),
    // Non-fatal: the turn is still running, so this is a transient notice
    // rather than `chatError`, which renders as a failed turn.
    onSystemWarning: (message, suggestedAction) =>
      showToast(suggestedAction ? `${message} — ${suggestedAction}` : message, 'warning'),
    supportsToolCalls,
    // assistant-ui only logs a paste or a drop that fails; this is the person told.
    onImageRefused: (sentence) => showToast(sentence, 'error'),
    thinking: thinking.forSend,
    onFarOpened: setFarOpen,
  });

  // A draft carried over a model switch goes back in the composer, once:
  // its images are the files already uploaded, so none is sent again.
  const landedDraft = useRef<ChatDraft | null>(null);
  useEffect(() => {
    if (!draft || landedDraft.current === draft) return;
    landedDraft.current = draft;
    if (draft.text) runtime.thread.composer.setText(draft.text);
    for (const image of draft.images) runtime.thread.composer.addAttachment(image).catch(() => {});
  }, [draft, runtime]);

  // Server state from registry - derives isServerRunning reactively
  // Note: If serverState is null (no event received yet), we assume running
  // because ChatPage is only opened when a server is already running.
  // A paired session subscribes to nothing — this registry only knows servers
  // started here, so its silence about the far machine must not be read as
  // that machine being down and the composer locked.
  const serverState = useServerState(modelId ?? -1);
  const isServerRunning =
    pairedChat || far || (serverState?.status !== 'stopped' && serverState?.status !== 'crashed');

  // Track previous status for transition-only toast
  const prevStatusRef = useRef(serverState?.status);

  // Show toast only on status transition to stopped/crashed (not on remount).
  // Never for a paired chat: whatever this machine's registry is reporting,
  // it is not the model answering, and saying the chat is read-only when it
  // is not is worse than saying nothing.
  useEffect(() => {
    const prev = prevStatusRef.current;
    const next = serverState?.status;

    if (!pairedChat && prev !== next && (next === 'stopped' || next === 'crashed')) {
      showToast(
        next === 'crashed'
          ? 'Server crashed. Chat is now read-only.'
          : 'Server stopped. Chat is now read-only.',
        'warning'
      );
    }

    prevStatusRef.current = next;
  }, [pairedChat, serverState?.status, showToast]);

  // An error belongs to the conversation it happened in.
  useEffect(() => setChatError(null), [activeConversationId]);

  // What changes a conversation: this machine's only, never a far chat.
  const { handleDeleteConversation, handleRenameConversation, handleClearConversation, handleExportConversation, handleUpdateSystemPrompt } =
    useConversationActions({ far, activeConversation, confirm, syncConversations, onError: setChatError });

  const handleNewConversation = () => {
    setNewConversationTitle(DEFAULT_CONVERSATION_TITLE);
    setNewConversationPrompt(activeConversation?.system_prompt ?? DEFAULT_SYSTEM_PROMPT);
    setIsNewConversationModalOpen(true);
  };

  const handleCreateConversation = async () => {
    if (far) return; // A chat is made on the machine that holds it.
    setCreatingConversation(true);
    try {
      const title = newConversationTitle.trim() || DEFAULT_CONVERSATION_TITLE;
      const systemPrompt = newConversationPrompt.trim() || DEFAULT_SYSTEM_PROMPT;
      // A chat with a far model is made for it, so it stays that machine's.
      const newId = await getTransport().createConversation({ title, modelId: null, systemPrompt, model: paired?.far ?? null });

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
      setChatError(formatError(error));
    } finally {
      setCreatingConversation(false);
    }
  };

  return (
    <div className="flex-1 flex flex-col min-h-0 bg-background">
      <AssistantRuntimeProvider runtime={runtime}>
        {/* Tool UI Components - render tool calls in chat messages */}
        <GenericToolUI />
        
        {/* The rail, the list and the notebook's head stay in both views; the
            head's margin switches the body between the thread and the console. */}
        <div className="flex flex-1 min-h-0">
          <ConversationRail
            onNewConversation={handleNewConversation}
            onSearch={handleSearch}
            listOpen={listFold.open}
            canFold={listFold.canFold}
            onToggleList={listFold.toggle}
            listId={listId}
            remote={pairedChat}
            running={countOf(activity.running)}
            unread={countOf(activity.unread)}
            source={source}
            onSource={(next) => { setActiveTab('chat'); switchSource(next); }}
          />
          <div id={listId} hidden={!listFold.open} className={cn('w-[280px] shrink-0 flex flex-col min-h-0 border-r border-border-light', !listFold.open && 'hidden')}>
            <ConversationListPanel
              conversations={conversations}
              activeConversationId={activeConversationId}
              onSelectConversation={(id) => {
                setActiveConversationId(id);
                // What a chat remembers can change on another device, and only
                // this machine's list says it: read it again as one is opened.
                if (!far) void syncConversations({ silent: true });
              }}
              onDeleteConversation={far ? undefined : handleDeleteConversation}
              searchQuery={conversationSearch}
              onSearchChange={setConversationSearch}
              loading={conversationLoading}
              searchInputRef={searchInputRef}
              running={activity.running}
              unread={activity.unread}
            />
          </div>
          <div className="flex flex-col flex-1 min-w-0 min-h-0">
            <ChatMessagesPanel
              key={activeConversationId ?? "none"}
              activeConversation={activeConversation}
              activeConversationId={activeConversationId}
              isServerConnected={isServerRunning}
              serverPort={far ? undefined : serverPort}
              titleGenerationPrompt={titleGenerationPrompt}
              onRenameConversation={handleRenameConversation}
              onClearConversation={handleClearConversation}
              onExportConversation={handleExportConversation}
              onUpdateSystemPrompt={handleUpdateSystemPrompt}
              onClose={onClose}
              messageLoading={messageLoading}
              syncConversations={syncConversations}
              chatError={chatError}
              showToast={showToast}
              timingTracker={timingTracker}
              currentStreamingAssistantMessageId={currentStreamingAssistantMessageId}
              endedRun={endedRun}
              supportsToolCalls={far ? null : supportsToolCalls}
              toolFormat={far ? null : toolFormat}
              modelName={far ? 'The other machine picks the model' : paired ? `${modelName} on ${paired.machineName}` : modelName}
              modelId={far ? undefined : modelId}
              source={source}
              onPickModel={onSwitchModel && ((choice) => onSwitchModel(choice, () => ({
                conversationId: landingConversationId(),
                draft: {
                  text: runtime.thread.composer.getState().text,
                  images: runtime.thread.composer.getState().attachments.flatMap((a) => a.file ?? []),
                },
              })))}
              startingModel={startingModel}
              onUnloadModel={onUnloadModel}
              quantization={far ? null : quantization}
              imageInput={imageInput}
              thinking={thinking}
              headMargin={
                <ChatPageControls activeTab={activeTab} onTabChange={setActiveTab} remote={pairedChat || far} onClose={onClose} />
              }
              headOnly={activeTab === 'console'}
            />
            {/* Console - always mounted, hidden when not active. Absent
                entirely for a paired chat: the process it reports on is on the
                other machine, so there is no id, port or log to hand it. */}
            {!props.paired && !far && (
              <TwoPanelLayout
                ref={activeTab === 'console' ? layoutRef : undefined}
                isHidden={activeTab !== 'console'}
                className="flex-1 min-h-0 border-t border-border-light"
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
                    onUnload={onUnloadModel}
                  />
                }
                right={<ConsoleLogPanel serverPort={props.serverPort} />}
              />
            )}
          </div>
        </div>
      </AssistantRuntimeProvider>

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
