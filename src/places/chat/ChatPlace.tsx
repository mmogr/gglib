/**
 * A chat: one conversation, with no list beside it and no Close. The
 * sidebar lists the chats and navigates; this place shows the one its
 * address names (`app/ChatRoute`, `app/FarChatRoute`), or an empty thread
 * whose first send makes the conversation.
 *
 * Leaving it keeps the model loaded (the endpoint serves it to every
 * client); Unload <model> is in the composer's model picker ▾. A model
 * unloaded or crashed while the chat is open replaces the composer with a
 * line that says so, with no banner and no toast.
 *
 * @module places/chat/ChatPlace
 */

import { useState, useEffect, useRef } from 'react';
import { AssistantRuntimeProvider } from '@assistant-ui/react';
import { useConversationActions } from './useConversationActions';
import { ChatMessagesPanel, type ModelChoice } from '../../components/ChatMessagesPanel';
import { useGglibRuntime } from '../../hooks/useGglibRuntime';
import { useSettings } from '../../hooks/useSettings';
import { useChatModelFacts } from '../../hooks/useChatModelFacts';
import { useImageInput } from '../../hooks/useImageInput';
import { useThinkingSwitch } from '../../hooks/useThinkingSwitch';
import { useDrawSwitch } from '../../hooks/useDrawSwitch';
import { useChatFolder } from '../../hooks/useChatFolder';
import { useChatTools } from '../../hooks/useChatTools';
import { useToastContext } from '../../contexts/ToastContext';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { refreshChats, useChats } from '../../services/chatRegistry';
import { useServerState } from '../../services/serverEvents';
import { DEFAULT_TITLE_GENERATION_PROMPT } from '../../services/transport';
import type { ConversationSummary } from '../../services/transport';
import type { HubChatOpen } from '../../types/generated/HubChatOpen';
import type { ModelRef } from '../../types/generated/ModelRef';
import type { ChatDraft } from '../../types/messages';

const NO_CHATS: ConversationSummary[] = [];

/**
 * Whose model is answering.
 *
 * The local arm is one of this machine's models, by the id the registry
 * knows; the port it is served on, when it is, names it for a title. A turn
 * needs none: the daemon loads the model the chat saved when it is not
 * running. The paired arm is a model of the machine on the other
 * end of the tunnel, by that machine and its id there — the daemon supplies
 * its port and key per turn, so neither exists on this side, and a union
 * rather than optional numbers is what stops the health subscription being
 * handed placeholders it would report as a dead server. The desk arm is one
 * of that machine's own chats, which it runs and keeps: read and carried on
 * here, never changed. A chat's machine is fixed: the picker, which moves a
 * chat between this machine's models, is offered on the local arm alone.
 */
export type ChatPlaceProps = {
  modelName: string;
  /** The conversation the address names; `null` for an empty thread. */
  conversationId: number | null;
  draft?: ChatDraft; // Unsent text and images to put back in the composer, e.g. after a model switch
  startingModel?: string | null; // The model Load it again is starting, which its line says
  // Load the model again, from the line that says it was unloaded; local only.
  onLoadModel?: () => Promise<void>;
  // Move the chat to another model, keeping open the conversation (and the
  // draft) that `context` reads when the switch lands; local only.
  onSwitchModel?: (choice: ModelChoice, context: () => { conversationId: number | null; draft: ChatDraft }) => Promise<void>;
  // Unload the model, which every client of the proxy loses with it; the
  // chat stays open, its composer saying so. Local only.
  onUnloadModel?: () => Promise<void>;
  /** The open conversation is now `id`: the first send made it, a branch, or Start over. The address follows. */
  onOpened?: (id: number) => void;
  /** The open conversation was deleted. */
  onDeleted?: () => void;
  /** Open the model's inspector, where its log is; local only. */
  onShowModel?: () => void;
  /** Open the model's log, after a crash; local only. */
  onShowLog?: () => void;
  /** Open a new chat on the paired machine's model, by its id there; local only. */
  onOpenFar?: (id: number) => void;
} & (
  | { paired?: undefined; desk?: undefined; serverPort?: number; modelId: number }
  | {
      /** The paired machine's model, and the name that machine is shown by. */
      paired: { far: ModelRef; machineName: string };
      desk?: undefined;
      serverPort?: undefined;
      modelId?: undefined;
    }
  | {
      /** One of the paired machine's own chats, and the name that machine is shown by. */
      desk: { machineName: string };
      paired?: undefined;
      serverPort?: undefined;
      modelId?: undefined;
    }
);

export default function ChatPlace(props: ChatPlaceProps) {
  const {
    serverPort,
    modelId,
    modelName,
    conversationId,
    draft,
    startingModel = null,
    onLoadModel,
    onSwitchModel,
    onUnloadModel,
    onOpened,
    onDeleted,
    onShowModel,
    onShowLog,
    onOpenFar,
    paired,
    desk,
  } = props;
  const pairedChat = paired !== undefined;
  const source = desk ? 'far' : 'this';
  const far = source === 'far';
  const [chatError, setChatError] = useState<string | null>(null);

  // The open conversation: the address's, or the one this page just opened
  // (a first send, a branch) until the address catches up with it.
  const [activeConversationId, setActiveConversationId] = useState(conversationId);
  const [addressed, setAddressed] = useState(conversationId);
  if (conversationId !== addressed) {
    setAddressed(conversationId);
    setActiveConversationId(conversationId);
  }
  // Read by a model switch when it lands, which can be well after the pick.
  const activeRef = useRef(activeConversationId);
  useEffect(() => {
    activeRef.current = activeConversationId;
  }, [activeConversationId]);
  const open = (id: number) => {
    setActiveConversationId(id);
    void refreshChats(source);
    onOpened?.(id);
  };

  const conversations = useChats(source).list ?? NO_CHATS;

  // The model a conversation made here keeps: a first send's, and Start over's.
  const chatModel: ModelRef | undefined =
    paired?.far ?? (modelId === undefined ? undefined : { machine: { kind: 'local' }, id: modelId });
  const syncConversations = () => refreshChats(source);

  // What a chat remembers can change on another device, and only its
  // machine's list says it: read it again as one is opened.
  useEffect(() => {
    if (activeConversationId !== null) void refreshChats(source);
  }, [activeConversationId, source]);

  const { showToast } = useToastContext();
  const { confirm } = useConfirmContext();

  const { settings } = useSettings();
  const titleGenerationPrompt = settings?.titleGenerationPrompt || DEFAULT_TITLE_GENERATION_PROMPT;

  // Tool support and quantisation for the active model. The model is fixed
  // for the lifetime of the place: a switch from the composer's picker
  // remounts it on the new model, with this conversation still open.
  const { supportsToolCalls, toolFormat, quantization, sees, contextLength: servedContext, thinks } = useChatModelFacts(modelId);
  const imageInput = useImageInput({ far, paired: paired?.far, sees, contextLength: servedContext });

  const activeConversation = conversations.find((c) => c.id === activeConversationId) ?? null;

  // The Thinking switch: shown where the chat's model thinks, saying what the
  // chat remembers. The far list tells none of a chat's settings, so a far
  // chat is kept as its machine last answered it, for this alone.
  const [farOpen, setFarOpen] = useState<HubChatOpen | null>(null);
  const thinking = useThinkingSwitch({ conversationId: activeConversationId, conversations, far, farOpen, paired: paired?.far, thinks });

  // The folder the chat's file tools read: chosen here, sent on the run that
  // changes it; none on a far chat.
  const folder = useChatFolder({ conversationId: activeConversationId, conversations, far, supportsToolCalls, sentTo: paired?.machineName });

  // The Draw button: greyed, with the reason, where a message cannot draw;
  // pressed, the next send says so, and that send alone.
  const draw = useDrawSwitch({ conversationId: activeConversationId, far, paired: paired?.far, supportsToolCalls, refresh: settings?.defaultImageModelId });

  // The tools its runs may call, saved with the chat as Tools ▸ ticks them;
  // a chat not yet made saves its choice as its first send makes it.
  const tools = useChatTools({ conversationId: activeConversationId, conversations, far, onError: (sentence) => showToast(sentence, 'error') });

  // Runtime: sends start runs the daemon owns and saves; opening a
  // conversation shows what is saved, then the run still going in it.
  const { runtime, isLoading: messageLoading, endedRun, previews, timingTracker, currentStreamingAssistantMessageId, branching } = useGglibRuntime({
    conversationId: activeConversationId ?? undefined,
    conversation: activeConversation,
    source,
    onConversationChanged: open,
    onBranched: (_id, unanswered) => unanswered
      ? showToast(`Saved as a new branch, but it was not answered: ${unanswered.message}`, 'warning')
      : showToast('Saved as a new branch. The original is kept.', 'success'),
    model: chatModel,
    onError: (error) => setChatError(error.message),
    // Non-fatal: the turn is still running, so this is a transient notice
    // rather than `chatError`, which renders as a failed turn.
    onSystemWarning: (message, suggestedAction) =>
      showToast(suggestedAction ? `${message} — ${suggestedAction}` : message, 'warning'),
    // assistant-ui only logs a paste or a drop that fails; this is the person told.
    onImageRefused: (sentence) => showToast(sentence, 'error'),
    thinking: thinking.forSend,
    folder: folder.forSend,
    draw: draw.forSend,
    tools: tools?.forCreate,
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
  // Note: If serverState is null (no event received yet), the composer is
  // live: a model not running is loaded by the turn that needs it.
  // A paired or far chat subscribes to nothing — this registry only knows
  // servers started here, so its silence about the far machine must not be
  // read as that machine being down and the composer locked.
  const serverState = useServerState(modelId ?? -1);
  const isServerRunning =
    pairedChat || far || (serverState?.status !== 'stopped' && serverState?.status !== 'crashed');
  // A model gone mid-chat replaces the composer with a line that says so: no
  // banner and no toast. Never for a paired or far chat, which this
  // registry knows nothing of.
  const modelGone = pairedChat || far ? null
    : serverState?.status === 'crashed' ? 'crashed'
      : serverState?.status === 'stopped' ? 'unloaded'
        : null;

  // An error belongs to the conversation it happened in.
  useEffect(() => setChatError(null), [activeConversationId]);

  // What changes a conversation: this machine's only, never a far chat.
  const actions = useConversationActions({
    far, activeConversation, confirm, onError: setChatError, onOpened: open, onDeleted, model: chatModel,
  });

  return (
    <div className="flex-1 flex flex-col min-h-0 min-w-0 bg-background">
      <AssistantRuntimeProvider runtime={runtime}>
        <ChatMessagesPanel
          key={activeConversationId ?? 'none'}
          activeConversation={activeConversation}
          activeConversationId={activeConversationId}
          isServerConnected={isServerRunning}
          serverPort={far ? undefined : serverPort}
          titleGenerationPrompt={titleGenerationPrompt}
          onRenameConversation={actions.rename}
          onClearConversation={actions.startOver}
          onExportConversation={actions.exportChat}
          onUpdateSystemPrompt={actions.updateSystemPrompt}
          onDeleteConversation={actions.remove}
          messageLoading={messageLoading}
          syncConversations={syncConversations}
          chatError={chatError}
          showToast={showToast}
          timingTracker={timingTracker}
          currentStreamingAssistantMessageId={currentStreamingAssistantMessageId}
          endedRun={endedRun}
          supportsToolCalls={far ? null : supportsToolCalls}
          toolFormat={far ? null : toolFormat}
          modelName={modelName}
          modelId={far ? undefined : modelId}
          source={source}
          onPickModel={onSwitchModel && ((choice) => onSwitchModel(choice, () => ({
            conversationId: activeRef.current,
            draft: {
              text: runtime.thread.composer.getState().text,
              images: runtime.thread.composer.getState().attachments.flatMap((a) => a.file ?? []),
            },
          })))}
          startingModel={startingModel}
          onLoadModel={onLoadModel}
          onUnloadModel={onUnloadModel}
          quantization={far ? null : quantization}
          imageInput={imageInput}
          thinking={thinking}
          folder={folder}
          branching={far ? undefined : branching}
          draw={draw}
          tools={tools}
          previews={previews}
          deskName={paired?.machineName ?? desk?.machineName}
          modelGone={modelGone}
          onShowModel={onShowModel}
          onShowLog={onShowLog}
          onOpenFar={onOpenFar}
        />
      </AssistantRuntimeProvider>
    </div>
  );
}
