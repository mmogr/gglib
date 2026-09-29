import { FC, ReactNode } from 'react';
import { Download, Pencil, RotateCcw, Sparkles } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { Input } from '../../ui/Input';
import { cn } from '../../../utils/cn';
import { TurnRow } from './TurnRow';

interface ChatPanelHeaderProps {
  title: string;
  /** Whether the assistant is currently producing a response. */
  isThreadRunning: boolean;
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
  /** The head's margin: the page's own controls. */
  margin?: ReactNode;
  /** Under the title: the system prompt. */
  children?: ReactNode;
}

/**
 * The notebook's head, on the turns' grid: the conversation's title (or its
 * rename field) and its actions in the body, the page's controls in the
 * margin.
 */
export const ChatPanelHeader: FC<ChatPanelHeaderProps> = ({
  title,
  isThreadRunning,
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
  margin,
  children,
}) => (
  <TurnRow
    className="border-t-0 pt-0"
    who={margin}
    body={
      <div className="flex flex-col gap-md">
        <div className="flex flex-wrap items-center gap-sm min-w-0">
          {isRenaming ? (
            <Input
              aria-label="Conversation title"
              className="text-2xl font-semibold bg-background border border-primary rounded-sm py-xs px-sm text-text min-w-[150px]"
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
            <h2 className="text-3xl font-semibold leading-tight m-0 min-w-0 break-words">{title}</h2>
          )}
          <div className="flex gap-xs shrink-0">
            <Button variant="ghost" size="sm" title="Rename conversation" onClick={onStartRename} iconOnly>
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
            <Button variant="ghost" size="sm" onClick={onClearConversation} title="Restart conversation" iconOnly>
              <Icon icon={RotateCcw} size={14} />
            </Button>
            <Button variant="ghost" size="sm" onClick={onExportConversation} title="Export conversation" iconOnly>
              <Icon icon={Download} size={14} />
            </Button>
          </div>
        </div>
        {children}
      </div>
    }
  />
);
