import { FC } from 'react';
import { Check, Pencil, Rocket, Square, Trash2, X } from 'lucide-react';
import { Icon } from '../../ui/Icon';

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
    <section className="inspector-section actions-section">
      <button 
        className={`btn btn-lg ${isRunning ? 'btn-danger' : 'btn-primary'}`}
        onClick={onToggleServer}
        disabled={isEditMode}
      >
        <span className="inline-flex items-center gap-2">
          <Icon icon={isRunning ? Square : Rocket} size={16} />
          {isRunning ? 'Stop Endpoint' : 'Start Endpoint'}
        </span>
      </button>
      <div className="secondary-actions">
        {isEditMode ? (
          <>
            <button className="btn btn-primary" onClick={onSave}>
              <span className="inline-flex items-center gap-2">
                <Icon icon={Check} size={14} />
                Save
              </span>
            </button>
            <button className="btn btn-secondary" onClick={onCancel}>
              <span className="inline-flex items-center gap-2">
                <Icon icon={X} size={14} />
                Cancel
              </span>
            </button>
          </>
        ) : (
          <>
            <button className="btn btn-secondary" onClick={onEdit}>
              <span className="inline-flex items-center gap-2">
                <Icon icon={Pencil} size={14} />
                Edit
              </span>
            </button>
            <button className="btn btn-secondary" onClick={onDelete}>
              <span className="inline-flex items-center gap-2">
                <Icon icon={Trash2} size={14} />
                Delete
              </span>
            </button>
          </>
        )}
      </div>
    </section>
  );
};
