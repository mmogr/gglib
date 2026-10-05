import { FC, useContext } from 'react';
import { ComposerPrimitive } from '@assistant-ui/react';
import { Brain, CircleOff } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Chip } from '../../ui/Chip';
import { Icon } from '../../ui/Icon';
import { ToolsPopover } from '../../ToolsPopover';
import { ToolSupportIndicator } from '../../ToolSupportIndicator';
import { getToolRegistry } from '../../../services/tools';
import type { ThinkingSwitch } from '../../../hooks/useThinkingSwitch';
import { TurnRow } from './TurnRow';
import { ModelPicker, type ModelChoice } from './ModelPicker';
import { AttachImageButton, ComposerImages, ImageInputContext } from './ComposerImages';
import { ContextRing } from './ContextRing';

interface ComposerFooterProps {
  isServerConnected: boolean;
  /** Whether the assistant is currently producing a response. */
  isThreadRunning: boolean;
  onStopGeneration: () => void;
  /** The model the next send goes to, as the page names it. */
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
}

const THINKING_ON = 'Thinking is on for this chat. Click to switch it off from the next message.';
const THINKING_OFF = 'Thinking is off for this chat. Click to switch it back on from the next message.';

/**
 * The composer, on the notebook's grid: the model (a picker, on this
 * machine), the tools, the context ring and the Thinking switch in the
 * margin; in the body, the images attached, then the attach button, the text
 * box and Stop or Send. An image is attached by the button, a paste or a
 * drop, each only where the model takes images, as `ImageInputContext` says.
 * The switch is a button, pressed while the chat thinks and there only for
 * a model that does; it changes what the next send says, never a reply
 * already being written. Its name is "Thinking" either way; switched off it
 * reads "Thinking off" beside another icon, so which way it is never rests
 * on its colours alone.
 */
export const ComposerFooter: FC<ComposerFooterProps> = ({
  isServerConnected,
  isThreadRunning,
  onStopGeneration,
  modelName,
  modelId,
  onPickModel,
  startingModel,
  onUnloadModel,
  quantization,
  supportsToolCalls,
  toolFormat,
  thinking,
}) => {
  const imageInput = useContext(ImageInputContext);
  return (
    <div className="@container shrink-0 mx-auto w-full max-w-[1000px] px-lg">
      <TurnRow
        className="border-t-0 pt-md pb-lg"
        who={
          <ModelPicker
            modelId={modelId}
            modelName={modelName}
            quantization={quantization}
            onPick={onPickModel}
            starting={startingModel}
            onUnload={isServerConnected ? onUnloadModel : undefined}
          />
        }
        made={
          <div className="flex flex-wrap items-center justify-end gap-sm">
            <ToolSupportIndicator
              supports={supportsToolCalls ?? null}
              hasToolsConfigured={getToolRegistry().getEnabledDefinitions().length > 0}
              toolFormat={toolFormat}
            />
            <ToolsPopover opensUpward align="left" modelId={modelId} />
            <ContextRing />
            {thinking?.shown && (
              <Chip
                leftIcon={<Icon icon={thinking.on ? Brain : CircleOff} size={12} />}
                selected={thinking.on}
                onClick={thinking.toggle}
                title={thinking.on ? THINKING_ON : THINKING_OFF}
              >
                Thinking{!thinking.on && <span aria-hidden="true"> off</span>}
              </Chip>
            )}
          </div>
        }
        body={
          <ComposerPrimitive.AttachmentDropzone asChild disabled={!imageInput.offered || !isServerConnected}>
            <ComposerPrimitive.Root className="flex flex-col gap-sm py-sm pr-sm pl-base border border-border rounded-lg bg-background-input focus-within:border-border-focus data-[dragging=true]:border-border-focus">
              <ComposerImages />
              <div className="flex gap-sm items-end">
                <AttachImageButton disabled={!isServerConnected} />
                <ComposerPrimitive.Input
                  aria-label="Message"
                  addAttachmentOnPaste={imageInput.offered}
                  className="flex-1 py-xs bg-transparent text-text text-base placeholder:text-text-disabled resize-none min-h-[40px] max-h-[150px] outline-none disabled:opacity-50 disabled:cursor-not-allowed"
                  placeholder={
                    isServerConnected
                      ? 'Type your message. Shift + Enter for newline'
                      : 'Server not connected'
                  }
                  disabled={!isServerConnected}
                />
                <div className="flex gap-sm shrink-0">
                  {isThreadRunning && (
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={onStopGeneration}
                      title="Stop generation"
                    >
                      Stop
                    </Button>
                  )}
                  <ComposerPrimitive.Send asChild>
                    <Button
                      variant="primary"
                      size="sm"
                      disabled={!isServerConnected}
                    >
                      Send ↵
                    </Button>
                  </ComposerPrimitive.Send>
                </div>
              </div>
            </ComposerPrimitive.Root>
          </ComposerPrimitive.AttachmentDropzone>
        }
      />
    </div>
  );
};
