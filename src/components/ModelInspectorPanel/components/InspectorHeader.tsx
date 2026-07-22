import { FC } from 'react';
import { CloudSync, Shield } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { Input } from '../../ui/Input';

interface InspectorHeaderProps {
  modelName: string;
  /** Whether the model has a HuggingFace repo to check for updates against. */
  hasHfRepo: boolean;
  isEditMode: boolean;
  editedName: string;
  onEditedNameChange: (name: string) => void;
  onVerify: () => void;
  onCheckUpdates: () => void;
}

/**
 * Inspector title bar: model name (or its edit field) plus the two
 * secondary maintenance actions.
 */
export const InspectorHeader: FC<InspectorHeaderProps> = ({
  modelName,
  hasHfRepo,
  isEditMode,
  editedName,
  onEditedNameChange,
  onVerify,
  onCheckUpdates,
}) => (
  <div className="p-base border-b border-border bg-background shrink-0">
    <div className="flex items-center justify-between gap-base w-full">
      {isEditMode ? (
        <Input
          type="text"
          className="w-full m-0 text-xl font-semibold"
          value={editedName}
          onChange={(e) => onEditedNameChange(e.target.value)}
          placeholder="Model name"
        />
      ) : (
        <h2 className="m-0 text-xl font-semibold truncate">{modelName}</h2>
      )}

      {!isEditMode && (
        <div className="flex items-center gap-xs shrink-0">
          <Button
            variant="ghost"
            iconOnly
            className="rounded-full"
            onClick={onVerify}
            title="Verify model integrity"
            aria-label="Verify model integrity"
          >
            <Icon icon={Shield} size={16} />
          </Button>
          <Button
            variant="ghost"
            iconOnly
            className="rounded-full"
            onClick={onCheckUpdates}
            disabled={!hasHfRepo}
            title={hasHfRepo ? 'Check for updates on HuggingFace' : 'No HuggingFace repo linked'}
            aria-label="Check for updates"
          >
            <Icon icon={CloudSync} size={16} />
          </Button>
        </div>
      )}
    </div>
  </div>
);
