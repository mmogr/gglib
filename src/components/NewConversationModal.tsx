import { FC } from 'react';

import { Button } from './ui/Button';
import { Input } from './ui/Input';
import { Textarea } from './ui/Textarea';
import { DEFAULT_SYSTEM_PROMPT } from '../hooks/useGglibRuntime';

interface NewConversationModalProps {
  title: string;
  onTitleChange: (title: string) => void;
  systemPrompt: string;
  onSystemPromptChange: (prompt: string) => void;
  creating: boolean;
  onCancel: () => void;
  onCreate: () => void;
}

/**
 * Title and system prompt for a conversation that does not exist yet.
 *
 * Fully controlled: the page owns the draft, because the same two values are
 * what it posts on create. Dismissing by backdrop is refused while the create
 * is in flight, so the request cannot outlive the form that describes it.
 */
export const NewConversationModal: FC<NewConversationModalProps> = ({
  title,
  onTitleChange,
  systemPrompt,
  onSystemPromptChange,
  creating,
  onCancel,
  onCreate,
}) => (
  <div
    className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-modal-backdrop"
    onMouseDown={(e) => e.target === e.currentTarget && !creating && onCancel()}
  >
    <div className="bg-surface border border-border rounded-lg p-xl w-[min(450px,90vw)] max-h-[90vh] overflow-y-auto flex flex-col gap-md">
      <h3 className="text-lg font-semibold m-0">Start a new chat</h3>
      <label className="flex flex-col gap-xs text-sm text-text-muted">
        Title
        <Input
          className="py-sm px-md border border-border rounded-sm bg-background text-text text-sm focus:outline-none focus:border-primary"
          value={title}
          onChange={(e) => onTitleChange(e.target.value)}
          placeholder="New Chat"
        />
      </label>
      <label className="flex flex-col gap-xs text-sm text-text-muted">
        System Prompt
        <Textarea
          className="py-sm px-md border border-border rounded-sm bg-background text-text text-sm font-[inherit] resize-y min-h-[100px] focus:outline-none focus:border-primary"
          value={systemPrompt}
          onChange={(e) => onSystemPromptChange(e.target.value)}
          placeholder={DEFAULT_SYSTEM_PROMPT}
          rows={4}
        />
      </label>
      <p className="text-xs text-text-muted m-0">
        The system prompt steers the assistant's behavior for the entire conversation.
      </p>
      <div className="flex justify-end gap-sm mt-sm">
        <Button type="button" variant="secondary" onClick={onCancel} disabled={creating}>
          Cancel
        </Button>
        <Button type="button" variant="primary" onClick={onCreate} disabled={creating}>
          {creating ? 'Creating…' : 'Create chat'}
        </Button>
      </div>
    </div>
  </div>
);
