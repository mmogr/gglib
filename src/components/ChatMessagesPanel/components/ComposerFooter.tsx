import { FC, useContext, useState } from 'react';
import { ComposerPrimitive, useThread, type ThreadState } from '@assistant-ui/react';
import { ArrowUp, Square, X } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { IconButton } from '../../ui/IconButton';
import { StatusMark } from '../../status/StatusMark';
import { REACH_WORDS, RESIDENCY_WORDS } from '../../status/words';
import { useRemoteState } from '../../../services/remoteRegistry';
import { useToastContext } from '../../../contexts/ToastContext';
import { awaySince } from '../../../places/devices/devices';
import { cn } from '../../../utils/cn';
import { formatError } from '../../../utils/errors';
import type { ThinkingSwitch } from '../../../hooks/useThinkingSwitch';
import type { DrawSwitch } from '../../../hooks/useDrawSwitch';
import type { ChatFolder } from '../../../hooks/useChatFolder';
import { useModelLoadWait } from '../hooks/useModelLoadWait';
import { FolderChip } from './FolderChip';
import { ModelPicker, type ModelChoice } from './ModelPicker';
import { ComposerImages, ImageInputContext } from './ComposerImages';
import { ContextRing } from './ContextRing';
import { ModelGoneLine, type ModelGone } from './ModelGoneLine';
import { PlusMenu } from './PlusMenu';
import { DRAW_ARMED } from './composerMenus';

interface ComposerFooterProps {
  isServerConnected: boolean;
  /** Whether the assistant is currently producing a response. */
  isThreadRunning: boolean;
  onStopGeneration: () => void;
  /** The model the next send goes to, by its bare name; empty for a far chat, whose machine picks it. */
  modelName: string;
  /** Its registry id; absent for a chat with another machine. */
  modelId?: number;
  /** Move the chat to another model; absent where there is none to pick. */
  onPickModel?: (choice: ModelChoice) => Promise<void>;
  /** The model a switch is starting; the picker is locked until it lands. */
  startingModel?: string | null;
  /** Unload the model; offered only while its server is up. */
  onUnloadModel?: () => Promise<void>;
  /** Its quantisation, from the model's catalogue entry, when known. */
  quantization?: string | null;
  /** null = capability status not yet resolved. */
  supportsToolCalls?: boolean | null;
  toolFormat?: string | null;
  /** The chat's Thinking switch; drawn only where it says it is shown. */
  thinking?: ThinkingSwitch;
  /** The chat's folder, where its file tools read. */
  folder?: ChatFolder;
  /** The Draw switch, which `[+]` › Draw a picture presses. */
  draw?: DrawSwitch;
  /** A far chat: one of the paired machine's own. */
  far?: boolean;
  /** Whether the chat is a saved conversation (its turns may still be loading), not a new one. */
  saved?: boolean;
  /** The paired machine the chat's model is on (a paired or a far chat), for the away line. */
  deskName?: string;
  /** This machine's model is no longer loaded: the composer is replaced by a line that says so. */
  modelGone?: ModelGone | null;
  /** Open the model's inspector, where its log is (Show <model>). */
  onShowModel?: () => void;
  /** Open the model's log (a crash's Show log). */
  onShowLog?: () => void;
  /** Open a new chat on the paired machine's model `id`. */
  onOpenFar?: (id: number) => void;
}

/** Whether the thread has no turns yet: a new chat's placeholder names the model. */
const isEmpty = (thread: ThreadState) => thread.messages.length === 0;

/** The Thinking toggle's titles: what a click does from the next message. */
const THINKING_ON = 'Thinking is on for this chat. Click to switch it off from the next message.';
const THINKING_OFF = 'Thinking is off for this chat. Click to switch it back on from the next message.';

/**
 * Thinking, as a labelled toggle: a switch named Thinking, neutral whichever
 * way it is, its track saying which.
 */
const ThinkingToggle: FC<{ thinking: ThinkingSwitch }> = ({ thinking }) => (
  <Button
    type="button"
    variant="ghost"
    size="sm"
    role="switch"
    aria-checked={thinking.on}
    onClick={thinking.toggle}
    title={thinking.on ? THINKING_ON : THINKING_OFF}
    className="font-normal"
    leftIcon={<Track on={thinking.on} />}
  >
    Thinking
  </Button>
);

/** The switch's track and knob: which way it is, beside its label. */
const Track: FC<{ on: boolean }> = ({ on }) => (
  <span
    aria-hidden
    className={cn(
      'relative inline-block h-3 w-[22px] shrink-0 rounded-full transition-colors duration-150',
      on ? 'bg-text-secondary' : 'bg-surface-elevated',
    )}
  >
    <span
      className={cn(
        'absolute top-[2px] h-2 w-2 rounded-full transition-[left] duration-150',
        on ? 'left-[12px] bg-background' : 'left-[2px] bg-text-muted',
      )}
    />
  </span>
);

/** `Draw ×`: the next message draws; × sends it without. */
const DrawChip: FC<{ draw: DrawSwitch }> = ({ draw }) => (
  <span className="inline-flex items-center rounded-base bg-surface-elevated pl-sm text-xs text-text-secondary" title={DRAW_ARMED}>
    Draw
    <IconButton type="button" label="Don't draw" size="sm" variant="ghost" className="text-current" onClick={draw.toggle}>
      <Icon icon={X} size={12} />
    </IconButton>
  </span>
);

/** The paired machine's away line, from the remote registry alone (CH-9); null while it is reached. */
function useAwayLine(deskName: string | undefined): string | null {
  const { status } = useRemoteState();
  const awayFor = status?.connected?.away_for_s;
  if (deskName === undefined || awayFor == null) return null;
  return `${deskName} is ${REACH_WORDS.away.toLowerCase()} since ${awaySince(awayFor)}. Your message will wait.`;
}

/**
 * The composer: the images attached, as tiles above the text; the text box;
 * then one row of `[+]`, the chips that are set (the chat's folder, Draw),
 * the Thinking toggle for a model that thinks, the context ring once there
 * is a reading, the model, and Send, which becomes Stop while a reply is
 * written. An image is attached from `[+]`, a paste or a drop, each only
 * where the model takes images, as `ImageInputContext` says.
 *
 * The model is a picker on this machine's chat and plain text on one with
 * the paired machine ("<model> on desk", "desk picks the model"); while
 * that machine is away it reads "desk is away since 14:02. Your message
 * will wait." and Send is disabled. While the newest reply waits for its
 * model to load it reads "Loading <model>… 12 s". Where this machine's
 * model is gone, the whole composer is one line that says so (`ModelGoneLine`).
 */
export const ComposerFooter: FC<ComposerFooterProps> = ({
  isServerConnected,
  isThreadRunning,
  onStopGeneration,
  modelName,
  modelId,
  onPickModel,
  startingModel = null,
  onUnloadModel,
  quantization,
  supportsToolCalls,
  toolFormat,
  thinking,
  folder,
  draw,
  far = false,
  saved = false,
  deskName,
  modelGone = null,
  onShowModel,
  onShowLog,
  onOpenFar,
}) => {
  const imageInput = useContext(ImageInputContext);
  const { showToast } = useToastContext();
  const empty = useThread({ optional: true, selector: isEmpty }) ?? true;
  const [typingFolder, setTypingFolder] = useState(false);
  const away = useAwayLine(deskName);
  const loadWait = useModelLoadWait();

  if (modelGone !== null) {
    const loadAgain =
      onPickModel && modelId !== undefined
        ? () =>
            void onPickModel({ modelId, modelName }).catch((error: unknown) =>
              showToast(`Could not start ${modelName}: ${formatError(error)}`, 'error'),
            )
        : undefined;
    return (
      <div className="mx-auto w-full max-w-[1000px] shrink-0 px-lg pb-lg">
        <ModelGoneLine gone={modelGone} modelName={modelName} starting={startingModel} onLoadAgain={loadAgain} onShowLog={onShowLog} />
      </div>
    );
  }

  // The model as the picker names it: on the paired machine, it says so.
  const named = far ? `${deskName} picks the model` : deskName !== undefined ? `${modelName} on ${deskName}` : modelName;
  const model = away ? (
    <span role="status" className="flex min-w-0 items-center gap-xs px-sm text-sm text-text-secondary">
      <StatusMark kind="away" />
      <span className="truncate">{away}</span>
    </span>
  ) : loadWait !== null ? (
    <span role="status" className="flex items-center gap-xs px-sm text-sm text-text-secondary tabular-nums">
      <StatusMark kind="changing" />
      {RESIDENCY_WORDS.loading} {far ? 'the model' : modelName}… {loadWait} s
    </span>
  ) : (
    <ModelPicker
      modelId={modelId}
      modelName={named}
      quantization={quantization}
      onPick={onPickModel}
      starting={startingModel}
      onUnload={isServerConnected ? onUnloadModel : undefined}
      onShow={onShowModel}
      onOpenFar={saved ? undefined : onOpenFar}
    />
  );

  const sendable = isServerConnected && away === null;
  // A new chat asks; a saved one replies, once its turns are known to be there.
  const placeholder = !saved && !far ? `Ask ${modelName} anything` : empty ? '' : 'Reply…';

  return (
    <div className="mx-auto w-full max-w-[1000px] shrink-0 px-lg pb-lg pt-md">
      <ComposerPrimitive.AttachmentDropzone asChild disabled={!imageInput.offered || !isServerConnected}>
        <ComposerPrimitive.Root className="flex flex-col gap-sm rounded-lg bg-background-input p-sm data-[dragging=true]:bg-surface-elevated">
          <ComposerImages />
          <ComposerPrimitive.Input
            aria-label="Message"
            addAttachmentOnPaste={imageInput.offered}
            submitOnEnter={sendable}
            className="max-h-[150px] min-h-[40px] resize-none bg-transparent px-sm py-xs text-base text-text outline-none placeholder:text-text-muted disabled:cursor-not-allowed disabled:opacity-50"
            placeholder={placeholder}
            disabled={!isServerConnected}
          />
          <div className="flex items-center gap-xs">
            <PlusMenu
              connected={isServerConnected}
              far={far}
              modelId={modelId}
              supportsToolCalls={supportsToolCalls}
              toolFormat={toolFormat}
              folder={folder}
              onTypeFolder={setTypingFolder}
              draw={draw}
            />
            {folder && <FolderChip folder={folder} typing={typingFolder} setTyping={setTypingFolder} />}
            {draw?.armed && <DrawChip draw={draw} />}
            {thinking?.shown && <ThinkingToggle thinking={thinking} />}
            <span className="flex-1" />
            <ContextRing />
            {model}
            {isThreadRunning ? (
              <Button type="button" variant="primary" size="sm" onClick={onStopGeneration} leftIcon={<Icon icon={Square} size={12} />}>
                Stop
              </Button>
            ) : (
              <ComposerPrimitive.Send asChild>
                <IconButton label="Send" variant="primary" size="sm" disabled={!sendable}>
                  <Icon icon={ArrowUp} size={16} />
                </IconButton>
              </ComposerPrimitive.Send>
            )}
          </div>
        </ComposerPrimitive.Root>
      </ComposerPrimitive.AttachmentDropzone>
    </div>
  );
};
