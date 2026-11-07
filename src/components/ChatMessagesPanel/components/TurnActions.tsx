import { FC, useContext } from 'react';
import { ActionBarPrimitive, useMessage } from '@assistant-ui/react';
import { MoreHorizontal } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { Menu, type MenuItem } from '../../ui/Menu';
import { cn } from '../../../utils/cn';
import { BranchingContext } from './BranchingContext';
import { MessageActionsContext } from './MessageActionsContext';

/** A 12 px text action: no box, no icon. */
const ACTION =
  'bg-transparent border-none p-0 text-xs text-text-secondary cursor-pointer hover:text-text disabled:opacity-50 disabled:cursor-default focus-visible:outline-2 focus-visible:outline-primary';

/** Copy, and "Copied" while assistant-ui says it copied. */
const CopyAction: FC<{ name: string }> = ({ name }) => (
  <ActionBarPrimitive.Copy className={cn(ACTION, 'group/copy')} aria-label={name}>
    <span className="group-data-[copied=true]/copy:hidden">Copy</span>
    <span className="hidden group-data-[copied=true]/copy:inline">Copied</span>
  </ActionBarPrimitive.Copy>
);

/** The ⋯ after the text actions; nothing where it would hold nothing. */
const MoreActions: FC<{ name: string; items: MenuItem[] }> = ({ name, items }) =>
  items.length === 0 ? null : (
    <Menu
      label={<Icon icon={MoreHorizontal} size={14} />}
      name={name}
      items={items}
      iconOnly
      align="start"
      className="h-auto min-h-0 p-0 text-text-secondary hover:bg-transparent hover:text-text"
    />
  );

/** Branch from here, where the chat has branches to make: not on a far chat. */
function useBranchItem(): MenuItem[] {
  const branching = useContext(BranchingContext);
  const id = useMessage((message) => message.id);
  if (!branching) return [];
  return [{ label: 'Branch from here', onPick: () => void branching.branchFrom(id) }];
}

/**
 * A reply's action line: Copy · Retry · Edit · ⋯ (Branch from here). Retry
 * answers the question again (on a new branch where it would rewrite a
 * saved reply); Edit is offered on a reply with text. A far chat's reply
 * offers Copy alone. Not drawn while the reply arrives.
 */
export const ReplyActions: FC<{ hasText: boolean }> = ({ hasText }) => {
  const far = useContext(MessageActionsContext)?.source === 'far';
  const branch = useBranchItem();
  return (
    <ActionBarPrimitive.Root className="flex items-center gap-sm">
      <CopyAction name="Copy reply" />
      {!far && (
        <ActionBarPrimitive.Reload className={ACTION} aria-label="Retry this reply">
          Retry
        </ActionBarPrimitive.Reload>
      )}
      {!far && hasText && (
        <ActionBarPrimitive.Edit className={ACTION} aria-label="Edit reply">
          Edit
        </ActionBarPrimitive.Edit>
      )}
      {!far && <MoreActions name="More for this reply" items={branch} />}
    </ActionBarPrimitive.Root>
  );
};

/**
 * A question's action line: Copy · Edit · ⋯ (Branch from here · Delete…,
 * which asks first). A far chat's question offers Copy alone.
 */
export const QuestionActions: FC = () => {
  const actions = useContext(MessageActionsContext);
  const id = useMessage((message) => message.id);
  const far = actions?.source === 'far';
  const items = [
    ...useBranchItem(),
    ...(actions && !far ? [{ label: 'Delete…', danger: true, onPick: () => actions.onDeleteMessage(id) }] : []),
  ];
  return (
    <ActionBarPrimitive.Root className="flex items-center gap-sm">
      <CopyAction name="Copy message" />
      {!far && (
        <ActionBarPrimitive.Edit className={ACTION} aria-label="Edit message">
          Edit
        </ActionBarPrimitive.Edit>
      )}
      {!far && <MoreActions name="More for this message" items={items} />}
    </ActionBarPrimitive.Root>
  );
};
