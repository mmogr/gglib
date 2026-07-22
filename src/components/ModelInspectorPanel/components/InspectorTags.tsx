import { FC } from 'react';
import { TagChips } from './TagChips';
import { TagAddInput } from './TagAddInput';

interface InspectorTagsProps {
  tags: string[];
  newTagInput: string;
  onNewTagInputChange: (value: string) => void;
  onAddTag: () => void;
  onRemoveTag: (tag: string) => void;
}

/**
 * Tag list plus the add-tag input, under a section heading.
 */
export const InspectorTags: FC<InspectorTagsProps> = ({
  tags,
  newTagInput,
  onNewTagInputChange,
  onAddTag,
  onRemoveTag,
}) => (
  <section className="mb-xl">
    <h3 className="m-0 mb-base text-xs font-semibold text-text-secondary uppercase tracking-[0.05em]">
      Tags
    </h3>
    <div className="flex flex-col gap-base">
      <TagChips tags={tags} onRemoveTag={onRemoveTag} />
      <TagAddInput value={newTagInput} onChange={onNewTagInputChange} onAdd={onAddTag} />
    </div>
  </section>
);
