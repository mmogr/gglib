import { FC } from 'react';
import { CloudSync, Shield } from 'lucide-react';
import type { GgufModel } from '../../../types';
import { canSee } from '../../../utils/canSee';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { Input } from '../../ui/Input';
import { VisionChip } from '../../VisionChip';
import { DrawsChip } from '../../DrawsChip';

interface InspectorHeaderProps {
  /** The model shown: its name, whether it reads images, and the family of one that draws them. */
  model: Pick<GgufModel, 'name' | 'imageInput' | 'imageFamily'> & Partial<Pick<GgufModel, 'missingComponents'>>;
  /** Whether the model has a HuggingFace repo to check for updates against. */
  hasHfRepo: boolean;
  isEditMode: boolean;
  editedName: string;
  onEditedNameChange: (name: string) => void;
  onVerify: () => void;
  onCheckUpdates: () => void;
}

/**
 * Inspector title bar: model name (or its edit field), the Vision chip on a
 * model that reads images, the Draws chip on one that draws them, plus the
 * two secondary maintenance actions.
 */
export const InspectorHeader: FC<InspectorHeaderProps> = ({
  model,
  hasHfRepo,
  isEditMode,
  editedName,
  onEditedNameChange,
  onVerify,
  onCheckUpdates,
}) => (
  <div className="p-md border-b border-border-light shrink-0">
    <div className="flex items-center justify-between gap-base w-full">
      {isEditMode ? (
        <Input
          type="text"
          className="w-full m-0 text-lg font-semibold"
          value={editedName}
          onChange={(e) => onEditedNameChange(e.target.value)}
          placeholder="Model name"
        />
      ) : (
        <div className="flex items-center gap-sm min-w-0">
          <h2 className="m-0 text-lg font-semibold truncate">{model.name}</h2>
          {canSee(model) && <VisionChip />}
          <DrawsChip model={model} />
        </div>
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
