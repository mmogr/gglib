import { FC } from 'react';
import { Button } from '../../ui/Button';
import { TagChips } from './TagChips';
import { TagAddInput } from './TagAddInput';

interface InspectorTagsProps {
  tags: string[];
  newTagInput: string;
  onNewTagInputChange: (value: string) => void;
  onAddTag: () => void;
  onRemoveTag: (tag: string) => void;
  /** Re-run capability detection; `full` rebuilds the system-tag namespace. */
  onRetag: (full: boolean) => void;
  retagging: boolean;
}

/**
 * Tag list plus the add-tag input, under a section heading.
 *
 * Tags drive launch flags and response parsing (`reasoning` selects the
 * reasoning format, `format:*` the chat dialect), which is why Rebuild is
 * destructive: it re-derives that whole namespace from the GGUF and drops
 * anything in it that detection no longer produces, including tags added by
 * hand. It is styled and confirmed as a destructive action for that reason.
 */
export const InspectorTags: FC<InspectorTagsProps> = ({
  tags,
  newTagInput,
  onNewTagInputChange,
  onAddTag,
  onRemoveTag,
  onRetag,
  retagging,
}) => (
  <section className="mb-xl">
    <div className="flex items-center justify-between gap-md mb-xs">
      <h3 className="m-0 text-sm font-semibold text-text">Tags</h3>
      <div className="flex gap-xs">
        <Button
          variant="ghost"
          size="sm"
          disabled={retagging}
          onClick={() => onRetag(false)}
        >
          Re-detect
        </Button>
        <Button
          variant="dangerGhost"
          size="sm"
          disabled={retagging}
          onClick={() => onRetag(true)}
        >
          Rebuild
        </Button>
      </div>
    </div>
    <p className="m-0 mb-base text-xs text-text-muted">
      Re-detect adds missing capability tags and removes nothing. Rebuild re-derives them from
      the GGUF, dropping detected tags it no longer finds — including ones you added by hand,
      such as a manual <code className="font-mono">reasoning</code> tag.
    </p>
    <div className="flex flex-col gap-base">
      <TagChips tags={tags} onRemoveTag={onRemoveTag} />
      <TagAddInput value={newTagInput} onChange={onNewTagInputChange} onAdd={onAddTag} />
    </div>
  </section>
);
