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
    return <p className="text-text-muted text-sm">No tags assigned</p>;
  }

  return (
    <div className="flex flex-wrap gap-sm">
      {tags.map(tag => (
        <div key={tag} className="inline-flex items-center gap-sm py-xs px-md border border-border rounded-lg text-sm bg-background text-text">
          {tag}
          <button
            className="bg-transparent border-none text-text cursor-pointer text-lg leading-none p-0 m-0 opacity-70 transition duration-200 hover:opacity-100 hover:text-danger"
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
