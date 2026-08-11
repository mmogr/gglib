import { FC } from 'react';
import { ComposerPrimitive } from '@assistant-ui/react';
import { Button } from '../../ui/Button';

interface ComposerFooterProps {
  isServerConnected: boolean;
  /** Whether the assistant is currently producing a response. */
  isThreadRunning: boolean;
  onStopGeneration: () => void;
}

/**
 * Composer area: the thinking indicator, message input, and the stop / send
 * controls.
 */
export const ComposerFooter: FC<ComposerFooterProps> = ({
  isServerConnected,
  isThreadRunning,
  onStopGeneration,
}) => (
  <div className="border-t border-border-light p-md shrink-0">
    {isThreadRunning && (
      <div className="text-sm text-primary mb-sm animate-research-pulse">Assistant is thinking…</div>
    )}
    <ComposerPrimitive.Root className="flex gap-sm items-end">
      <ComposerPrimitive.Input
        className="flex-1 py-sm px-md border border-border rounded-base bg-background-input text-text text-sm placeholder:text-text-disabled resize-none min-h-[40px] max-h-[150px] outline-none focus-visible:border-border-focus focus-visible:ring-2 focus-visible:ring-primary/10 hover:border-border-hover disabled:opacity-50 disabled:cursor-not-allowed"
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
            variant="dangerGhost"
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
