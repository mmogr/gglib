import { FC, KeyboardEvent } from 'react';

interface TagAddInputProps {
  value: string;
  onChange: (value: string) => void;
  onAdd: () => void;
}

/**
 * Input field with button to add a new tag.
 */
export const TagAddInput: FC<TagAddInputProps> = ({ value, onChange, onAdd }) => {
  const handleKeyPress = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      onAdd();
    }
  };

  return (
    <div className="tag-add-dropdown">
      <input
        type="text"
        className="tag-select"
        placeholder="Add tag..."
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyPress={handleKeyPress}
      />
      <button
        className="btn btn-secondary btn-sm"
        onClick={onAdd}
        disabled={!value.trim()}
      >
        Add
      </button>
    </div>
  );
};
