import React from 'react';
import { Button } from '../ui/Button';
import { Textarea } from '../ui/Textarea';
import { DEFAULT_SYSTEM_PROMPT } from '../../hooks/useGglibRuntime';
import type { ConversationSummary } from '../../services/clients/chat';

interface SystemPromptCardProps {
  activeConversation: ConversationSummary | null;
  isEditingPrompt: boolean;
  setIsEditingPrompt: (editing: boolean) => void;
  systemPromptDraft: string;
  setSystemPromptDraft: (draft: string) => void;
  promptPreview: string;
  promptHasChanges: boolean;
  savingSystemPrompt: boolean;
  onSave: () => void;
  onKeyDown: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  promptTextareaRef: React.RefObject<HTMLTextAreaElement | null>;
}

export const SystemPromptCard: React.FC<SystemPromptCardProps> = ({
  activeConversation,
  isEditingPrompt,
  setIsEditingPrompt,
  systemPromptDraft,
  setSystemPromptDraft,
  promptPreview,
  promptHasChanges,
  savingSystemPrompt,
  onSave,
  onKeyDown,
  promptTextareaRef,
}) => (
  <section className="border border-border rounded-base p-md bg-background flex flex-col gap-sm shrink-0">
    <div className="flex justify-between gap-md items-start">
      <div>
        <p className="text-xs uppercase tracking-[1px] text-text-muted m-0 mb-xs">System prompt</p>
        {!isEditingPrompt && (
          <p className="m-0 text-text text-sm leading-[1.5] line-clamp-2">{promptPreview}</p>
        )}
      </div>
      <div className="flex gap-sm items-center shrink-0">
        {isEditingPrompt ? (
          <span className="text-xs text-primary">Editing…</span>
        ) : (
          <Button
            variant="secondary"
            size="sm"
            onClick={() => {
              setSystemPromptDraft(activeConversation?.system_prompt ?? DEFAULT_SYSTEM_PROMPT);
              setIsEditingPrompt(true);
            }}
            disabled={!activeConversation}
          >
            Edit
          </Button>
        )}
      </div>
    </div>
    {isEditingPrompt && (
      <>
        <Textarea
          ref={promptTextareaRef}
          className="w-full p-sm border border-border rounded-sm bg-surface text-text text-sm font-[inherit] resize-y min-h-[80px] focus:outline-none focus:border-primary"
          value={systemPromptDraft}
          onChange={(e) => setSystemPromptDraft(e.target.value)}
          placeholder={DEFAULT_SYSTEM_PROMPT}
          rows={4}
          onKeyDown={onKeyDown}
        />
        <div className="flex justify-between items-center gap-sm">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setSystemPromptDraft(DEFAULT_SYSTEM_PROMPT)}
          >
            Reset
          </Button>
          <div className="flex gap-sm">
            <Button
              variant="secondary"
              size="sm"
              onClick={() => {
                setIsEditingPrompt(false);
                setSystemPromptDraft(activeConversation?.system_prompt ?? DEFAULT_SYSTEM_PROMPT);
              }}
              disabled={savingSystemPrompt}
            >
              Cancel
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={onSave}
              disabled={savingSystemPrompt || !promptHasChanges}
            >
              {savingSystemPrompt ? 'Saving…' : 'Save'}
            </Button>
          </div>
        </div>
      </>
    )}
  </section>
);
