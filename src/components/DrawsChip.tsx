import { FC } from 'react';
import { Palette } from 'lucide-react';
import type { GgufModel } from '../types';
import { componentRoleLabel, imageFamilyLabel } from '../utils/imageFamily';
import { Chip } from './ui/Chip';
import { Icon } from './ui/Icon';

interface DrawsChipProps {
  /** The model's family and the roles it has no file linked for. */
  model: Pick<GgufModel, 'imageFamily'> & Partial<Pick<GgufModel, 'missingComponents'>>;
}

/**
 * Marks a model that draws images, by its family: "Draws · Flux.1". Neutral,
 * since the family is a fact about the model and not a state. Its title
 * names the roles still missing a file, as `gglib model list` does. Nothing
 * for a model that chats.
 */
export const DrawsChip: FC<DrawsChipProps> = ({ model }) => {
  if (!model.imageFamily) return null;
  const family = imageFamilyLabel(model.imageFamily);
  const missing = model.missingComponents ?? [];
  const title =
    missing.length > 0
      ? `Draws images (${family}); needs ${missing.map(componentRoleLabel).join(', ')}`
      : `Draws images (${family})`;
  return (
    <Chip size="sm" leftIcon={<Icon icon={Palette} size={11} />} title={title}>
      Draws · {family}
    </Chip>
  );
};
