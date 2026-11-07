/**
 * Details › Tags: the model's tags as chips, each with ×, and a field that
 * adds one on Enter or Add. Re-detecting and rebuilding the detected tags
 * are the ⋯ menu's.
 *
 * @module inspector/local/Tags
 */

import type { FC } from 'react';
import { Button } from '../../components/ui/Button';
import { Chip } from '../../components/ui/Chip';
import { Input } from '../../components/ui/Input';

interface TagsProps {
  tags: string[];
  draft: string;
  onDraft: (value: string) => void;
  onAdd: () => void;
  onRemove: (tag: string) => void;
}

export const Tags: FC<TagsProps> = ({ tags, draft, onDraft, onAdd, onRemove }) => (
  <div className="flex flex-col gap-sm">
    {tags.length > 0 && (
      <div className="flex flex-wrap gap-xs">
        {tags.map((tag) => (
          <Chip key={tag} onRemove={() => onRemove(tag)} removeLabel={`Remove tag ${tag}`}>
            {tag}
          </Chip>
        ))}
      </div>
    )}
    <div className="flex items-center gap-xs">
      <Input
        size="sm"
        aria-label="Add a tag"
        placeholder="tag"
        value={draft}
        onChange={(e) => onDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') onAdd();
        }}
        className="min-w-0 flex-1"
      />
      <Button variant="ghost" size="sm" onClick={onAdd} disabled={!draft.trim()}>
        Add
      </Button>
    </div>
  </div>
);
