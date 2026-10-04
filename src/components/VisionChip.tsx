import { FC } from 'react';
import { Eye } from 'lucide-react';
import { Chip } from './ui/Chip';
import { Icon } from './ui/Icon';

/**
 * Marks a model that reads images: one linked to a projector. Neutral, since
 * it is a fact about the model and not a state.
 */
export const VisionChip: FC = () => (
  <Chip size="sm" leftIcon={<Icon icon={Eye} size={11} />} title="Reads images: it is linked to a projector">
    Vision
  </Chip>
);
