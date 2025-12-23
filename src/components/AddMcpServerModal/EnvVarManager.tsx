import { FC } from "react";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Icon } from "../ui/Icon";
import { Plus, X } from "lucide-react";
import styles from "../AddMcpServerModal.module.css";

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
    <div className={styles.section}>
      <div className={styles.sectionHeader}>
        <label className={styles.label}>Environment Variables</label>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className={styles.addBtn}
          onClick={onAdd}
          disabled={disabled}
        >
          <Icon icon={Plus} size={14} />
          Add variable
        </Button>
      </div>
      {envVars.length === 0 ? (
        <p className={styles.hint}>
          Add environment variables for API keys and secrets
        </p>
      ) : (
        <div className={styles.envVars}>
          {envVars.map(([key, value], index) => (
            <div key={index} className={styles.envRow}>
              <Input
                type="text"
                className={styles.envKey}
                value={key}
                onChange={(e) => onUpdate(index, 0, e.target.value)}
                placeholder="KEY"
                disabled={disabled}
              />
              <Input
                type="password"
                className={styles.envValue}
                value={value}
                onChange={(e) => onUpdate(index, 1, e.target.value)}
                placeholder="value"
                disabled={disabled}
              />
              <button
                type="button"
                className={styles.envRemove}
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
    </div>
  );
};
