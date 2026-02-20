import { FC } from "react";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Icon } from "../ui/Icon";
import { Plus, X } from "lucide-react";
import { Stack, Label } from '../primitives';

interface EnvVarManagerProps {
  envVars: [string, string][];
  onAdd: () => void;
  onRemove: (index: number) => void;
  onUpdate: (index: number, field: 0 | 1, value: string) => void;
  disabled: boolean;
}

export const EnvVarManager: FC<EnvVarManagerProps> = ({
  envVars,
  onAdd,
  onRemove,
  onUpdate,
  disabled,
}) => {
  return (
    <Stack gap="xs">
      <div className="flex justify-between items-center">
        <Label size="sm">Environment Variables</Label>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="px-0 text-xs text-primary hover:text-primary"
          onClick={onAdd}
          disabled={disabled}
        >
          <Icon icon={Plus} size={14} />
          Add variable
        </Button>
      </div>
      {envVars.length === 0 ? (
        <p className="text-xs text-text-secondary">
          Add environment variables for API keys and secrets
        </p>
      ) : (
        <div className="flex flex-col gap-sm">
          {envVars.map(([key, value], index) => (
            <div key={index} className="flex gap-sm items-center">
              <Input
                type="text"
                className="flex-1 font-mono"
                value={key}
                onChange={(e) => onUpdate(index, 0, e.target.value)}
                placeholder="KEY"
                disabled={disabled}
              />
              <Input
                type="password"
                className="flex-[2]"
                value={value}
                onChange={(e) => onUpdate(index, 1, e.target.value)}
                placeholder="value"
                disabled={disabled}
              />
              <button
                type="button"
                className="flex items-center justify-center w-6 h-6 bg-none border-none text-[1.25rem] text-[#6b7280] cursor-pointer rounded-[0.25rem] hover:bg-[rgba(239,68,68,0.15)] hover:text-[#ef4444]"
                onClick={() => onRemove(index)}
                disabled={disabled}
                aria-label="Remove variable"
              >
                <Icon icon={X} size={14} />
              </button>
            </div>
          ))}
        </div>
      )}
    </Stack>
  );
};
