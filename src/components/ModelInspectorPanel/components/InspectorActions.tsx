import { FC } from 'react';
import { Check, Pencil, Rocket, Square, Trash2, X } from 'lucide-react';
import { Icon } from '../../ui/Icon';
import { Button } from '../../ui/Button';

interface InspectorActionsProps {
  isRunning: boolean;
  isEditMode: boolean;
  onToggleServer: () => void;
  onEdit: () => void;
  onSave: () => void;
  onCancel: () => void;
  onDelete: () => void;
}

/**
 * Action button row for the model inspector.
 * Shows Start/Stop Endpoint, Edit/Save/Cancel, and Delete buttons.
 */
export const InspectorActions: FC<InspectorActionsProps> = ({
  isRunning,
  isEditMode,
  onToggleServer,
  onEdit,
  onSave,
  onCancel,
  onDelete,
}) => {
  return (
    <section className="mb-xl flex flex-col gap-base">
      <Button 
        variant={isRunning ? 'danger' : 'primary'}
        size="lg"
        onClick={onToggleServer}
        disabled={isEditMode}
        leftIcon={<Icon icon={isRunning ? Square : Rocket} size={16} />}
      >
        {isRunning ? 'Stop Endpoint' : 'Start Endpoint'}
      </Button>
      <div className="flex gap-md">
        {isEditMode ? (
          <>
            <Button 
              variant="primary"
              onClick={onSave}
              leftIcon={<Icon icon={Check} size={14} />}
            >
              Save
            </Button>
            <Button 
              variant="secondary"
              onClick={onCancel}
              leftIcon={<Icon icon={X} size={14} />}
            >
              Cancel
            </Button>
          </>
        ) : (
          <>
            <Button 
              variant="secondary"
              onClick={onEdit}
              leftIcon={<Icon icon={Pencil} size={14} />}
            >
              Edit
            </Button>
            <Button 
              variant="secondary"
              onClick={onDelete}
              leftIcon={<Icon icon={Trash2} size={14} />}
            >
              Delete
            </Button>
          </>
        )}
      </div>
    </section>
  );
};
