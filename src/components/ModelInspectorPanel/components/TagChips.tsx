import { FC } from 'react';
import { Chip } from '../../ui/Chip';

interface TagChipsProps {
  tags: string[];
  onRemoveTag: (tag: string) => void;
}

/**
 * Displays a list of tag chips with remove buttons.
 */
export const TagChips: FC<TagChipsProps> = ({ tags, onRemoveTag }) => {
  if (tags.length === 0) {
    return <p className="text-text-muted text-sm">No tags assigned</p>;
  }

  return (
    <div className="flex flex-wrap gap-sm">
      {tags.map(tag => (
        <Chip key={tag} onRemove={() => onRemoveTag(tag)} removeLabel={`Remove tag ${tag}`}>
          {tag}
        </Chip>
      ))}
    </div>
  );
};
