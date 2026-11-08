import { FC, KeyboardEvent, useEffect, useRef, useState } from 'react';
import { Button } from '../../ui/Button';
import { Textarea } from '../../ui/Textarea';
import { DEFAULT_SYSTEM_PROMPT } from '../../../hooks/useGglibRuntime';
import type { ConversationSummary } from '../../../services/transport';

interface SystemPromptLineProps {
  conversation: ConversationSummary | null;
  /** Whether the editor is open: from the line's Edit, or the title's System prompt…. */
  editing: boolean;
  onEdit: () => void;
  onDone: () => void;
  onSave: (prompt: string | null) => Promise<void>;
}

/** Whether `conversation` has a system prompt of its own, not the default. */
function hasCustomPrompt(conversation: ConversationSummary | null): boolean {
  const prompt = conversation?.system_prompt?.trim();
  return !!prompt && prompt !== DEFAULT_SYSTEM_PROMPT;
}

/**
 * The system prompt above the first turn: nothing for the default, one
 * tertiary line "Custom system prompt · Edit" for a prompt of the chat's
 * own, and an inline editor (Reset to default, Cancel, Save) while editing.
 * The draft is local: the parent hears only a save.
 */
export const SystemPromptLine: FC<SystemPromptLineProps> = ({ conversation, editing, onEdit, onDone, onSave }) => {
  const systemPrompt = conversation?.system_prompt?.trim() || DEFAULT_SYSTEM_PROMPT;
  const [draft, setDraft] = useState(systemPrompt);
  const [saving, setSaving] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  // Each opening starts from what the chat has now.
  useEffect(() => {
    if (!editing) return;
    setDraft(systemPrompt);
    textareaRef.current?.focus();
    // Only on opening: a list read while typing must not undo the typing.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editing]);

  if (!editing) {
    if (!hasCustomPrompt(conversation)) return null;
    return (
      <p className="m-0 flex items-center gap-xs text-xs text-text-muted">
        <span>Custom system prompt</span>
        <span aria-hidden>·</span>
        <Button variant="link" size="sm" className="text-xs font-normal text-text-secondary" onClick={onEdit} title={systemPrompt}>
          Edit
        </Button>
      </p>
    );
  }

  const changed = draft.trim() !== systemPrompt;
  const save = async () => {
    if (!changed) {
      onDone();
      return;
    }
    setSaving(true);
    try {
      const trimmed = draft.trim();
      // The default is stored as none, so a chat reset to it shows nothing.
      await onSave(trimmed.length && trimmed !== DEFAULT_SYSTEM_PROMPT ? trimmed : null);
      onDone();
    } finally {
      setSaving(false);
    }
  };
  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      void save();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onDone();
    }
  };

  return (
    <section aria-label="System prompt" className="flex flex-col gap-sm">
      <Textarea
        ref={textareaRef}
        aria-label="System prompt"
        className="min-h-[80px] resize-y"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        placeholder={DEFAULT_SYSTEM_PROMPT}
        rows={4}
        onKeyDown={onKeyDown}
      />
      <div className="flex justify-between items-center gap-sm">
        <Button variant="ghost" size="sm" onClick={() => setDraft(DEFAULT_SYSTEM_PROMPT)}>
          Reset to default
        </Button>
        <div className="flex gap-sm">
          <Button variant="secondary" size="sm" onClick={onDone} disabled={saving}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" onClick={() => void save()} disabled={saving || !changed}>
            {saving ? 'Saving…' : 'Save'}
          </Button>
        </div>
      </div>
    </section>
  );
};
