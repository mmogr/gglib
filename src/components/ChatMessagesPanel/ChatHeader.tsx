import React from 'react';
import { Download, Mic, MicOff, Pencil, RotateCcw, Sparkles } from 'lucide-react';
import { Button } from '../ui/Button';
import { Icon } from '../ui/Icon';
import { Input } from '../ui/Input';
import { ToolsPopover } from '../ToolsPopover';
import { ToolSupportIndicator } from '../ToolSupportIndicator';
import { cn } from '../../utils/cn';
import { getToolRegistry } from '../../services/tools';
import type { UseVoiceModeReturn } from '../../hooks/useVoiceMode';

interface ChatHeaderProps {
  title: string | undefined;
  isRenaming: boolean;
  titleDraft: string;
  setTitleDraft: (value: string) => void;
  startRenaming: () => void;
  commitRename: () => void;
  cancelRenaming: () => void;
  isGeneratingTitle: boolean;
  generateTitle: () => void;
  isThreadRunning: boolean;
  activeConversationId: number | null;
  serverPort: number;
  supportsToolCalls: boolean | null;
  toolFormat: string | null | undefined;
  voice: UseVoiceModeReturn | undefined;
  onClearConversation: () => void;
  onExportConversation: () => void;
}

export const ChatHeader: React.FC<ChatHeaderProps> = ({
  title,
  isRenaming,
  titleDraft,
  setTitleDraft,
  startRenaming,
  commitRename,
  cancelRenaming,
  isGeneratingTitle,
  generateTitle,
  isThreadRunning,
  activeConversationId,
  serverPort,
  supportsToolCalls,
  toolFormat,
  voice,
  onClearConversation,
  onExportConversation,
}) => (
  <div className="p-base border-b border-border bg-background shrink-0 flex flex-wrap justify-between items-center gap-md phone:flex-nowrap">
    <div className="flex items-center gap-sm min-w-0 basis-full phone:basis-auto phone:flex-1">
      {isRenaming ? (
        <Input
          className="text-lg font-semibold bg-background border border-primary rounded-sm py-xs px-sm text-text min-w-[150px]"
          value={titleDraft}
          autoFocus
          onChange={(e) => setTitleDraft(e.target.value)}
          onBlur={commitRename}
          onKeyDown={(e) => {
            if (e.key === 'Enter') commitRename();
            else if (e.key === 'Escape') cancelRenaming();
          }}
        />
      ) : (
        <h2 className="text-lg font-semibold m-0 overflow-hidden text-ellipsis whitespace-nowrap">{title || 'New Chat'}</h2>
      )}
      <Button variant="ghost" size="sm" title="Rename conversation" onClick={startRenaming} iconOnly>
        <Icon icon={Pencil} size={14} />
      </Button>
      <Button
        variant="ghost"
        size="sm"
        className={cn(isGeneratingTitle && 'pointer-events-none')}
        title={
          !activeConversationId
            ? 'No active conversation'
            : !serverPort
              ? 'Start a server to generate titles'
              : 'Generate title with AI'
        }
        onClick={() => generateTitle()}
        disabled={!activeConversationId || !serverPort || isGeneratingTitle || isThreadRunning}
        iconOnly
      >
        {isGeneratingTitle ? (
          <span className="inline-block w-[14px] h-[14px] border-2 border-text-muted border-t-primary rounded-full animate-spin-360" aria-label="Generating title…" />
        ) : (
          <Icon icon={Sparkles} size={14} />
        )}
      </Button>
      <span className={cn('text-xs py-xs px-sm rounded-full bg-background text-text-muted shrink-0', isThreadRunning && 'bg-primary/10 text-primary animate-research-pulse')}>
        {isThreadRunning ? 'Responding…' : 'Idle'}
      </span>
      <ToolSupportIndicator
        supports={supportsToolCalls}
        hasToolsConfigured={getToolRegistry().getEnabledDefinitions().length > 0}
        toolFormat={toolFormat}
      />
    </div>
    <div className="flex gap-sm shrink-0">
      <ToolsPopover />
      {voice?.isSupported && (
        <Button
          variant="ghost"
          size="sm"
          className={cn(voice.isActive && 'text-error')}
          onClick={() => voice.isActive ? voice.stop() : voice.start()}
          title={voice.isActive ? 'Stop voice mode' : 'Start voice mode'}
          iconOnly
        >
          <Icon icon={voice.isActive ? MicOff : Mic} size={14} />
        </Button>
      )}
      <Button variant="ghost" size="sm" onClick={onClearConversation} title="Restart conversation" iconOnly>
        <Icon icon={RotateCcw} size={14} />
      </Button>
      <Button variant="ghost" size="sm" onClick={onExportConversation} title="Export conversation" iconOnly>
        <Icon icon={Download} size={14} />
      </Button>
    </div>
  </div>
);
