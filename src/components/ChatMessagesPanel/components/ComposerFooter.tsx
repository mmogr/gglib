import { FC } from 'react';
import { ComposerPrimitive } from '@assistant-ui/react';
import { Button } from '../../ui/Button';
import { CouncilToggle } from '../../CouncilToggle';

interface ComposerFooterProps {
  isServerConnected: boolean;
  /** Whether the assistant is currently producing a response. */
  isThreadRunning: boolean;
  isCouncilMode: boolean;
  onToggleCouncil: () => void;
  onStopGeneration: () => void;
}

/**
 * Composer area: the thinking indicator, message input, council toggle, and
 * the stop / send controls.
 *
 * Council mode stays owned by the panel — it also drives the placeholder here
 * and the submit callback the runtime calls.
 */
export const ComposerFooter: FC<ComposerFooterProps> = ({
  isServerConnected,
  isThreadRunning,
  isCouncilMode,
  onToggleCouncil,
  onStopGeneration,
}) => (
  <div className="border-t border-border p-md shrink-0">
    {isThreadRunning && (
      <div className="text-sm text-primary mb-sm animate-research-pulse">Assistant is thinking…</div>
    )}
    <ComposerPrimitive.Root className="flex gap-sm items-end">
      <ComposerPrimitive.Input
        className="flex-1 py-sm px-md border border-border rounded-base bg-surface text-text text-sm font-[inherit] resize-none min-h-[40px] max-h-[150px] focus:outline-none focus:border-primary disabled:opacity-50 disabled:cursor-not-allowed"
        placeholder={
          isServerConnected
            ? isCouncilMode
              ? 'Describe the goal for the orchestrator…'
              : 'Type your message. Shift + Enter for newline'
            : 'Server not connected'
        }
        disabled={!isServerConnected}
      />
      <CouncilToggle
        active={isCouncilMode}
        onToggle={onToggleCouncil}
        disabled={!isServerConnected}
      />
      <div className="flex gap-sm shrink-0">
        {isThreadRunning && (
          <Button
            variant="danger"
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
    </ComposerPrimitive.Root>
  </div>
);
