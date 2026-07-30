import { FC } from 'react';
import { Download, Pencil, RotateCcw, Sparkles } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { Input } from '../../ui/Input';
import { ToolsPopover } from '../../ToolsPopover';
import { ToolSupportIndicator } from '../../ToolSupportIndicator';
import { getToolRegistry } from '../../../services/tools';
import { cn } from '../../../utils/cn';

interface ChatPanelHeaderProps {
  title: string;
  /** Whether the assistant is currently producing a response. */
  isThreadRunning: boolean;
  /** null = capability status not yet resolved. */
  supportsToolCalls?: boolean | null;
  toolFormat?: string | null;
  /** Why title generation is unavailable, or null when it is available. */
  generateTitleBlockedReason: string | null;
  isRenaming: boolean;
  titleDraft: string;
  isGeneratingTitle: boolean;
  onStartRename: () => void;
  onChangeTitleDraft: (value: string) => void;
  onCommitRename: () => void;
  onCancelRename: () => void;
  onGenerateTitle: () => void;
  onClearConversation: () => Promise<void>;
  onExportConversation: () => void;
}

/**
 * Chat panel title bar: the conversation title (or its rename field), AI title
 * generation, live status badge, tool-support indicator, and the conversation
 * actions.
 */
export const ChatPanelHeader: FC<ChatPanelHeaderProps> = ({
  title,
  isThreadRunning,
  supportsToolCalls,
  toolFormat,
  generateTitleBlockedReason,
  isRenaming,
  titleDraft,
  isGeneratingTitle,
  onStartRename,
  onChangeTitleDraft,
  onCommitRename,
  onCancelRename,
  onGenerateTitle,
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
          onChange={(e) => onChangeTitleDraft(e.target.value)}
          onBlur={onCommitRename}
          onKeyDown={(e) => {
            if (e.key === 'Enter') onCommitRename();
            else if (e.key === 'Escape') onCancelRename();
          }}
        />
      ) : (
        <h2 className="text-lg font-semibold m-0 overflow-hidden text-ellipsis whitespace-nowrap">{title}</h2>
      )}
      <Button
        variant="ghost"
        size="sm"
        title="Rename conversation"
        onClick={onStartRename}
        iconOnly
      >
        <Icon icon={Pencil} size={14} />
      </Button>
      <Button
        variant="ghost"
        size="sm"
        className={cn(isGeneratingTitle && 'pointer-events-none')}
        title={generateTitleBlockedReason ?? 'Generate title with AI'}
        onClick={onGenerateTitle}
        disabled={!!generateTitleBlockedReason || isGeneratingTitle || isThreadRunning}
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
        supports={supportsToolCalls ?? null}
        hasToolsConfigured={getToolRegistry().getEnabledDefinitions().length > 0}
        toolFormat={toolFormat}
      />
    </div>
    <div className="flex gap-sm shrink-0">
      <ToolsPopover />
      <Button variant="ghost" size="sm" onClick={onClearConversation} title="Restart conversation" iconOnly>
        <Icon icon={RotateCcw} size={14} />
      </Button>
      <Button variant="ghost" size="sm" onClick={onExportConversation} title="Export conversation" iconOnly>
        <Icon icon={Download} size={14} />
      </Button>
    </div>
  </div>
);
