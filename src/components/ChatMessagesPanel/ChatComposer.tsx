import React from 'react';
import {
  ComposerPrimitive,
  useComposerRuntime,
} from '@assistant-ui/react';
import { Button } from '../ui/Button';
import { DeepResearchToggle } from '../DeepResearch';
import type { UseDeepResearchReturn } from '../../hooks/useDeepResearch';

interface ChatComposerProps {
  isServerConnected: boolean;
  isThreadRunning: boolean;
  isDeepResearchEnabled: boolean;
  toggleDeepResearch: () => void;
  deepResearch: Pick<UseDeepResearchReturn, 'isRunning' | 'requestWrapUp' | 'state'>;
  stopDeepResearch: () => void;
  onDeepResearchSubmit: (query: string) => void;
  onStopGeneration: () => void;
}

export const ChatComposer: React.FC<ChatComposerProps> = ({
  isServerConnected,
  isThreadRunning,
  isDeepResearchEnabled,
  toggleDeepResearch,
  deepResearch,
  stopDeepResearch,
  onDeepResearchSubmit,
  onStopGeneration,
}) => {
  const composerRuntime = useComposerRuntime({ optional: true });

  return (
    <div className="border-t border-border p-md shrink-0">
      {isThreadRunning && !deepResearch.isRunning && (
        <div className="text-sm text-primary mb-sm animate-research-pulse">Assistant is thinking…</div>
      )}
      {deepResearch.isRunning && (
        <div className="text-sm text-primary mb-sm animate-research-pulse">Researching… This may take a few minutes.</div>
      )}
      <ComposerPrimitive.Root className="flex gap-sm items-end">
        <ComposerPrimitive.Input
          className="flex-1 py-sm px-md border border-border rounded-base bg-surface text-text text-sm font-[inherit] resize-none min-h-[40px] max-h-[150px] focus:outline-none focus:border-primary disabled:opacity-50 disabled:cursor-not-allowed"
          placeholder={
            isServerConnected
              ? isDeepResearchEnabled
                ? 'Ask a research question (Deep Research mode)'
                : 'Type your message. Shift + Enter for newline'
              : 'Server not connected'
          }
          disabled={!isServerConnected || deepResearch.isRunning}
        />
        <div className="flex gap-sm shrink-0">
          <DeepResearchToggle
            isEnabled={isDeepResearchEnabled}
            onToggle={toggleDeepResearch}
            isRunning={deepResearch.isRunning}
            onStop={stopDeepResearch}
            onWrapUp={deepResearch.requestWrapUp}
            researchPhase={deepResearch.state?.phase}
            disabled={!isServerConnected || isThreadRunning}
            disabledReason={
              !isServerConnected
                ? 'Server not connected'
                : isThreadRunning
                ? 'Wait for current response'
                : undefined
            }
          />
          {isThreadRunning && !deepResearch.isRunning && (
            <Button
              variant="danger"
              size="sm"
              onClick={onStopGeneration}
              title="Stop generation"
            >
              Stop
            </Button>
          )}
          {isDeepResearchEnabled ? (
            <Button
              variant="primary"
              size="sm"
              disabled={!isServerConnected || deepResearch.isRunning}
              onClick={() => {
                if (!composerRuntime) return;
                const text = composerRuntime.getState().text.trim();
                if (!text) return;
                composerRuntime.setText('');
                onDeepResearchSubmit(text);
              }}
            >
              Research ↵
            </Button>
          ) : (
            <ComposerPrimitive.Send asChild>
              <Button
                variant="primary"
                size="sm"
                disabled={!isServerConnected}
              >
                Send ↵
              </Button>
            </ComposerPrimitive.Send>
          )}
        </div>
      </ComposerPrimitive.Root>
    </div>
  );
};
