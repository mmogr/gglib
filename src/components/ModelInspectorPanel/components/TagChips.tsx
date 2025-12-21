import { FC } from 'react';

interface TagChipsProps {
  tags: string[];
  onRemoveTag: (tag: string) => void;
}

/**
 * Displays a list of tag chips with remove buttons.
 */
export const TagChips: FC<TagChipsProps> = ({ tags, onRemoveTag }) => {
  if (tags.length === 0) {
    return <p className="text-muted">No tags assigned</p>;
  }

  return (
    <div className="tag-chips">
      {tags.map(tag => (
        <div key={tag} className="tag-chip">
          {tag}
          <button
            className="tag-remove"
            onClick={() => onRemoveTag(tag)}
            title="Remove tag"
          >
            ×
          </button>
        </div>
      ))}
    </div>
  );
};
