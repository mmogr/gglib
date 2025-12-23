import { FC, KeyboardEvent } from 'react';
import { Button } from '../../ui/Button';
import { Input } from '../../ui/Input';

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
      <Input
        type="text"
        className="tag-select"
        placeholder="Add tag..."
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyPress={handleKeyPress}
      />
      <Button
        variant="secondary"
        size="sm"
        onClick={onAdd}
        disabled={!value.trim()}
      >
        Add
      </Button>
    </div>
  );
};
