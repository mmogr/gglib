import { FC, KeyboardEvent, useEffect, useMemo, useRef, useState } from 'react';
import { Button } from '../../ui/Button';
import { Textarea } from '../../ui/Textarea';
import { DEFAULT_SYSTEM_PROMPT } from '../../../hooks/useGglibRuntime';
import type { ConversationSummary } from '../../../services/transport';

interface SystemPromptSectionProps {
  conversation: ConversationSummary | null;
  onSave: (prompt: string | null) => Promise<void>;
}

/**
 * System prompt card: a clamped preview that expands into an editor.
 *
 * Draft state is local — the parent only hears about it on save.
 */
export const SystemPromptSection: FC<SystemPromptSectionProps> = ({ conversation, onSave }) => {
  const [isEditing, setIsEditing] = useState(false);
  const [draft, setDraft] = useState(DEFAULT_SYSTEM_PROMPT);
  const [saving, setSaving] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  const conversationId = conversation?.id ?? null;
  const systemPrompt = conversation?.system_prompt ?? null;

  // Keep the draft in step with the conversation while not editing.
  useEffect(() => {
    if (!isEditing) {
      setDraft(systemPrompt ?? DEFAULT_SYSTEM_PROMPT);
    }
  }, [systemPrompt, isEditing]);

  useEffect(() => {
    if (isEditing) {
      textareaRef.current?.focus();
    }
  }, [isEditing]);

  // Switching conversations abandons any in-progress edit.
  useEffect(() => {
    setIsEditing(false);
    setSaving(false);
  }, [conversationId]);

  const preview = useMemo(
    () => systemPrompt?.trim() || DEFAULT_SYSTEM_PROMPT,
    [systemPrompt],
  );

  const hasChanges = useMemo(
    () => draft.trim() !== preview,
    [draft, preview],
  );

  const handleSave = async () => {
    if (!hasChanges) {
      setIsEditing(false);
      return;
    }
    setSaving(true);
    try {
      const trimmed = draft.trim();
      await onSave(trimmed.length ? trimmed : null);
      setIsEditing(false);
    } finally {
      setSaving(false);
    }
  };

  const handleCancel = () => {
    setIsEditing(false);
    setDraft(systemPrompt ?? DEFAULT_SYSTEM_PROMPT);
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      handleSave();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      handleCancel();
    }
  };

  return (
    <section className="border border-border rounded-base p-md bg-background flex flex-col gap-sm shrink-0">
      <div className="flex justify-between gap-md items-start">
        <div>
          <p className="text-xs uppercase tracking-[1px] text-text-muted m-0 mb-xs">System prompt</p>
          {!isEditing && (
            <p className="m-0 text-text text-sm leading-[1.5] line-clamp-2">{preview}</p>
          )}
        </div>
        <div className="flex gap-sm items-center shrink-0">
          {isEditing ? (
            <span className="text-xs text-primary">Editing…</span>
          ) : (
            <Button
              variant="secondary"
              size="sm"
              onClick={() => {
                setDraft(systemPrompt ?? DEFAULT_SYSTEM_PROMPT);
                setIsEditing(true);
              }}
              disabled={!conversation}
            >
              Edit
            </Button>
          )}
        </div>
      </div>
      {isEditing && (
        <>
          <Textarea
            ref={textareaRef}
            className="w-full p-sm border border-border rounded-sm bg-surface text-text text-sm font-[inherit] resize-y min-h-[80px] focus:outline-none focus:border-primary"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={DEFAULT_SYSTEM_PROMPT}
            rows={4}
            onKeyDown={handleKeyDown}
          />
          <div className="flex justify-between items-center gap-sm">
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setDraft(DEFAULT_SYSTEM_PROMPT)}
            >
              Reset
            </Button>
            <div className="flex gap-sm">
              <Button
                variant="secondary"
                size="sm"
                onClick={handleCancel}
                disabled={saving}
              >
                Cancel
              </Button>
              <Button
                variant="primary"
                size="sm"
                onClick={handleSave}
                disabled={saving || !hasChanges}
              >
                {saving ? 'Saving…' : 'Save'}
              </Button>
            </div>
          </div>
        </>
      )}
    </section>
  );
};
