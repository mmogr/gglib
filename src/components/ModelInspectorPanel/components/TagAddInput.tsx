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
    <div className="mt-sm flex items-center gap-sm">
      <Input
        type="text"
        className="py-sm px-md bg-background-input border border-border rounded-base text-text text-sm cursor-pointer w-full flex-1 transition duration-200 focus:outline-none focus:border-border-focus"
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
