import { FC } from 'react';
import { PackageOpen } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { EmptyState } from '../../primitives';

/**
 * Shown when no model is selected in the library panel.
 */
export const InspectorEmptyState: FC = () => (
  <div className="flex-1 min-h-0 flex flex-col items-center justify-center">
    <EmptyState
      icon={<Icon icon={PackageOpen} size={40} />}
      title="No model selected"
      description="Pick a model from the library to see its metadata, tags, and inference defaults."
    />
  </div>
);
