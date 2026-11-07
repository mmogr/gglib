import { FC, ReactNode } from 'react';
import { Input } from '../../ui/Input';
import { StatusMark } from '../../status/StatusMark';
import { Menu, type MenuItem } from '../../../places/library/Menu';
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
  /** Open the system prompt's editor above the first turn. */
  onEditSystemPrompt: () => void;
  onClearConversation: () => Promise<void>;
  onExportConversation: () => void;
  onDeleteConversation: () => void;
  /** Under the title: the system prompt's line, or its editor. */
  children?: ReactNode;
  /** A chat this page reads but does not change: the title alone, no ▾. */
  readOnly?: boolean;
}

const TITLE = 'text-base font-semibold leading-tight';

/**
 * The chat's head: its title (16/600), which is the menu button for what
 * changes the chat: Rename · Generate title · System prompt… · Export ·
 * Start over (same prompt)… — Delete…. A far chat is changed only on its
 * own machine, so its title is plain text.
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
  onEditSystemPrompt,
  onClearConversation,
  onExportConversation,
  onDeleteConversation,
  children,
  readOnly = false,
}) => {
  const generateBlocked = generateTitleBlockedReason ?? (isThreadRunning ? 'Wait for the reply to finish' : null);
  const items: MenuItem[] = [
    { label: 'Rename', onPick: onStartRename },
    {
      label: 'Generate title',
      onPick: onGenerateTitle,
      disabled: generateBlocked !== null || isGeneratingTitle,
      title: generateBlocked ?? 'Ask the model for a title',
    },
    { label: 'System prompt…', onPick: onEditSystemPrompt },
    { label: 'Export', onPick: onExportConversation },
    { label: 'Start over (same prompt)…', onPick: () => void onClearConversation() },
    { label: 'Delete…', onPick: onDeleteConversation, danger: true, gapBefore: true },
  ];
  return (
    <TurnRow
      className="border-t-0 pt-0"
      who={null}
      body={
        <div className="flex flex-col gap-sm">
          <div className="flex flex-wrap items-center gap-sm min-w-0">
            {isRenaming ? (
              <Input
                aria-label="Conversation title"
                className={`${TITLE} bg-background border border-border rounded-sm py-xs px-sm text-text min-w-[150px]`}
                value={titleDraft}
                autoFocus
                onChange={(e) => onChangeTitleDraft(e.target.value)}
                onBlur={onCommitRename}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') onCommitRename();
                  else if (e.key === 'Escape') onCancelRename();
                }}
              />
            ) : readOnly ? (
              <h2 className={`${TITLE} m-0 min-w-0 break-words`}>{title}</h2>
            ) : (
              <h2 className="m-0 min-w-0">
                <Menu
                  label={<span className={`${TITLE} text-text break-words text-left`}>{title}</span>}
                  items={items}
                  align="start"
                  className="h-auto px-0 hover:bg-transparent"
                />
              </h2>
            )}
            {isGeneratingTitle && <StatusMark kind="changing" label="Generating title…" />}
          </div>
          {children}
        </div>
      }
    />
  );
};
